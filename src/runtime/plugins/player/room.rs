use mlua::UserData;
use shared::PlayerKey;

use crate::runtime::{app_data, get_app_data, get_str_hash};

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
                .add_player_to_room(this.pk, get_str_hash(&name));
            Ok(())
        });

        methods.add_method("exit", |lua, this, name: Option<String>| {
            let replicator = get_app_data::<app_data::NetworkReplicator>(lua)?;
            match name {
                Some(name) => {
                    replicator.remove_player_from_room(this.pk, get_str_hash(&name));
                }
                None => {
                    replicator.clear_player_rooms(this.pk);
                }
            }
            Ok(())
        });
    }
}
