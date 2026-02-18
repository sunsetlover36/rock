use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SpriteChar {
    char: String,
    color: String,
    bg_color: String,
    visible: bool,
}
