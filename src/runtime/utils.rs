use std::hash::{Hash, Hasher};

use ahash::AHasher;
use color_eyre::eyre;

use crate::runtime::{
    app_data,
    network_replicator::protocol::{EntityReplicationAction, ReplicationMark},
    plugins::entity::components::Room,
};

pub trait LuaResultExt {
    type Ok;
    fn wrap_err(self, msg: &str) -> eyre::Result<Self::Ok>;
}
impl<T> LuaResultExt for Result<T, mlua::Error> {
    type Ok = T;
    fn wrap_err(self, msg: &str) -> eyre::Result<T> {
        self.map_err(|e| eyre::eyre!("{}: {}", msg, e))
    }
}

pub trait EyreResultExt {
    type Ok;
    fn wrap_eyre_err(self) -> mlua::Result<Self::Ok>;
}
impl<T> EyreResultExt for eyre::Result<T, eyre::Report> {
    type Ok = T;
    fn wrap_eyre_err(self) -> mlua::Result<T> {
        self.map_err(|report| mlua::Error::runtime(report.to_string()))
    }
}

pub fn get_app_data<'lua, T>(lua: &'lua mlua::Lua) -> mlua::Result<mlua::AppDataRef<'lua, T>>
where
    T: 'static,
{
    lua.app_data_ref::<T>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))
}
pub fn get_app_data_mut<'lua, T>(lua: &'lua mlua::Lua) -> mlua::Result<mlua::AppDataRefMut<'lua, T>>
where
    T: 'static,
{
    lua.app_data_mut::<T>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))
}

pub fn get_str_hash(s: &str) -> u64 {
    let mut hasher = AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

pub fn despawn_entity(lua: &mlua::Lua, entity: hecs::Entity) -> mlua::Result<bool> {
    let mut world = get_app_data_mut::<app_data::World>(lua)?;
    let room_id = world.get::<&Room>(entity).ok().map(|c| c.0);

    match world.despawn(entity) {
        Ok(()) => {
            if let Some(room_id) = room_id {
                let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)?;
                let _ = replicator_tx.0.send(ReplicationMark::Entity {
                    entity,
                    action: EntityReplicationAction::Despawn(room_id),
                });
            }

            Ok(true)
        }
        Err(hecs::NoSuchEntity) => Ok(false),
    }
}
