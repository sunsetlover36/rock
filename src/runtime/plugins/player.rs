use color_eyre::eyre;
use mlua::IntoLua;
use shared::{PlayerId, PlayerKey};

use super::protocol::{AsyncTask, GameModePlugin, PluginName};
use crate::runtime::{app_data, utils::get_app_data};

mod handle;
pub(crate) use handle::PlayerHandle;

mod broadcast_rx;
use broadcast_rx::BroadcastRx;

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

        let broadcast_fn = lua.create_function(|_, _: ()| Ok(BroadcastRx {}))?;
        table.set("broadcast", broadcast_fn)?;

        let list_fn = lua.create_function(|lua, _: ()| {
            let players: Vec<PlayerHandle> = get_app_data::<app_data::ClientApi>(lua)?
                .list()
                .iter()
                .map(|pk| PlayerHandle::new(*pk))
                .collect();

            Ok(players)
        })?;
        table.set("list", list_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
