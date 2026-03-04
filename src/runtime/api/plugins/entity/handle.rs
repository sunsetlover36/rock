use mlua::{LuaSerdeExt, UserData};
use smallvec::smallvec;

use crate::runtime::{
    api::{
        on::{
            EventScope, OnPluginLazy,
            protocol::{EntityEventData, GameModeEvent, GameModeEventData},
        },
        plugins::entity::{
            components::{
                ComponentData, Control, CustomDataComponent, Name, OwnedBy, Position, Rotation,
                Sprite2D, SpriteChar,
            },
            event_descriptors::ENTITY_EVENT_DESCRIPTORS,
            macros::{add_handle_methods, for_each_handle},
        },
    },
    app_data,
    utils::{get_app_data, get_app_data_mut},
};

#[derive(Clone)]
pub(super) struct EntityHandle {
    pub entity: hecs::Entity,
    pub blueprint_id: u64,
}
impl EntityHandle {
    fn get_custom(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let world = get_app_data::<app_data::World>(lua)?;

        if let Ok(comp) = world.get::<&CustomDataComponent>(self.entity) {
            return Ok(lua.registry_value(&comp.0)?);
        } else {
            return Ok(mlua::Value::Nil);
        }
    }
    fn set_custom(&self, lua: &mlua::Lua, table: mlua::Table) -> mlua::Result<()> {
        let event_bus = get_app_data::<app_data::EventBus>(lua)?.clone();
        let mut world = get_app_data_mut::<app_data::World>(lua)?;

        let rk = lua.create_registry_value(&table)?;
        if let Ok(mut comp) = world.get::<&mut CustomDataComponent>(self.entity) {
            comp.0 = rk;
        } else {
            world
                .insert_one(self.entity, CustomDataComponent(rk))
                .map_err(|e| {
                    mlua::Error::runtime(format!(
                        "Failed to change custom data for the entity: {}",
                        e
                    ))
                })?;
        }

        event_bus.schedule_event(GameModeEvent {
            scopes: smallvec![
                EventScope::Entity(self.entity.id().into()),
                EventScope::Blueprint(self.blueprint_id),
            ],
            data: GameModeEventData::Entity(EntityEventData::CustomDataUpdate(table)),
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
    }
}
