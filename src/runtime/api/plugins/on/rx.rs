use mlua::UserData;

use crate::{
    runtime::{
        api::on::{
            EventScope, GameModeEventKey, GameModeListener, GameModeListenerParams, PlayerEventKey,
        },
        app_data,
    },
    rx::{HasRxPipeline, RxOperator, RxPipeline, add_rx_methods},
};

#[derive(Clone)]
pub(super) struct OnRx {
    event: GameModeEventKey,
    scope: EventScope,
    name: Option<String>,
    pipeline: RxPipeline,
}
impl OnRx {
    pub fn new(event: GameModeEventKey, scope: EventScope) -> Self {
        Self {
            event,
            name: None,
            scope,
            pipeline: RxPipeline::default(),
        }
    }

    fn construct_listener(&self, handle: mlua::Function, seq: u64) -> GameModeListener {
        let builder = self.clone();
        GameModeListener::new(GameModeListenerParams {
            name: builder.name,
            created_at_seq: seq,
            scope: self.scope,
            handle,
            pipeline: self.pipeline.clone(),
        })
    }

    fn add_event_listener(&self, lua: &mlua::Lua, handle: mlua::Function) -> mlua::Result<()> {
        let current_seq = lua
            .app_data_mut::<app_data::EventBus>()
            .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
            .increment_sequence();
        lua.app_data_mut::<app_data::EventListeners>()
            .ok_or_else(|| mlua::Error::runtime("App data is not initialiezd"))?
            .entry(self.event)
            .or_default()
            .push(self.construct_listener(handle, current_seq));

        Ok(())
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

        // DSL sugar -> where + select for input events
        methods.add_method("bind_action", |lua, this, event_name: String| {
            if this.event != GameModeEventKey::Player(PlayerEventKey::Input) {
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
            this.add_event_listener(lua, handle)?;
            Ok(())
        });
    }
}
