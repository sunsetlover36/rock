use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;

pub fn json_to_lua(lua: &Lua, value: JsonValue) -> mlua::Result<LuaValue> {
    Ok(match value {
        JsonValue::Null => LuaValue::Nil,

        JsonValue::Bool(b) => LuaValue::Boolean(b),

        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                LuaValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                LuaValue::Number(f)
            } else {
                LuaValue::Nil
            }
        }

        JsonValue::String(s) => LuaValue::String(lua.create_string(&s)?),

        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.into_iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            LuaValue::Table(table)
        }

        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k, json_to_lua(lua, v)?)?;
            }
            LuaValue::Table(table)
        }
    })
}

pub fn multivalue_to_json(lua: &Lua, mv: mlua::MultiValue) -> mlua::Result<String> {
    match mv.len() {
        0 => Ok("null".to_owned()),
        1 => {
            let v: serde_json::Value =
                lua.from_value::<serde_json::Value>(mv.into_iter().next().unwrap())?;
            Ok(serde_json::to_string(&v).map_err(mlua::Error::runtime)?)
        }
        _ => {
            let arr: Vec<serde_json::Value> = mv
                .into_iter()
                .map(|v| lua.from_value(v))
                .collect::<mlua::Result<_>>()?;
            Ok(serde_json::to_string(&arr).map_err(mlua::Error::runtime)?)
        }
    }
}

pub fn custom_table_to_json(
    lua: &mlua::Lua,
    custom: Option<&mlua::Table>,
) -> mlua::Result<serde_json::Map<String, serde_json::Value>> {
    let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    if let Some(custom) = custom {
        for pair in custom.pairs::<String, mlua::Value>() {
            let (key, value) = pair?;
            map.insert(key, lua.from_value(value)?);
        }
    }

    Ok(map)
}

pub fn is_json_empty(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Array(arr) => arr.is_empty(),
        serde_json::Value::Object(map) => map.is_empty(),
        _ => false,
    }
}
