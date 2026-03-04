use std::collections::{HashMap, hash_map};

use mlua::{LuaSerdeExt, UserData};

use crate::runtime::{
    api::{
        on::{EventScope, OnPluginLazy},
        plugins::entity::{
            components::{
                Blueprint, ComponentData, Control, CustomDataComponent, OwnedBy, Position,
                Rotation, Sprite2D, SpriteChar,
            },
            event_descriptors::ENTITY_EVENT_DESCRIPTORS,
            handle::EntityHandle,
            macros::{add_blueprint_methods, for_each_blueprint},
        },
    },
    app_data,
    utils::{get_app_data, get_app_data_mut},
};

pub type BlueprintId = u64;

#[derive(Clone)]
pub(crate) struct EntityBlueprint {
    id: BlueprintId,
    pub name: Option<String>,
    pub components: Vec<ComponentData>,
    pub customs: HashMap<String, serde_json::Value>,
}
impl EntityBlueprint {
    pub fn new(id: BlueprintId) -> Self {
        Self {
            id,
            name: None,
            components: Vec::new(),
            customs: HashMap::new(),
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

        methods.add_method("from", |lua, this, name: String| {
            if !this.components.is_empty() || !this.customs.is_empty() {
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

        methods.add_method("name", |_, this, name: String| {
            let mut next = this.clone();
            next.name = Some(name);
            Ok(next)
        });

        methods.add_method("custom", |lua, this, table: mlua::Table| {
            let mut next = this.clone();

            for pair in table.pairs::<String, mlua::Value>() {
                let (key, value) = pair?;
                next.customs.insert(key, lua.from_value(value)?);
            }

            Ok(next)
        });

        methods.add_method_mut("spawn", |lua, this, _: ()| {
            let runtime_phase = get_app_data::<app_data::RuntimePhase>(lua)?;
            if *runtime_phase == app_data::RuntimePhase::Blueprints {
                return Err(mlua::Error::runtime(
                    "Access denied: cannot spawn during blueprint loading phase",
                ));
            }

            let mut builder = hecs::EntityBuilder::new();
            // TODO: repeated code
            for component in &this.components {
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
                        builder.add(c.clone());
                    }
                    ComponentData::Name(c) => {
                        builder.add(c.clone());
                    }
                    // Blueprint component cannot be attached manually
                    ComponentData::Blueprint(_) => {}
                };
            }
            builder.add(Blueprint(this.id));

            if this.customs.len() != 0 {
                let customs = lua.to_value(&this.customs)?;
                builder.add(CustomDataComponent(lua.create_registry_value(customs)?));
            }

            let entity = get_app_data_mut::<app_data::World>(lua)?.spawn(builder.build());

            // Layer garbage collection
            let layers = get_app_data::<app_data::ActiveLayers>(lua)?;
            if let Some(layer_id) = layers.last() {
                let cleaner = lua.create_function(move |lua, _: ()| {
                    let _ = get_app_data_mut::<app_data::World>(lua)?.despawn(entity.clone());
                    Ok(())
                })?;

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
    }
}
