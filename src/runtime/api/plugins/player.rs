use color_eyre::eyre;
use mlua::IntoLua;
use shared::{PlayerId, PlayerKey};

use crate::runtime::{
    GameModeClientCommand,
    api::protocol::{AsyncTask, GameModePlugin, PluginName},
    app_data,
    utils::get_app_data,
};

mod handle;
pub(crate) use handle::PlayerHandle;

pub(crate) struct PlayerPlugin {}
impl GameModePlugin for PlayerPlugin {
    fn name(&self) -> PluginName {
        PluginName::Player
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let get_fn = lua.create_function(|lua, pid: PlayerId| {
            let pk = PlayerKey::unpack(pid);
            if get_app_data::<app_data::ClientApi>(lua)?.has(pk) {
                Ok(PlayerHandle::new(pk).into_lua(lua)?)
            } else {
                Ok(mlua::Value::Nil)
            }
        })?;
        table.set("get", get_fn)?;

        let broadcast_fn = lua.create_function(|lua, text: String| {
            get_app_data::<app_data::ClientApi>(lua)?
                .send(GameModeClientCommand::Broadcast { text });

            Ok(())
        })?;
        table.set("broadcast", broadcast_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
