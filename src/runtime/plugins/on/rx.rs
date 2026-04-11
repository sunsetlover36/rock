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
}

#[derive(Clone)]
pub(super) struct OnRx {
    event_key: GameModeEventKey,
    scope: EventScope,
    name: Option<String>,
    priority: u32,
    pipeline: RxPipeline,
}
impl OnRx {
    pub fn new(event_key: GameModeEventKey, scope: EventScope) -> Self {
        Self {
            event_key,
            scope,
            name: None,
            priority: 0,
            pipeline: RxPipeline::default(),
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
        })
    }

    fn add_event_listener(
        &self,
        lua: &mlua::Lua,
        handle: mlua::Function,
    ) -> mlua::Result<SequenceId> {
        let event_key = self.event_key;

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
            }));
            entry.sort_by(|a, b| b.priority.cmp(&a.priority));
        }

        // Layer garbage collection
        let layers = get_app_data::<app_data::ActiveLayers>(lua)?;
        if let Some(layer) = layers.0.last() {
            let cleaner = lua.create_function(move |lua, _: ()| {
                get_app_data_mut::<app_data::EventListeners>(lua)?
                    .0
                    .entry(event_key)
                    .and_modify(|listeners| listeners.retain(|l| l.get_seq() != current_seq));

                Ok(())
            })?;

            get_app_data_mut::<app_data::LayerRegistry>(lua)?
                .layers
                .entry(layer.to_owned())
                .and_modify(|l| l.cleaners.push(cleaner));
        }

        Ok(current_seq)
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
            let seq = this.add_event_listener(lua, handle)?;
            Ok(ListenerHandle {
                event_key: this.event_key,
                name: this.name.clone(),
                seq,
            })
        });
    }
}
