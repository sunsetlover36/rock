use mlua::{Lua, Value as LuaValue};
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
