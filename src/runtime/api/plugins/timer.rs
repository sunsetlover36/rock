use color_eyre::eyre;

use crate::runtime::{
    api::{
        plugins::timer::rx::TimerRx,
        protocol::{AsyncTask, GameModePlugin, PluginName},
    },
    app_data,
    utils::get_app_data,
};

mod handle;
mod rx;

pub(crate) struct TimerPlugin {}
impl GameModePlugin for TimerPlugin {
    fn name(&self) -> PluginName {
        PluginName::Timer
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let create_fn = lua.create_function(|_, _: ()| Ok(TimerRx::default()))?;
        table.set("create", create_fn)?;

        let cancel_fn = lua.create_function(|lua, id: String| {
            get_app_data::<app_data::TimerManager>(lua)?.cancel_timer(id);
            Ok(())
        })?;
        table.set("cancel", cancel_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
