use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Vector2D {
    x: u32,
    y: u32,
}
