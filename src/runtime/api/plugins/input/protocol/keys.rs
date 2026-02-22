use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde_repr::Deserialize_repr;
use strum::{AsRefStr, EnumIter};

// TODO: do i actually need this macro? remove it if serde deserialize works well
macro_rules! impl_from_lua_for_repr_enum {
    ($enum_ty:ty) => {
        impl mlua::FromLua for $enum_ty {
            fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
                match value {
                    mlua::Value::Integer(i) => {
                        let v =
                            u8::try_from(i).map_err(|_| mlua::Error::FromLuaConversionError {
                                from: "integer",
                                to: stringify!($enum_ty).into(),
                                message: Some("out of range".into()),
                            })?;

                        <$enum_ty>::try_from(v).map_err(|_| mlua::Error::FromLuaConversionError {
                            from: "integer",
                            to: stringify!($enum_ty).into(),
                            message: Some("invalid key value".into()),
                        })
                    }
                    _ => Err(mlua::Error::FromLuaConversionError {
                        from: value.type_name(),
                        to: stringify!($enum_ty).into(),
                        message: Some("expected integer".into()),
                    }),
                }
            }
        }
    };
}

#[derive(
    Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive, Deserialize_repr, EnumIter, AsRefStr,
)]
#[repr(u8)]
pub(crate) enum KeyboardKey {
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyT,
    KeyY,
    KeyU,
    KeyI,
    KeyO,
    KeyP,
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyG,
    KeyH,
    KeyJ,
    KeyK,
    KeyL,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyN,
    KeyM,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    Space,
    Tab,
    CapsLock,
    Enter,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}
impl_from_lua_for_repr_enum!(KeyboardKey);

#[derive(
    Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive, Deserialize_repr, EnumIter, AsRefStr,
)]
#[repr(u8)]
pub(crate) enum MouseKey {
    Left,
    Right,
    Middle,
    Scroll,
}
impl_from_lua_for_repr_enum!(MouseKey);

#[derive(
    Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive, Deserialize_repr, EnumIter, AsRefStr,
)]
#[repr(u8)]
pub(crate) enum ControllerButton {
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    LeftStick,
    RightStick,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    ButtonY,
    ButtonA,
    ButtonX,
    ButtonB,
}
impl_from_lua_for_repr_enum!(ControllerButton);

#[derive(
    Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive, Deserialize_repr, EnumIter, AsRefStr,
)]
#[repr(u8)]
pub(crate) enum ControllerStick {
    LeftStick,
    RightStick,
}
impl_from_lua_for_repr_enum!(ControllerStick);
