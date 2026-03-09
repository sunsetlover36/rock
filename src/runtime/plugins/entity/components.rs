use serde::Serialize;
use strum::{AsRefStr, EnumDiscriminants, EnumIter, EnumString};

mod position;
pub(crate) use position::Position;

mod rotation;
pub(crate) use rotation::Rotation;

mod control;
pub(crate) use control::Control;

mod sprite_2d;
pub(crate) use sprite_2d::Sprite2D;

mod sprite_char;
pub(crate) use sprite_char::SpriteChar;

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
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub(crate) enum ComponentData {
    Position(Position),
    Rotation(Rotation),
    Control(Control),
    Sprite2D(Sprite2D),
    SpriteChar(SpriteChar),
    OwnedBy(OwnedBy),
    Blueprint(Blueprint),
    Name(Name),
    Room(Room),
}
