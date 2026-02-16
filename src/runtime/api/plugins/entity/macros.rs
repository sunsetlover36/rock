macro_rules! for_each_component {
    ($methods:ident, $callback:ident) => {
        $callback!($methods, "vector", Vector2D, Vector2D);
        $callback!($methods, "transform", Transform2D, Transform2D);
        $callback!($methods, "control", Control, Control);
        $callback!($methods, "sprite", Sprite2D, Sprite2D);
        $callback!($methods, "sprite_char", SpriteChar, SpriteChar);
    };
}
pub(crate) use for_each_component;

macro_rules! add_blueprint_methods {
    ($methods:ident, $lua_name:literal, $variant:ident, $comp_type:ty) => {
        $methods.add_method($lua_name, |lua, this, data: mlua::Value| {
            let data: $comp_type = lua.from_value(data)?;
            let mut next = this.clone();
            next.components.push(ComponentData::$variant(data));
            Ok(next)
        });
    };
}
pub(crate) use add_blueprint_methods;

macro_rules! add_handle_methods {
    ($methods:ident, $lua_name:literal, $variant:ident, $comp_type:ty) => {
        $methods.add_method($lua_name, |lua, this, data: Option<mlua::Value>| {
            let mut app_data = lua
                .app_data_mut::<GameModeAppData>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            let world = &mut app_data.world;

            if let Some(v) = data {
                let comp_data: $comp_type = lua.from_value(v)?;

                // set
                if let Ok(mut field) = world.get::<&mut $comp_type>(this.entity) {
                    *field = comp_data;
                } else {
                    world.insert_one(this.entity, comp_data).map_err(|e| {
                        mlua::Error::runtime(format!(
                            "Failed to add a component to the entity in method `{}`: {}",
                            $lua_name, e
                        ))
                    })?;
                }

                return Ok(mlua::Value::UserData(lua.create_userdata(this.clone())?));
            } else {
                // get
                if let Ok(field) = world.get::<&$comp_type>(this.entity) {
                    return Ok(lua.to_value(&*field)?);
                } else {
                    return Ok(mlua::Value::Nil);
                }
            }
        });
    };
}
pub(crate) use add_handle_methods;
