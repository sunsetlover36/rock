use mlua::{LuaSerdeExt, UserData};
use rock_wire::{OutgoingPacket, PlayerKey, SignalPacket, components::RadialArea};

use crate::{
    envelope::{EnvelopeRecipient, ServerEnvelope},
    runtime::{app_data, get_app_data, network_replicator::protocol::RoomId, room_str_to_id},
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::runtime::plugins::player) enum SignalScope {
    Global,
    Player(PlayerKey),
}

#[derive(Clone)]
pub(in crate::runtime::plugins::player) struct SignalRx {
    scope: SignalScope,
    name: Option<String>,
    data: Option<serde_json::Map<String, serde_json::Value>>,
    area: Option<RadialArea>,
    room: Option<RoomId>,
}
impl SignalRx {
    pub fn new(scope: SignalScope, name: Option<String>) -> Self {
        Self {
            scope,
            name,
            data: None,
            area: None,
            room: None,
        }
    }
}
impl UserData for SignalRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("data", |lua, this, data: mlua::Table| {
            let mut next = this.clone();
            next.data = lua.from_value(mlua::Value::Table(data))?;
            Ok(next)
        });

        methods.add_method("area", |lua, this, area: mlua::Table| {
            if this.scope != SignalScope::Global {
                return Err(mlua::Error::runtime(
                    "Cannot set `:area()` constraint for a signal tied to a specific player",
                ));
            }

            let area: RadialArea = lua.from_value(mlua::Value::Table(area))?;
            let mut next = this.clone();
            next.area = Some(area);
            Ok(next)
        });

        methods.add_method("room", |lua, this, name: String| {
            if this.scope != SignalScope::Global {
                return Err(mlua::Error::runtime(
                    "Cannot set `:room()` constraint for a signal tied to a specific player",
                ));
            }

            let mut next = this.clone();
            next.room = Some(room_str_to_id(lua, &name)?);
            Ok(next)
        });

        methods.add_method("send", |lua, this, _: ()| {
            let client_api_data = get_app_data::<app_data::ClientApi>(lua)?;
            let client_api = &client_api_data.0;

            let data = this.data.clone().ok_or_else(|| {
                mlua::Error::runtime("Failed to send a signal: no data to send was provided")
            })?;
            let payload = OutgoingPacket::Signal(SignalPacket {
                name: this.name.clone(),
                data,
            });

            match this.scope {
                SignalScope::Global => match this.room {
                    Some(room_id) => {
                        let replicator_data = get_app_data::<app_data::NetworkReplicator>(lua)?;
                        let replicator = &replicator_data.0;

                        match this.area {
                            Some(area) => {
                                let world = get_app_data::<app_data::World>(lua)?;
                                client_api.send(ServerEnvelope {
                                    recipient: EnvelopeRecipient::List(
                                        replicator.get_players_in_area(&world.0, room_id, area),
                                    ),
                                    payload,
                                });
                            }
                            None => {
                                let players = replicator.get_players_in_room(room_id);
                                client_api.send(ServerEnvelope {
                                    recipient: EnvelopeRecipient::List(players),
                                    payload,
                                });
                            }
                        }
                    }
                    None => client_api.send(ServerEnvelope {
                        recipient: EnvelopeRecipient::All,
                        payload,
                    }),
                },
                SignalScope::Player(pk) => {
                    client_api.send(ServerEnvelope {
                        recipient: EnvelopeRecipient::Single(pk),
                        payload,
                    });
                }
            }

            Ok(())
        });
    }
}
