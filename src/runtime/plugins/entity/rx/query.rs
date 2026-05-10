use mlua::{IntoLuaMulti, LuaSerdeExt, UserData};
use rock_wire::PlayerId;

use crate::{
    runtime::{
        app_data,
        network_replicator::protocol::{Area, RoomId},
        plugins::entity::{
            BlueprintId, EntityBlueprint,
            components::{Blueprint, Name, OwnedBy, Position, Room},
            handle::EntityHandle,
        },
        room_str_to_id,
        utils::get_app_data,
    },
    rx::{
        HasPipeline, RxPipeline, RxSentry, RxSentryError,
        core::{CoreSentryError, add_core_pipeline_methods},
        operator::add_op_pipeline_methods,
    },
};

#[derive(Clone, Default)]
pub(in crate::runtime::plugins::entity) struct QueryRx {
    owned_by: Option<PlayerId>,
    named: Option<String>,
    in_room: Option<RoomId>,
    area: Option<Area>,
    blueprint_id: Option<BlueprintId>,
    pipeline: RxPipeline,
}
impl QueryRx {
    fn matches(
        &self,
        blueprint: &Blueprint,
        name: Option<&Name>,
        owned_by: Option<&OwnedBy>,
        room: Option<&Room>,
        position: Option<&Position>,
    ) -> bool {
        let blueprint_check = self.blueprint_id.map_or(true, |b_id| b_id == blueprint.0);

        let ownership_check = match owned_by {
            Some(owned_by) => self.owned_by.map_or(true, |owner| owner == owned_by.0),
            None => self.owned_by.is_none(),
        };

        let name_check = match name {
            Some(name) => self
                .named
                .as_ref()
                .map_or(true, |filter_name| filter_name == &name.0),
            None => self.named.is_none(),
        };

        let room_check = match room {
            Some(room) => self.in_room.map_or(true, |room_id| room_id == room.0),
            None => self.in_room.is_none(),
        };

        let pos_check = match position {
            Some(position) => self.area.map_or(true, |area| area.contains(position.0)),
            None => self.area.is_none(),
        };

        blueprint_check && ownership_check && name_check && room_check && pos_check
    }

    fn get_matching_entities(&self, lua: &mlua::Lua) -> mlua::Result<Vec<(hecs::Entity, u64)>> {
        let entities = {
            let mut entities = Vec::new();

            let world = get_app_data::<app_data::World>(lua)?;
            for (entity, blueprint, name, owned_by, room, position) in world
                .0
                .query::<(
                    hecs::Entity,
                    &Blueprint,
                    Option<&Name>,
                    Option<&OwnedBy>,
                    Option<&Room>,
                    Option<&Position>,
                )>()
                .iter()
            {
                if self.matches(blueprint, name, owned_by, room, position) {
                    entities.push((entity, blueprint.0));
                }
            }

            entities
        };

        let mut processed_entities = Vec::new();
        let mut rx_sentry = RxSentry::new(self.pipeline.clone());
        for entity in entities {
            let args = EntityHandle {
                entity: entity.0,
                blueprint_id: entity.1,
            }
            .into_lua_multi(lua)?;

            match rx_sentry.process(args) {
                Ok(Some(_)) => {
                    processed_entities.push(entity);
                }

                Ok(None)
                | Err(RxSentryError::Core(CoreSentryError::Skipping))
                | Err(RxSentryError::Core(CoreSentryError::Throttled)) => {
                    continue;
                }

                Err(RxSentryError::Core(CoreSentryError::LimitReached(_))) => {
                    break;
                }
                Err(RxSentryError::Op(err)) => {
                    return Err(err);
                }
            }
        }

        Ok(processed_entities)
    }
}

impl HasPipeline for QueryRx {
    fn pipeline(&self) -> &RxPipeline {
        &self.pipeline
    }
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.pipeline
    }
}

impl UserData for QueryRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_core_pipeline_methods(methods);
        add_op_pipeline_methods(methods);

        methods.add_method("owned_by", |_, this, pid: PlayerId| {
            let mut next = this.clone();
            next.owned_by = Some(pid);
            Ok(next)
        });

        methods.add_method("named", |_, this, name: String| {
            let mut next = this.clone();
            next.named = Some(name);
            Ok(next)
        });

        methods.add_method("in_room", |lua, this, room: String| {
            let mut next = this.clone();
            next.in_room = Some(room_str_to_id(lua, &room)?);
            Ok(next)
        });

        methods.add_method("at", |lua, this, area: mlua::Value| {
            let area: Area = lua.from_value(area)?;

            let mut next = this.clone();
            next.area = Some(area);
            Ok(next)
        });

        methods.add_method("blueprint", |_, this, bp: mlua::AnyUserData| {
            let bp = bp.borrow::<EntityBlueprint>()?;

            let mut next = this.clone();
            next.blueprint_id = Some(bp.id());
            Ok(next)
        });

        methods.add_method("count", |lua, this, _: ()| {
            Ok(this.get_matching_entities(lua)?.len())
        });

        methods.add_method("first", |lua, this, _: ()| {
            Ok(this
                .get_matching_entities(lua)?
                .first()
                .map(|e| EntityHandle {
                    entity: e.0,
                    blueprint_id: e.1,
                }))
        });

        methods.add_method("each", |lua, this, handle: mlua::Function| {
            for entity in this.get_matching_entities(lua)? {
                let args = EntityHandle {
                    entity: entity.0,
                    blueprint_id: entity.1,
                }
                .into_lua_multi(lua)?;

                handle.call::<()>(args)?;
            }

            Ok(())
        });
    }
}
