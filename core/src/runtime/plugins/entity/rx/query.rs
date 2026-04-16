use mlua::{IntoLuaMulti, LuaSerdeExt, UserData};
use shared::{PlayerId, components::RadialArea};

use crate::{
    runtime::{
        app_data,
        network_replicator::protocol::RoomId,
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
    area: Option<RadialArea>,
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
            Some(position) => self.area.map_or(true, |area| {
                area.position.distance_squared(&position.0) <= area.radius * area.radius
            }),
            None => self.area.is_none(),
        };

        blueprint_check && ownership_check && name_check && room_check && pos_check
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
            let area: RadialArea = lua.from_value(area)?;

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
            let world = get_app_data::<app_data::World>(lua)?;
            let mut count: usize = 0;

            for (blueprint, name, owned_by, room, position) in world
                .0
                .query::<(
                    &Blueprint,
                    Option<&Name>,
                    Option<&OwnedBy>,
                    Option<&Room>,
                    Option<&Position>,
                )>()
                .iter()
            {
                if this.matches(blueprint, name, owned_by, room, position) {
                    count += 1;
                }
            }

            Ok(count)
        });

        methods.add_method("each", |lua, this, handle: mlua::Function| {
            let mut entities = Vec::new();
            {
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
                    if this.matches(blueprint, name, owned_by, room, position) {
                        entities.push((entity, blueprint.0));
                    }
                }
            };

            let mut rx_sentry = RxSentry::new(this.pipeline.clone());
            for entity in entities {
                let args = EntityHandle {
                    entity: entity.0,
                    blueprint_id: entity.1,
                }
                .into_lua_multi(lua)?;

                match rx_sentry.process(args) {
                    Ok(Some(args)) => {
                        handle.call::<()>(args)?;
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

            Ok(())
        });
    }
}
