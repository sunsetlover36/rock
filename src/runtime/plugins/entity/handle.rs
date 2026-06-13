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
    room_id_to_name, room_str_to_id,
    utils::{get_app_data, get_app_data_mut},
};
use crate::utils::{custom_table_to_json, json_to_lua};

#[derive(Clone)]
pub(crate) struct EntityHandle {
    pub entity: hecs::Entity,
    pub blueprint_id: u64,
}
impl EntityHandle {
    fn get_custom(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let customs = get_app_data::<app_data::EntityCustoms>(lua)?;
        if let Some(custom) = customs.0.get(&self.entity) {
            let custom = custom_table_to_json(lua, Some(custom))?;
            json_to_lua(lua, serde_json::Value::Object(custom))
        } else {
            Ok(mlua::Value::Nil)
        }
    }
    fn set_custom(&self, lua: &mlua::Lua, table: mlua::Table) -> mlua::Result<()> {
        let previous_custom = {
            let customs = get_app_data::<app_data::EntityCustoms>(lua)?;
            custom_table_to_json(lua, customs.0.get(&self.entity))?
        };
        let next_custom = custom_table_to_json(lua, Some(&table))?;

        let mut changed_keys = Vec::new();
        for (key, value) in &next_custom {
            if previous_custom.get(key) != Some(value) {
                changed_keys.push(key.clone());
            }
        }
        for key in previous_custom.keys() {
            if !next_custom.contains_key(key) {
                changed_keys.push(key.clone());
            }
        }

        if !next_custom.is_empty() {
            let mut field_registry = get_app_data_mut::<FieldRegistry>(lua)?;
            for key in next_custom.keys() {
                if field_registry.is_reserved_field(&key) {
                    return Err(mlua::Error::runtime(format!(
                        "Cannot use reserved core component name '{}' in custom fields",
                        key
                    )));
                }

                field_registry.get_or_add_bit_for(&key).map_err(|e| {
                    mlua::Error::runtime(format!(
                        "Failed to add a new bit index for key '{}': {}",
                        key, e
                    ))
                })?;
            }
        }

        get_app_data_mut::<app_data::EntityCustoms>(lua)?
            .0
            .insert(self.entity, table.clone());

        let event_bus = get_app_data::<app_data::EventBus>(lua)?;
        event_bus.0.schedule_event(GameModeEvent {
            scopes: smallvec![
                EventScope::Entity(self.entity.id().into()),
                EventScope::Blueprint(self.blueprint_id),
            ],
            data: GameModeEventData::Entity(EntityEventData::CustomDataUpdate(table.clone())),
        });

        let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)?;
        for key in changed_keys {
            let _ = replicator_tx.0.send(ReplicationMark::Entity {
                entity: self.entity,
                action: EntityReplicationAction::Update(EntityDirtyComponent::CustomField(key)),
            });
        }

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
            let mut world_data = get_app_data_mut::<app_data::World>(lua)?;
            let world = &mut world_data.0;
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
                    event_bus.0.schedule_event(GameModeEvent {
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
                        room_id_to_name(lua, room_comp.0)
                            .and_then(|name| lua.create_string(&name))
                            .map(mlua::Value::String)
                    } else {
                        Ok(mlua::Value::Nil)
                    }
                }
            }
        });

        methods.add_method("custom", |lua, this, value: mlua::Value| match value {
            mlua::Value::Table(table) => {
                this.set_custom(lua, table)?;
                Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?))
            }
            mlua::Value::Function(f) => {
                let custom = this.get_custom(lua)?;
                this.set_custom(lua, f.call::<mlua::Table>(custom)?)?;
                Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?))
            }
            mlua::Value::Nil => this.get_custom(lua),
            _ => Err(mlua::Error::runtime(
                "entity.custom: got an unknown value type",
            )),
        });

        methods.add_method("despawn", |lua, this, _: ()| {
            despawn_entity(lua, this.entity)
        });

        methods.add_method("exists", |lua, this, _: ()| {
            Ok(get_app_data::<app_data::World>(lua)?
                .0
                .contains(this.entity))
        });

        methods.add_method("sync", |_, this, _: ()| {
            Ok(SyncRx::new(ReplicationTarget::Entity(this.entity)))
        });
    }
}
