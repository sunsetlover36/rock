use mlua::UserData;
use rock_wire::{PlayerKey, farcaster::Fid};

#[derive(Clone)]
pub(crate) struct PlayerSnapshot {
    pk: PlayerKey,
    identity: Option<String>,
}

impl PlayerSnapshot {
    pub fn new(pk: PlayerKey, identity: Option<String>) -> Self {
        Self { pk, identity }
    }

    fn fid_from_identity(identity: &str) -> Option<Fid> {
        identity.strip_prefix("fc:")?.parse::<Fid>().ok()
    }
}

impl UserData for PlayerSnapshot {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, _: ()| Ok(this.pk.pack()));

        methods.add_method("who", |_, this, _: ()| Ok(this.identity.clone()));

        methods.add_method("fid", |_, this, _: ()| {
            Ok(this
                .identity
                .as_deref()
                .and_then(PlayerSnapshot::fid_from_identity))
        });
    }
}
