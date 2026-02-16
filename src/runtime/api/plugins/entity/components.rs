use strum::EnumDiscriminants;

mod vector_2d;
pub(crate) use vector_2d::Vector2D;

mod transform_2d;
pub(crate) use transform_2d::Transform2D;

mod control;
pub(crate) use control::Control;

mod sprite_2d;
pub(crate) use sprite_2d::Sprite2D;

mod sprite_char;
pub(crate) use sprite_char::SpriteChar;

#[derive(EnumDiscriminants, Clone)]
#[strum_discriminants(name(ComponentKey))]
#[strum_discriminants(derive(Hash))]
pub(crate) enum ComponentData {
    Vector2D(Vector2D),
    Transform2D(Transform2D),
    Control(Control),
    Sprite2D(Sprite2D),
    SpriteChar(SpriteChar),
}
pub(crate) struct CustomDataComponent(pub mlua::RegistryKey);
