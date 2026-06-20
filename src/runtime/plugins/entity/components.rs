use serde::Serialize;
use strum::{AsRefStr, EnumDiscriminants, EnumIter, EnumString};

mod position;
pub(crate) use position::Position;

mod rotation;
pub(crate) use rotation::Rotation;

mod owned_by;
pub(crate) use owned_by::OwnedBy;

mod blueprint;
pub(crate) use blueprint::Blueprint;

mod name;
pub(crate) use name::Name;

mod room;
pub(crate) use room::Room;

#[derive(Debug, EnumDiscriminants, Clone, Serialize)]
#[serde(untagged)]
#[strum_discriminants(name(ComponentKey))]
#[strum_discriminants(derive(Hash, EnumIter, EnumString, AsRefStr))]
#[strum_discriminants(strum(serialize_all = "snake_case"))]
pub(crate) enum ComponentData {
    Position(Position),
    Rotation(Rotation),
    OwnedBy(OwnedBy),
    Blueprint(Blueprint),
    Name(Name),
    Room(Room),
}
