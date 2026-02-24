use mlua::UserData;

use crate::runtime::{
    api::on::{EventScope, GameModeEventKey, GameModeListener, PlayerEventKey},
    app_data,
};

#[derive(Clone)]
pub(crate) enum RxOperator {
    Filter(mlua::Function),
    Map(mlua::Function),
}

#[derive(Clone)]
pub(super) struct RxBuilder {
    event: GameModeEventKey,
    scope: EventScope,
    name: Option<String>,
    limit: Option<u32>,
    operators: Vec<RxOperator>,
    consumed: bool,
}
impl RxBuilder {
    pub fn new(event: GameModeEventKey, scope: EventScope) -> Self {
        Self {
            event,
            name: None,
            limit: None,
            scope,
            operators: Vec::new(),
            consumed: false,
        }
    }

    fn construct_listener(&self, handle: mlua::Function, seq: u64) -> GameModeListener {
        let builder = self.clone();
        GameModeListener {
            name: builder.name,
            created_at_seq: seq,
            handle,
            call_count: 0,
            limit: self.limit,
            scope: self.scope,
            operators: builder.operators,
        }
    }
    fn ensure_not_consumed(&self) -> mlua::Result<()> {
        if self.consumed {
            Err(mlua::Error::runtime(
                "This event chain was already consumed. Create a new event chain.",
            ))
        } else {
            Ok(())
        }
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
impl UserData for RxBuilder {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("take", |_, this, n: u32| {
            let mut next = this.clone();
            next.limit = Some(n);
            Ok(next)
        });

        methods.add_method("where", |_, this, predicate: mlua::Function| {
            let mut next = this.clone();
            next.operators.push(RxOperator::Filter(predicate));
            Ok(next)
        });
        methods.add_method("select", |_, this, selector: mlua::Function| {
            let mut next = this.clone();
            next.operators.push(RxOperator::Map(selector));
            Ok(next)
        });

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
            next.operators.push(predicate);

            let map =
                RxOperator::Map(lua.create_function(move |_, args: (u64, mlua::Table)| {
                    let pid: u64 = args.0;
                    let action_table: mlua::Table = args.1;
                    let data: mlua::Value = action_table.get("data")?;
                    Ok((pid, data))
                })?);
            next.operators.push(map);

            Ok(next)
        });

        methods.add_method_mut("once", |lua, this, handle| {
            this.ensure_not_consumed()?;
            this.consumed = true;

            this.limit = Some(1);

            this.add_event_listener(lua, handle)?;
            Ok(())
        });
        methods.add_method_mut("each", |lua, this, handle| {
            this.ensure_not_consumed()?;
            this.consumed = true;

            this.add_event_listener(lua, handle)?;
            Ok(())
        });
    }
}
