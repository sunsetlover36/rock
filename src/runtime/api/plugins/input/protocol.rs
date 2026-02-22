use strum::{AsRefStr, EnumDiscriminants, EnumIter, EnumString};

mod keys;
pub(crate) use keys::*;

#[derive(Debug, Copy, Clone, EnumDiscriminants)]
#[strum_discriminants(name(InputSource))]
#[strum_discriminants(derive(EnumString, AsRefStr, EnumIter))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub(crate) enum InputKey {
    Keyboard(KeyboardKey),
    Mouse(MouseKey),
    Controller(ControllerButton),
    Stick(ControllerStick),
}

#[derive(Debug, Clone)]
pub(crate) struct Vector2DKeyboardBindings {
    pub up: Vec<KeyboardKey>,
    pub down: Vec<KeyboardKey>,
    pub left: Vec<KeyboardKey>,
    pub right: Vec<KeyboardKey>,
}

#[derive(Debug, Clone)]
pub(crate) struct Vector2DControllerBindings {
    pub up: Vec<ControllerButton>,
    pub down: Vec<ControllerButton>,
    pub left: Vec<ControllerButton>,
    pub right: Vec<ControllerButton>,
}

#[derive(Debug, Clone)]
pub(crate) struct Vector2DBindings {
    pub keyboard: Option<Vector2DKeyboardBindings>,
    pub controller: Option<Vector2DControllerBindings>,
    pub stick: Option<ControllerStick>,
}

#[derive(Debug, Clone)]
pub(crate) struct ButtonBindings {
    pub keyboard: Option<Vec<KeyboardKey>>,
    pub mouse: Option<Vec<MouseKey>>,
    pub controller: Option<Vec<ControllerButton>>,
}

#[derive(Debug, Clone)]
pub(crate) struct AxisKeyboardBindings {
    negative: Vec<KeyboardKey>,
    positive: Vec<KeyboardKey>,
}

#[derive(Debug, Clone)]
pub(crate) struct AxisControllerBindings {
    negative: Vec<ControllerButton>,
    positive: Vec<ControllerButton>,
}

#[derive(Debug, Clone)]
pub(crate) struct AxisBindings {
    pub keyboard: Option<AxisKeyboardBindings>,
    pub controller: Option<AxisControllerBindings>,
    pub stick: Option<ControllerStick>,
}

#[derive(Debug, Clone)]
pub enum DefaultBinding {
    Vector2D(Vector2DBindings),
    Button(ButtonBindings),
    Axis(AxisBindings),
}
