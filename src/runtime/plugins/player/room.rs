use mlua::UserData;
use shared::PlayerKey;

use crate::runtime::{app_data, get_app_data, room_str_to_id};

pub(super) struct PlayerRoom {
    pk: PlayerKey,
}
impl PlayerRoom {
    pub fn new(pk: PlayerKey) -> Self {
        Self { pk }
    }
}
impl UserData for PlayerRoom {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("enter", |lua, this, name: String| {
            get_app_data::<app_data::NetworkReplicator>(lua)?
                .0
                .add_player_to_room(lua, this.pk, room_str_to_id(lua, &name)?)?;
            Ok(())
        });

        methods.add_method("exit", |lua, this, name: Option<String>| {
            let replicator_data = get_app_data::<app_data::NetworkReplicator>(lua)?;
            let replicator = &replicator_data.0;

            match name {
                Some(name) => {
                    replicator.remove_player_from_room(
                        lua,
                        this.pk,
                        room_str_to_id(lua, &name)?,
                    )?;
                }
                None => {
                    replicator.clear_player_rooms(lua, this.pk)?;
                }
            }
            Ok(())
        });
    }
}
