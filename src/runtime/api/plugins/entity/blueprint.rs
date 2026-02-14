use std::collections::HashMap;

use mlua::{LuaSerdeExt, UserData};

use crate::runtime::{
    api::plugins::entity::{
        components::{
            ComponentVariant, Control, CustomDataComponent, Sprite2D, SpriteChar, Transform2D,
            Vector2D,
        },
        handle::EntityHandle,
        macros::{add_blueprint_methods, for_each_component},
    },
    app_data::GameModeAppData,
};

#[derive(Clone)]
pub(super) struct EntityBlueprint {
    pub name: Option<String>,
    pub components: Vec<ComponentVariant>,
    pub customs: HashMap<String, serde_json::Value>,
}
impl EntityBlueprint {
    pub fn new() -> Self {
        Self {
            name: None,
            components: Vec::new(),
            customs: HashMap::new(),
        }
    }
}
impl UserData for EntityBlueprint {
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
            let mut builder = hecs::EntityBuilder::new();
            for component in &this.components {
                match component {
                    ComponentVariant::Vector2D(c) => builder.add(c.clone()),
                    ComponentVariant::Transform2D(c) => builder.add(c.clone()),
                    ComponentVariant::Control(c) => builder.add(c.clone()),
                    ComponentVariant::Sprite2D(c) => builder.add(c.clone()),
                    ComponentVariant::SpriteChar(c) => builder.add(c.clone()),
                };
            }

            if this.customs.len() != 0 {
                let customs = lua.to_value(&this.customs)?;
                builder.add(CustomDataComponent(lua.create_registry_value(customs)?));
            }

            let mut app_data = lua
                .app_data_mut::<GameModeAppData>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            let entity = app_data.world.spawn(builder.build());
            Ok(EntityHandle { entity })
        });
    }
}
