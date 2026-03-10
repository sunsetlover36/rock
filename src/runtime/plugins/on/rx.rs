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
        utils::{get_app_data, get_app_data_mut},
    },
    rx::{
        CoreRxPipeline, HasCoreRxPipeline, add_core_rx_methods,
        operator::{HasOpRxPipeline, OpRxPipeline, RxOp, add_op_rx_methods},
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
    core_pipeline: CoreRxPipeline,
    op_pipeline: OpRxPipeline,
}
impl OnRx {
    pub fn new(event_key: GameModeEventKey, scope: EventScope) -> Self {
        Self {
            event_key,
            scope,
            name: None,
            priority: 0,
            core_pipeline: CoreRxPipeline::default(),
            op_pipeline: OpRxPipeline::default(),
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
            core_pipeline: self.core_pipeline.clone(),
            op_pipeline: self.op_pipeline.clone(),
        })
    }

    fn add_event_listener(
        &self,
        lua: &mlua::Lua,
        handle: mlua::Function,
    ) -> mlua::Result<SequenceId> {
        let event_key = self.event_key;

        let current_seq = get_app_data::<app_data::EventBus>(lua)?.increment_sequence();
        let context = *get_app_data::<app_data::ExecutionContext>(lua)?;
        {
            let mut listeners = get_app_data_mut::<app_data::EventListeners>(lua)?;
            let entry = listeners.entry(event_key).or_default();

            if let Some(name) = &self.name
                && entry.iter().any(|l| l.name.as_ref() == Some(name))
            {
                return Err(mlua::Error::runtime(format!(
                    "Event listener with name `{}` already exists!",
                    name
                )));
            } else {
                entry.push(self.construct_listener(NewListenerParams {
                    handle,
                    seq: current_seq,
                    context,
                }));
            }

            entry.sort_by(|a, b| b.priority.cmp(&a.priority));
        }

        // Layer garbage collection
        let layers = get_app_data::<app_data::ActiveLayers>(lua)?;
        if let Some(layer) = layers.last() {
            let cleaner = lua.create_function(move |lua, _: ()| {
                get_app_data_mut::<app_data::EventListeners>(lua)?
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

impl HasCoreRxPipeline for OnRx {
    fn core_pipeline_mut(&mut self) -> &mut CoreRxPipeline {
        &mut self.core_pipeline
    }
}
impl HasOpRxPipeline for OnRx {
    fn op_pipeline_mut(&mut self) -> &mut OpRxPipeline {
        &mut self.op_pipeline
    }
}

impl UserData for OnRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_core_rx_methods(methods);
        add_op_rx_methods(methods);

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

        // DSL sugar -> where + select for input events
        methods.add_method("bind_action", |lua, this, event_name: String| {
            if this.event_key != GameModeEventKey::Player(PlayerEventKey::Input) {
                return Err(mlua::Error::external(
                    "Method `:bind_action()` can only be used with 'on.player.input' events",
                ));
            }

            let mut next = this.clone();
            let predicate = RxOp::Filter(lua.create_function(
                move |_, args: (SequenceId, mlua::Table)| {
                    let action_table: mlua::Table = args.1;
                    let action_name: String = action_table.get("name")?;
                    Ok(action_name == event_name)
                },
            )?);
            next.op_pipeline_mut().operators.push(predicate);

            let map = RxOp::Map(lua.create_function(
                move |_, args: (SequenceId, mlua::Table)| {
                    let pid = args.0;
                    let action_table = args.1;
                    let data: mlua::Value = action_table.get("data")?;
                    Ok((pid, data))
                },
            )?);
            next.op_pipeline_mut().operators.push(map);

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
