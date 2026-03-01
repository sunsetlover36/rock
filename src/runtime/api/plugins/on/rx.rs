use mlua::UserData;

use crate::{
    runtime::{
        api::on::{
            EventScope, GameModeEventKey, GameModeListener, GameModeListenerParams, PlayerEventKey,
            handle::ListenerHandle,
        },
        app_data::{self, ExecutionContext},
    },
    rx::{HasRxPipeline, RxOperator, RxPipeline, add_rx_methods},
};

struct NewListenerParams {
    handle: mlua::Function,
    seq: u64,
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
            pipeline: self.pipeline.clone(),
        })
    }

    fn add_event_listener(&self, lua: &mlua::Lua, handle: mlua::Function) -> mlua::Result<u64> {
        let event_key = self.event_key;

        let current_seq = lua
            .app_data_mut::<app_data::EventBus>()
            .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
            .increment_sequence();
        let context = *lua
            .app_data_ref::<app_data::ExecutionContext>()
            .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
        {
            let mut listeners = lua
                .app_data_mut::<app_data::EventListeners>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialiezd"))?;
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
        let layers = lua
            .app_data_ref::<app_data::ActiveLayers>()
            .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
        if let Some(layer) = layers.last() {
            let cleaner = lua.create_function(move |lua, _: ()| {
                lua.app_data_mut::<app_data::EventListeners>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                    .entry(event_key)
                    .and_modify(|listeners| listeners.retain(|l| l.get_seq() != current_seq));

                Ok(())
            })?;
            lua.app_data_mut::<app_data::LayerRegistry>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                .layers
                .entry(layer.to_owned())
                .and_modify(|l| l.cleaners.push(cleaner));
        }

        Ok(current_seq)
    }
}
impl HasRxPipeline for OnRx {
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.pipeline
    }
}
impl UserData for OnRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_rx_methods(methods);

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
            let predicate =
                RxOperator::Filter(lua.create_function(move |_, args: (u64, mlua::Table)| {
                    let action_table: mlua::Table = args.1;
                    let action_name: String = action_table.get("name")?;
                    Ok(action_name == event_name)
                })?);
            next.pipeline.operators.push(predicate);

            let map =
                RxOperator::Map(lua.create_function(move |_, args: (u64, mlua::Table)| {
                    let pid: u64 = args.0;
                    let action_table: mlua::Table = args.1;
                    let data: mlua::Value = action_table.get("data")?;
                    Ok((pid, data))
                })?);
            next.pipeline.operators.push(map);

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
