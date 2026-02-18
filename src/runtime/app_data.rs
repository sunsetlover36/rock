use std::{collections::HashMap, rc::Rc};

use crate::runtime::{
    EventBus,
    api::on::{GameModeEventKey, GameModeListener},
};

pub struct GameModeAppData {
    pub event_listeners: HashMap<GameModeEventKey, Vec<GameModeListener>>,
    pub scenes: HashMap<String, mlua::Function>,
    pub scene_plugins: HashMap<String, mlua::Table>,
    pub yielder: Option<mlua::Function>,
    pub world: hecs::World,
    pub event_bus: Rc<EventBus>,
}
