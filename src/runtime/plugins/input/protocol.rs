use std::rc::Rc;

use serde::Deserialize;
use rock_wire::InputKind;
use strum::{AsRefStr, EnumDiscriminants, EnumString};

mod keys;
pub(crate) use keys::*;

#[derive(Debug, Copy, Clone, EnumDiscriminants)]
#[strum_discriminants(name(InputSource))]
#[strum_discriminants(derive(EnumString, AsRefStr))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub(crate) enum InputKey {
    Keyboard(KeyboardKey),
    Mouse(MouseKey),
    Controller(ControllerButton),
    Stick(ControllerStick),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Vector2DKeyboardBindings {
    pub up: Vec<KeyboardKey>,
    pub down: Vec<KeyboardKey>,
    pub left: Vec<KeyboardKey>,
    pub right: Vec<KeyboardKey>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Vector2DControllerBindings {
    pub up: Vec<ControllerButton>,
    pub down: Vec<ControllerButton>,
    pub left: Vec<ControllerButton>,
    pub right: Vec<ControllerButton>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Vector2DBindings {
    pub keyboard: Option<Vector2DKeyboardBindings>,
    pub controller: Option<Vector2DControllerBindings>,
    pub stick: Option<ControllerStick>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ButtonBindings {
    pub keyboard: Option<Vec<KeyboardKey>>,
    pub mouse: Option<Vec<MouseKey>>,
    pub controller: Option<Vec<ControllerButton>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AxisKeyboardBindings {
    negative: Vec<KeyboardKey>,
    positive: Vec<KeyboardKey>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AxisControllerBindings {
    negative: Vec<ControllerButton>,
    positive: Vec<ControllerButton>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AxisBindings {
    pub keyboard: Option<AxisKeyboardBindings>,
    pub controller: Option<AxisControllerBindings>,
    pub stick: Option<ControllerStick>,
}

#[derive(Debug, Clone)]
pub(crate) enum InputBindings {
    Vector2D(Vector2DBindings),
    Button(ButtonBindings),
    Axis(AxisBindings),
}
impl InputBindings {
    pub fn kind(&self) -> InputKind {
        match self {
            InputBindings::Vector2D(_) => InputKind::Vector2D,
            InputBindings::Button(_) => InputKind::Button,
            InputBindings::Axis(_) => InputKind::Axis,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InputEvent {
    pub name: Rc<str>,
    pub bindings: InputBindings,
}
