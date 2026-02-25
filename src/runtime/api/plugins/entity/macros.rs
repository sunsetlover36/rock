// Component methods specification
macro_rules! for_each_blueprint {
    ($methods:ident, $callback:ident) => {
        $callback!($methods, "vector", Vector2D, Vector2D);
        $callback!($methods, "transform", Transform2D, Transform2D);
        $callback!($methods, "control", Control, Control);
        $callback!($methods, "sprite", Sprite2D, Sprite2D);
        $callback!($methods, "sprite_char", SpriteChar, SpriteChar);
        $callback!($methods, "owned_by", OwnedBy, OwnedBy);
    };
}
pub(crate) use for_each_blueprint;

macro_rules! for_each_handle {
    ($methods:ident, $callback:ident) => {
        $callback!($methods, "vector", Vector2D, Vector2D);
        $callback!($methods, "transform", Transform2D, Transform2D);
        $callback!($methods, "control", Control, Control);
        $callback!($methods, "sprite", Sprite2D, Sprite2D);
        $callback!($methods, "sprite_char", SpriteChar, SpriteChar);
        $callback!($methods, "owned_by", OwnedBy, OwnedBy);
        $callback!($methods, "name", Name, Name);
    };
}
pub(crate) use for_each_handle;

// Methods inclusion
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
            let event_bus = lua
                .app_data_ref::<app_data::EventBus>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                .clone();
            let mut world = lua
                .app_data_mut::<app_data::World>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;

            if let Some(v) = data {
                let comp_data: $comp_type = lua.from_value(v)?;

                // set
                if let Ok(mut field) = world.get::<&mut $comp_type>(this.entity) {
                    *field = comp_data.clone();
                } else {
                    world
                        .insert_one(this.entity, comp_data.clone())
                        .map_err(|e| {
                            mlua::Error::runtime(format!(
                                "Failed to add a component to the entity in method `{}`: {}",
                                $lua_name, e
                            ))
                        })?;
                }

                event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![
                        EventScope::Entity(this.entity.id().into()),
                        EventScope::Blueprint(this.blueprint_id),
                    ],
                    data: GameModeEventData::Entity(EntityEventData::ComponentUpdate(
                        ComponentData::$variant(comp_data),
                    )),
                });
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
