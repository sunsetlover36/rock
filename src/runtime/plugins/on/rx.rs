use mlua::UserData;

use super::{
    GameModeListener,
    handle::ListenerHandle,
    protocol::{EventScope, GameModeEventKey, GameModeListenerParams, PlayerEventKey},
};
use crate::{
    runtime::{
        app_data::{self, ExecutionContext},
        event_bus::SequenceId,
        plugins::{on::protocol::TimerEventKey, player::PlayerHandle},
        utils::{get_app_data, get_app_data_mut},
    },
    rx::{
        HasPipeline, RxPipeline, RxSentry,
        core::add_core_pipeline_methods,
        operator::{RxOp, add_op_pipeline_methods},
    },
};

struct NewListenerParams {
    handle: mlua::Function,
    seq: SequenceId,
    context: ExecutionContext,
    owner: Option<crate::runtime::plugins::layer::LayerId>,
    active: crate::runtime::lifetime::Cancellation,
    players: Vec<rock_wire::PlayerKey>,
}

#[derive(Clone)]
pub(super) struct OnRx {
    event_key: GameModeEventKey,
    scope: EventScope,
    name: Option<String>,
    priority: u32,
    pipeline: RxPipeline,
    players: Vec<rock_wire::PlayerKey>,
}
impl OnRx {
    pub fn new(event_key: GameModeEventKey, scope: EventScope) -> Self {
        Self {
            event_key,
            scope,
            name: None,
            priority: 0,
            pipeline: RxPipeline::default(),
            players: Vec::new(),
        }
    }

    fn construct_listener(&self, params: NewListenerParams) -> GameModeListener {
        let builder = self.clone();
        GameModeListener::new(GameModeListenerParams {
            name: builder.name,
            created_at_seq: params.seq,
            scope: self.scope,
            context: params.context,
            handle: params.handle,
            priority: self.priority,
            rx_sentry: RxSentry::new(builder.pipeline),
            owner: params.owner,
            active: params.active,
            players: params.players,
        })
    }

    fn add_event_listener(
        &self,
        lua: &mlua::Lua,
        handle: mlua::Function,
    ) -> mlua::Result<(SequenceId, crate::runtime::lifetime::Cancellation)> {
        let event_key = self.event_key;
        let owner = crate::runtime::plugins::layer::current_layer(lua)?;
        let work_context = crate::runtime::lifetime::WorkContext::capture(lua, &self.players)?;
        let active = crate::runtime::lifetime::Cancellation::new();

        let current_seq = get_app_data::<app_data::EventBus>(lua)?
            .0
            .increment_sequence();
        let context = *get_app_data::<app_data::ExecutionContext>(lua)?;
        {
            let mut listeners = get_app_data_mut::<app_data::EventListeners>(lua)?;
            let entry = listeners.0.entry(event_key).or_default();

            if let Some(name) = &self.name
                && entry.iter().any(|l| l.name.as_ref() == Some(name))
            {
                return Err(mlua::Error::runtime(format!(
                    "Event listener with name `{}` already exists!",
                    name
                )));
            }

            entry.push(self.construct_listener(NewListenerParams {
                handle,
                seq: current_seq,
                context,
                owner,
                active: active.clone(),
                players: work_context.players.clone(),
            }));
            entry.sort_by(|a, b| b.priority.cmp(&a.priority));
        }

        // Layer garbage collection
        if let Some(layer) = owner {
            let active = active.clone();
            let cleaner = lua.create_function(move |lua, _: ()| {
                active.cancel();
                get_app_data_mut::<app_data::EventListeners>(lua)?
                    .0
                    .entry(event_key)
                    .and_modify(|listeners| listeners.retain(|l| l.get_seq() != current_seq));

                Ok(())
            })?;

            get_app_data_mut::<app_data::LayerRegistry>(lua)?
                .layers
                .entry(layer)
                .and_modify(|l| l.cleaners.push(cleaner));
        }

        crate::runtime::lifetime::register_players(lua, &work_context.players, &active);
        Ok((current_seq, active))
    }
}

impl HasPipeline for OnRx {
    fn pipeline(&self) -> &RxPipeline {
        &self.pipeline
    }
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.pipeline
    }
}

impl UserData for OnRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_core_pipeline_methods(methods);
        add_op_pipeline_methods(methods);

        methods.add_method(
            "while_connected",
            |_, this, player: mlua::UserDataRef<PlayerHandle>| {
                let mut next = this.clone();
                if !next.players.contains(&player.key()) {
                    next.players.push(player.key());
                }
                Ok(next)
            },
        );

        methods.add_method("name", |_, this, name: String| {
            let mut next = this.clone();
            next.name = Some(name);
            Ok(next)
        });

        methods.add_method("priority", |_, this, priority: u32| {
            let mut next = this.clone();
            next.priority = priority;
            Ok(next)
        });

        // DSL sugar for input events -> where + select
        methods.add_method("bind_action", |lua, this, event_name: String| {
            if this.event_key != GameModeEventKey::Player(PlayerEventKey::Input) {
                return Err(mlua::Error::external(
                    "Method `:bind_action()` can only be used with 'on.player.input' events",
                ));
            }

            let mut next = this.clone();
            let predicate = RxOp::Filter(lua.create_function(
                move |_, (_, action_table): (mlua::AnyUserData, mlua::Table)| {
                    let action_name: String = action_table.get("name")?;
                    Ok(action_name == event_name)
                },
            )?);
            next.pipeline_mut().add_operator(predicate);

            let map = RxOp::Map(lua.create_function(
                |_, (ud, action_table): (mlua::AnyUserData, mlua::Table)| {
                    let player = ud.borrow::<PlayerHandle>()?;
                    let data: mlua::Value = action_table.get("data")?;
                    Ok((player.clone(), data))
                },
            )?);
            next.pipeline_mut().add_operator(map);

            Ok(next)
        });

        // DSL sugar for timers -> named filter
        methods.add_method("named", |lua, this, name: String| {
            if this.event_key != GameModeEventKey::Timer(TimerEventKey::Fire) {
                return Err(mlua::Error::external(
                    "Method `:named()` can only be used with 'on.timer.fire' events",
                ));
            }

            let mut next = this.clone();
            let predicate =
                RxOp::Filter(lua.create_function(move |_, args: (String, mlua::Value)| {
                    let timer_id = args.0;
                    Ok(timer_id == name)
                })?);
            next.pipeline_mut().add_operator(predicate);

            let map = RxOp::Map(lua.create_function(|_, args: (String, mlua::Value)| Ok(args.1))?);
            next.pipeline_mut().add_operator(map);

            Ok(next)
        });

        methods.add_method("each", |lua, this, handle| {
            let (seq, active) = this.add_event_listener(lua, handle)?;
            Ok(ListenerHandle {
                event_key: this.event_key,
                name: this.name.clone(),
                seq,
                active,
            })
        });
    }
}
