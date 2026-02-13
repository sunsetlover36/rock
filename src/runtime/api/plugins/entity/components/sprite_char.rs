use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SpriteChar {
    char: String,
    color: String,
    bg_color: String,
    visible: bool,
}
