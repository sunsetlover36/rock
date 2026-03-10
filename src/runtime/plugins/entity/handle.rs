use mlua::{LuaSerdeExt, UserData};
use smallvec::smallvec;

use super::{
    components::{
        ComponentData, Control, Name, OwnedBy, Position, Room, Rotation, Sprite2D, SpriteChar,
    },
    event_descriptors::ENTITY_EVENT_DESCRIPTORS,
    macros::{add_handle_methods, for_each_handle},
    rx::SyncRx,
};
use crate::runtime::{
    app_data, get_str_hash,
    network_replicator::{
        FieldRegistry,
        protocol::{EntityDirtyComponent, ReplicationMark, ReplicationTarget},
    },
    plugins::{
        OnPluginLazy,
        on::protocol::{EntityEventData, EventScope, GameModeEvent, GameModeEventData},
    },
    utils::{get_app_data, get_app_data_mut},
};

#[derive(Clone)]
pub(crate) struct EntityHandle {
    pub entity: hecs::Entity,
    pub blueprint_id: u64,
}
impl EntityHandle {
    fn get_custom(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let customs = get_app_data::<app_data::EntityCustoms>(lua)?;
        if let Some(custom) = customs.get(&self.entity) {
            return Ok(mlua::Value::Table(custom.clone()));
        } else {
            return Ok(mlua::Value::Nil);
        }
    }
    fn set_custom(&self, lua: &mlua::Lua, table: mlua::Table) -> mlua::Result<()> {
        if !table.is_empty() {
            let field_registry = get_app_data::<FieldRegistry>(lua)?;
            for pair in table.pairs::<String, mlua::Value>() {
                let (key, _) = pair?;
                if field_registry.is_reserved_field(&key) {
                    return Err(mlua::Error::runtime(format!(
                        "Cannot use reserved core component name '{}' in custom fields",
                        key
                    )));
                }
            }
        }

        get_app_data_mut::<app_data::EntityCustoms>(lua)?.insert(self.entity, table.clone());

        let event_bus = get_app_data::<app_data::EventBus>(lua)?;
        event_bus.schedule_event(GameModeEvent {
            scopes: smallvec![
                EventScope::Entity(self.entity.id().into()),
                EventScope::Blueprint(self.blueprint_id),
            ],
            data: GameModeEventData::Entity(EntityEventData::CustomDataUpdate(table.clone())),
        });

        let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)?;
        let _ = replicator_tx.0.send(ReplicationMark::Entity {
            id: self.entity,
            component: EntityDirtyComponent::Custom,
        });

        Ok(())
    }
}
impl UserData for EntityHandle {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // TODO: every get call creates a new table
        fields.add_field_method_get("on", |_, this| {
            Ok(OnPluginLazy {
                descriptors: ENTITY_EVENT_DESCRIPTORS,
                scope: EventScope::Entity(this.entity.id().into()),
            })
        });
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        for_each_handle!(methods, add_handle_methods);

        methods.add_method("room", |lua, this, name: Option<String>| {
            let mut world = get_app_data_mut::<app_data::World>(lua)?;
            match name {
                Some(name) => {
                    if let Ok(mut field) = world.get::<&mut Room>(this.entity) {
                        *field = Room(get_str_hash(&name));
                    }
                }
                None => {
                    let _ = world.remove_one::<Room>(this.entity);
                }
            }

            Ok(())
        });

        methods.add_method("custom", |lua, this, value: mlua::Value| match value {
            mlua::Value::Table(table) => {
                this.set_custom(lua, table)?;
                return Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?));
            }
            mlua::Value::Function(f) => {
                let custom = this.get_custom(lua)?;
                this.set_custom(lua, f.call::<mlua::Table>(custom)?)?;
                return Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?));
            }
            mlua::Value::Nil => {
                return this.get_custom(lua);
            }
            _ => {
                return Err(mlua::Error::runtime(
                    "entity.custom: got an unknown value type",
                ));
            }
        });

        methods.add_method("despawn", |lua, this, _: ()| {
            match get_app_data_mut::<app_data::World>(lua)?.despawn(this.entity) {
                Ok(()) => Ok(true),
                Err(hecs::NoSuchEntity) => Ok(false),
            }
        });

        methods.add_method("exists", |lua, this, _: ()| {
            Ok(get_app_data::<app_data::World>(lua)?.contains(this.entity))
        });

        methods.add_method("sync", |_, this, _: ()| {
            Ok(SyncRx::new(ReplicationTarget::Entity(this.entity)))
        });
    }
}
