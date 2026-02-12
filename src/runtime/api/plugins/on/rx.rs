use mlua::UserData;

use crate::runtime::{GameModeEvent, GameModeListener, app_data::GameModeAppData};

#[derive(Clone)]
pub(super) struct RxBuilder {
    event: GameModeEvent,
    name: Option<String>,
    limit: Option<u32>,
    filters: Vec<mlua::Function>,
    consumed: bool,
}
impl RxBuilder {
    pub fn new(event: GameModeEvent) -> Self {
        Self {
            event,
            name: None,
            limit: None,
            filters: Vec::new(),
            consumed: false,
        }
    }

    fn construct_listener(&self, handle: mlua::Function) -> GameModeListener {
        let builder = self.clone();
        GameModeListener {
            name: builder.name,
            handle,
            call_count: 0,
            limit: self.limit,
            filters: builder.filters,
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
}
impl UserData for RxBuilder {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("take", |_, this, n: u32| {
            let mut next = this.clone();
            next.limit = Some(n);
            Ok(next)
        });

        methods.add_method("where", |_, this, filter| {
            let mut next = this.clone();
            next.filters.push(filter);
            Ok(next)
        });

        methods.add_method_mut("once", |lua, this, handle| {
            this.ensure_not_consumed()?;

            this.consumed = true;
            this.limit = Some(1);

            let mut app_data = lua
                .app_data_mut::<GameModeAppData>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            app_data
                .event_listeners
                .entry(this.event)
                .or_default()
                .push(this.construct_listener(handle));

            Ok(())
        });
        methods.add_method_mut("each", |lua, this, handle| {
            this.ensure_not_consumed()?;

            this.consumed = true;

            let mut app_data = lua
                .app_data_mut::<GameModeAppData>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            app_data
                .event_listeners
                .entry(this.event)
                .or_default()
                .push(this.construct_listener(handle));

            Ok(())
        });
    }
}
