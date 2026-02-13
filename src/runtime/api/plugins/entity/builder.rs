use mlua::{LuaSerdeExt, UserData};

use crate::runtime::api::plugins::entity::components::{
    ComponentVariant, Control, Sprite2D, SpriteChar, Transform2D, Vector2D,
};

macro_rules! register_components {
    ($methods:expr, {
        $($name:ident : $variant:ident($type:ty)),* $(,)?
    }) => {
        $(
            $methods.add_method(stringify!($name), |lua, builder, value: mlua::Value| {
                builder.ensure_not_finsihed()?;

                let data: $type = lua.from_value(value)?;

                let mut next = builder.clone();
                next.components.push(ComponentVariant::$variant(data));
                Ok(next)
            })
        )*
    }
}

#[derive(Clone)]
pub(super) struct EntityBuilder {
    name: String,
    components: Vec<ComponentVariant>,
    custom_data: Option<mlua::Table>,
    finished: bool,
}
impl EntityBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            components: Vec::new(),
            custom_data: None,
            finished: false,
        }
    }
    fn ensure_not_finished(&self) -> mlua::Result<()> {
        if self.finished {
            Err(mlua::Error::runtime(
                "This entity was already defined. Define a new one",
            ))
        } else {
            Ok(())
        }
    }
}
impl UserData for EntityBuilder {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        register_components!(methods, {
            vector: Vector2D(Vector2D),
            transform: Transform2D(Transform2D),
            control: Control(Control),
            sprite: Sprite2D(Sprite2D),
            sprite_char: SpriteChar(SpriteChar),
        });

        methods.add_method("custom", |_, this, value: mlua::Table| {
            this.ensure_not_finsihed()?;

            let mut next = this.clone();
            next.custom_data = Some(value);
            Ok(next)
        });

        methods.add_method_mut("finish", |lua, this, _: ()| {
            this.finished = true;
        });
    }
}
