use std::collections::HashMap;

use mlua::{LuaSerdeExt, UserData};

use crate::runtime::{
    api::{
        on::{EventScope, OnPluginLazy},
        plugins::entity::{
            components::{
                ComponentData, Control, CustomDataComponent, Sprite2D, SpriteChar, Transform2D,
                Vector2D,
            },
            event_descriptors::ENTITY_EVENT_DESCRIPTORS,
            handle::EntityHandle,
            macros::{add_blueprint_methods, for_each_component},
        },
    },
    app_data,
};

#[derive(Clone)]
pub(super) struct EntityBlueprint {
    id: u64,
    pub name: Option<String>,
    pub components: Vec<ComponentData>,
    pub customs: HashMap<String, serde_json::Value>,
}
impl EntityBlueprint {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            name: None,
            components: Vec::new(),
            customs: HashMap::new(),
        }
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
        for_each_component!(methods, add_blueprint_methods);

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
            let runtime_phase = lua
                .app_data_ref::<app_data::RuntimePhase>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            if *runtime_phase == app_data::RuntimePhase::Blueprints {
                return Err(mlua::Error::runtime(
                    "Access denied: cannot spawn during blueprint loading phase",
                ));
            }

            let mut builder = hecs::EntityBuilder::new();
            for component in &this.components {
                match component {
                    ComponentData::Vector2D(c) => builder.add(c.clone()),
                    ComponentData::Transform2D(c) => builder.add(c.clone()),
                    ComponentData::Control(c) => builder.add(c.clone()),
                    ComponentData::Sprite2D(c) => builder.add(c.clone()),
                    ComponentData::SpriteChar(c) => builder.add(c.clone()),
                };
            }

            if this.customs.len() != 0 {
                let customs = lua.to_value(&this.customs)?;
                builder.add(CustomDataComponent(lua.create_registry_value(customs)?));
            }

            let mut world = lua
                .app_data_mut::<app_data::World>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            let entity = world.spawn(builder.build());
            Ok(EntityHandle { entity })
        });
    }
}
