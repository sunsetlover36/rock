use mlua::UserData;
use shared::PlayerKey;

use crate::runtime::{
    app_data, get_app_data,
    plugins::entity::{EntityHandle, components::Room},
};

pub(super) struct PlayerVision {
    pk: PlayerKey,
}
impl PlayerVision {
    pub fn new(pk: PlayerKey) -> Self {
        Self { pk }
    }
}
impl UserData for PlayerVision {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "attach",
            |lua, this, handle: mlua::UserDataRef<EntityHandle>| {
                let world = get_app_data::<app_data::World>(lua)?;
                let room_id = world
                    .get::<&Room>(handle.entity)
                    .map_or(None, |room_comp| Some(room_comp.0));

                get_app_data::<app_data::NetworkReplicator>(lua)?.add_player_anchor(
                    this.pk,
                    handle.entity,
                    room_id,
                );

                Ok(())
            },
        );

        methods.add_method(
            "detach",
            |lua, this, handle: Option<mlua::UserDataRef<EntityHandle>>| {
                match handle {
                    Some(handle) => {
                        let world = get_app_data::<app_data::World>(lua)?;
                        let room_id = world
                            .get::<&Room>(handle.entity)
                            .map_or(None, |room_comp| Some(room_comp.0));

                        get_app_data::<app_data::NetworkReplicator>(lua)?.remove_player_anchor(
                            this.pk,
                            handle.entity,
                            room_id,
                        );
                    }
                    None => {
                        get_app_data::<app_data::NetworkReplicator>(lua)?
                            .clear_player_anchors(lua, this.pk)?;
                    }
                }

                Ok(())
            },
        );
    }
}
