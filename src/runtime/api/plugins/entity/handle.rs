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
                ComponentData, Control, CustomDataComponent, Sprite2D, SpriteChar, Transform2D,
                Vector2D,
            },
            event_descriptors::ENTITY_EVENT_DESCRIPTORS,
            macros::{add_handle_methods, for_each_component},
        },
    },
    app_data,
};

#[derive(Clone)]
pub(super) struct EntityHandle {
    pub entity: hecs::Entity,
    pub blueprint_id: u64,
}
impl UserData for EntityHandle {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("on", |_, this| {
            Ok(OnPluginLazy {
                descriptors: ENTITY_EVENT_DESCRIPTORS,
                scope: EventScope::Entity(this.entity.id().into()),
            })
        });
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        for_each_component!(methods, add_handle_methods);

        methods.add_method("custom", |lua, this, table: Option<mlua::Table>| {
            let event_bus = lua
                .app_data_ref::<app_data::EventBus>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                .clone();
            let mut world = lua
                .app_data_mut::<app_data::World>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;

            if let Some(table) = table {
                let rk = lua.create_registry_value(&table)?;
                if let Ok(mut comp) = world.get::<&mut CustomDataComponent>(this.entity) {
                    comp.0 = rk;
                } else {
                    world
                        .insert_one(this.entity, CustomDataComponent(rk))
                        .map_err(|e| {
                            mlua::Error::runtime(format!(
                                "Failed to change custom data for the entity: {}",
                                e
                            ))
                        })?;
                }

                event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![
                        EventScope::Entity(this.entity.id().into()),
                        EventScope::Blueprint(this.blueprint_id),
                    ],
                    data: GameModeEventData::Entity(EntityEventData::CustomDataUpdate(table)),
                });
                return Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?));
            } else {
                if let Ok(comp) = world.get::<&CustomDataComponent>(this.entity) {
                    return Ok(lua.registry_value(&comp.0)?);
                } else {
                    return Ok(mlua::Value::Nil);
                }
            }
        });
    }
}
