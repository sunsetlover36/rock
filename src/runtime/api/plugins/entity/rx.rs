use mlua::{IntoLuaMulti, UserData};
use shared::PlayerId;

use crate::{
    runtime::{
        api::plugins::entity::{
            components::{Blueprint, OwnedBy},
            handle::EntityHandle,
        },
        app_data,
        utils::get_app_data,
    },
    rx::{HasRxPipeline, RxPipeline, add_rx_methods},
};

#[derive(Clone)]
pub(crate) struct EntityRx {
    owned_by: Option<PlayerId>,
    pipeline: RxPipeline,
}
impl EntityRx {
    pub fn new() -> Self {
        EntityRx {
            owned_by: None,
            pipeline: RxPipeline::default(),
        }
    }
}
impl HasRxPipeline for EntityRx {
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.pipeline
    }
}
impl UserData for EntityRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_rx_methods(methods);

        methods.add_method("owned_by", |_, this, pid: PlayerId| {
            let mut next = this.clone();
            next.owned_by = Some(pid);
            Ok(next)
        });

        methods.add_method("each", |lua, this, handle: mlua::Function| {
            let mut entities = Vec::new();
            {
                let world = get_app_data::<app_data::World>(lua)?;

                for (entity, owned_by, blueprint) in
                    world.query::<(hecs::Entity, &OwnedBy, &Blueprint)>().iter()
                {
                    if this.owned_by == Some(owned_by.0) {
                        entities.push((entity, blueprint.0));
                    }
                }
            };

            for entity in entities {
                // TODO: keep that in mind, might be optimized
                let args = this.pipeline.process(
                    EntityHandle {
                        entity: entity.0,
                        blueprint_id: entity.1,
                    }
                    .into_lua_multi(lua)?,
                )?;
                if let Some(args) = args {
                    handle.call::<()>(args)?;
                }
            }

            Ok(())
        });
    }
}
