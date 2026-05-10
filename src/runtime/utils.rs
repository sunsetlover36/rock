use std::hash::{Hash, Hasher};

use ahash::AHasher;
use color_eyre::eyre;

use crate::runtime::{
    app_data,
    network_replicator::protocol::{EntityReplicationAction, ReplicationMark, RoomId},
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
pub fn room_str_to_id(lua: &mlua::Lua, s: &str) -> mlua::Result<RoomId> {
    let id = get_str_hash(s);
    get_app_data_mut::<app_data::RoomIdToName>(lua)?
        .0
        .insert(id, s.to_owned());
    Ok(id)
}
pub fn room_id_to_name(lua: &mlua::Lua, id: u64) -> mlua::Result<String> {
    let room_id_to_name = get_app_data::<app_data::RoomIdToName>(lua)?;
    room_id_to_name.0.get(&id).cloned().ok_or_else(|| mlua::Error::runtime(format!("Failed to convert an argument for PlayerEventData::Warp: room name not found for room ID {}", id)))
}

pub fn spawn_entity(lua: &mlua::Lua, entity: hecs::BuiltEntity) -> mlua::Result<hecs::Entity> {
    let mut world_data = get_app_data_mut::<app_data::World>(lua)?;
    let world = &mut world_data.0;

    let entity = world.spawn(entity);
    if let Ok(room_comp) = world.get::<&Room>(entity) {
        let room_id = room_comp.0;
        let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)?;
        let _ = replicator_tx.0.send(ReplicationMark::Entity {
            entity,
            action: EntityReplicationAction::Spawn(room_id),
        });
    }

    Ok(entity)
}
pub fn despawn_entity(lua: &mlua::Lua, entity: hecs::Entity) -> mlua::Result<bool> {
    let mut world_data = get_app_data_mut::<app_data::World>(lua)?;
    let world = &mut world_data.0;

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
