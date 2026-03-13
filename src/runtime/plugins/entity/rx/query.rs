use mlua::{IntoLuaMulti, UserData};
use shared::PlayerId;

use crate::{
    runtime::{
        app_data,
        plugins::entity::{
            components::{Blueprint, Name, OwnedBy},
            handle::EntityHandle,
        },
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
    pipeline: RxPipeline,
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

        methods.add_method("each", |lua, this, handle: mlua::Function| {
            let mut entities = Vec::new();
            {
                let world = get_app_data::<app_data::World>(lua)?;

                for (entity, name, owned_by, blueprint) in world
                    .query::<(hecs::Entity, Option<&Name>, Option<&OwnedBy>, &Blueprint)>()
                    .iter()
                {
                    let ownership_check = match owned_by {
                        Some(owned_by) => this.owned_by.map_or(true, |owner| owner == owned_by.0),
                        None => this.owned_by.is_none(),
                    };
                    let name_check = match name {
                        Some(name) => this
                            .named
                            .as_ref()
                            .map_or(true, |filter_name| filter_name == &name.0),
                        None => this.named.is_none(),
                    };

                    if ownership_check && name_check {
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
