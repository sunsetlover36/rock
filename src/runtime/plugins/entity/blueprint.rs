use std::collections::{HashMap, hash_map};

use mlua::{LuaSerdeExt, UserData};

use super::{
    components::{
        Blueprint, ComponentData, Control, OwnedBy, Position, Room, Rotation, Sprite2D, SpriteChar,
    },
    event_descriptors::ENTITY_EVENT_DESCRIPTORS,
    handle::EntityHandle,
    macros::{add_blueprint_methods, for_each_blueprint},
    rx::SyncRx,
};
use crate::runtime::{
    app_data, despawn_entity,
    network_replicator::{FieldRegistry, protocol::ReplicationTarget},
    plugins::{OnPluginLazy, entity::components::ComponentKey, on::protocol::EventScope},
    room_str_to_id, spawn_entity,
    utils::{get_app_data, get_app_data_mut},
};

pub type BlueprintId = u64;

#[derive(Clone)]
pub(crate) struct EntityBlueprint {
    id: BlueprintId,
    pub name: Option<String>,
    pub components: HashMap<ComponentKey, ComponentData>,
    pub customs: Option<serde_json::Map<String, serde_json::Value>>,
}
impl EntityBlueprint {
    pub fn new(id: BlueprintId) -> Self {
        Self {
            id,
            name: None,
            components: HashMap::new(),
            customs: None,
        }
    }

    pub fn id(&self) -> BlueprintId {
        self.id
    }
}
impl UserData for EntityBlueprint {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("on", |_, this| {
            Ok(OnPluginLazy {
                scope: EventScope::Blueprint(this.id),
                descriptors: ENTITY_EVENT_DESCRIPTORS,
            })
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        for_each_blueprint!(methods, add_blueprint_methods);

        methods.add_method("name", |_, this, name: String| {
            let mut next = this.clone();
            next.name = Some(name);
            Ok(next)
        });

        methods.add_method("room", |lua, this, name: String| {
            let mut next = this.clone();
            next.components.insert(
                ComponentKey::Room,
                ComponentData::Room(Room(room_str_to_id(lua, &name)?)),
            );
            Ok(next)
        });

        methods.add_method("custom", |lua, this, table: mlua::Table| {
            let mut next = this.clone();

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

                let json = lua.from_value::<serde_json::Map<String, serde_json::Value>>(
                    mlua::Value::Table(table),
                )?;
                next.customs = Some(json);
            } else {
                next.customs = None;
            }

            Ok(next)
        });

        methods.add_method("from", |lua, this, name: String| {
            let has_customs = this.customs.as_ref().map_or(false, |v| v.is_empty());
            if !this.components.is_empty() || has_customs {
                return Err(mlua::Error::runtime(format!("Cannot call `from(\"{}\")`: blueprint already contains components or custom data", name)));
            }

            let registry = get_app_data::<app_data::BlueprintRegistry>(lua)?;
            let blueprints = &registry.blueprints;
            if let Some(blueprint) = blueprints.get(&name) {
                let blueprint = blueprint.clone();
                let mut this = this.clone();
                this.components = blueprint.components;
                this.customs = blueprint.customs;

                return Ok(this);
            } else {
                return Err(mlua::Error::runtime(format!("Cannot call `from(\"{}\")`: blueprint not found", name)));
            }
        });
        methods.add_method(
            "register",
            |lua, this, name: String| match get_app_data_mut::<app_data::BlueprintRegistry>(lua)?
                .blueprints
                .entry(name.clone())
            {
                hash_map::Entry::Occupied(_) => Err(mlua::Error::runtime(format!(
                    "Cannot call `register(\"{}\")`: blueprint with this name already exists",
                    name
                ))),
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(this.clone());
                    Ok(())
                }
            },
        );

        methods.add_method_mut("spawn", |lua, this, _: ()| {
            let runtime_phase = get_app_data::<app_data::RuntimePhase>(lua)?;
            if *runtime_phase == app_data::RuntimePhase::Blueprints {
                return Err(mlua::Error::runtime(
                    "Access denied: cannot spawn during blueprint loading phase",
                ));
            }

            let mut builder = hecs::EntityBuilder::new();
            // TODO: repeated code
            for component in this.components.values() {
                match component {
                    ComponentData::Position(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::Rotation(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::Control(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::Sprite2D(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::SpriteChar(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::OwnedBy(c) => {
                        builder.add(*c);
                    }
                    ComponentData::Name(c) => {
                        builder.add(c.clone());
                    }
                    ComponentData::Room(c) => {
                        builder.add(*c);
                    }
                    // Blueprint component cannot be attached manually
                    ComponentData::Blueprint(_) => {}
                };
            }
            builder.add(Blueprint(this.id));

            let entity = spawn_entity(lua, builder.build())?;
            if let Some(customs) = &this.customs {
                let value = lua.to_value(customs)?;
                let table = value.as_table().ok_or_else(|| mlua::Error::runtime(format!("Failed to spawn an entity: custom component data is not a table, blueprint ID '{}'", this.id)))?;
                get_app_data_mut::<app_data::EntityCustoms>(lua)?.insert(entity, table.clone());
            }

            // Layer garbage collection
            let layers = get_app_data::<app_data::ActiveLayers>(lua)?;
            if let Some(layer_id) = layers.last() {
                let cleaner = lua.create_function(move |lua, _: ()| despawn_entity(lua, entity))?;

                get_app_data_mut::<app_data::LayerRegistry>(lua)?
                    .layers
                    .entry(layer_id.to_owned())
                    .and_modify(|layer| layer.cleaners.push(cleaner));
            }

            Ok(EntityHandle {
                entity,
                blueprint_id: this.id,
            })
        });

        methods.add_method("sync", |_, this, _: ()| {
            Ok(SyncRx::new(ReplicationTarget::Blueprint(this.id)))
        });
    }
}
