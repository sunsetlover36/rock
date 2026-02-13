use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Control {
    speed: u32,
}
