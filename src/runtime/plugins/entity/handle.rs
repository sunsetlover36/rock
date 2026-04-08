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
    app_data, despawn_entity,
    network_replicator::{
        FieldRegistry,
        protocol::{
            EntityDirtyComponent, EntityReplicationAction, ReplicationMark, ReplicationTarget,
        },
    },
    plugins::{
        OnPluginLazy,
        on::protocol::{EntityEventData, EventScope, GameModeEvent, GameModeEventData},
    },
    room_str_to_id,
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
            let mut field_registry = get_app_data_mut::<FieldRegistry>(lua)?;
            for pair in table.pairs::<String, mlua::Value>() {
                let (key, _) = pair?;
                if field_registry.is_reserved_field(&key) {
                    return Err(mlua::Error::runtime(format!(
                        "Cannot use reserved core component name '{}' in custom fields",
                        key
                    )));
                }

                field_registry.add_bit_for(&key).map_err(|e| {
                    mlua::Error::runtime(format!(
                        "Failed to add a new bit index for key '{}': {}",
                        key, e
                    ))
                })?;
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
            entity: self.entity,
            action: EntityReplicationAction::Update(EntityDirtyComponent::Custom),
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
                    let mut prev_room_id = None;
                    let room_comp = Room(room_str_to_id(lua, &name)?);
                    if let Ok(mut field) = world.get::<&mut Room>(this.entity) {
                        prev_room_id = Some(field.0);
                        *field = room_comp;
                    } else {
                        world.insert_one(this.entity, room_comp).map_err(|e| {
                            mlua::Error::runtime(format!(
                                "Failed to add a room component to the entity in method `:room`: {}",
                                e
                            ))
                        })?;
                    }

                    let event_bus = get_app_data::<app_data::EventBus>(lua)?;
                    event_bus.schedule_event(GameModeEvent {
                        scopes: smallvec![
                            EventScope::Entity(this.entity.id().into()),
                            EventScope::Blueprint(this.blueprint_id),
                        ],
                        data: GameModeEventData::Entity(EntityEventData::ComponentUpdate(
                            ComponentData::Room(room_comp),
                        )),
                    });

                    let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)?;
                    let _ = replicator_tx.0.send(ReplicationMark::Entity {
                        entity: this.entity,
                        action: EntityReplicationAction::Warp {
                            from: prev_room_id,
                            to: Some(room_comp.0)
                        },
                    });

                    Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?))
                }
                None => {
                    if let Ok(room_comp) = world.get::<&Room>(this.entity) {
                        Ok(mlua::Value::Integer(room_comp.0 as i64))
                    } else {
                        Ok(mlua::Value::Nil)
                    }
                }
            }
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
            despawn_entity(lua, this.entity)
        });

        methods.add_method("exists", |lua, this, _: ()| {
            Ok(get_app_data::<app_data::World>(lua)?.contains(this.entity))
        });

        methods.add_method("sync", |_, this, _: ()| {
            Ok(SyncRx::new(ReplicationTarget::Entity(this.entity)))
        });
    }
}
