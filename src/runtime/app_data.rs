use std::{collections::HashMap, rc::Rc};

use crate::runtime::{
    EventBus as EventBusStruct,
    api::{
        on::{GameModeEventKey, GameModeListener},
        protocol::PluginName,
    },
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimePhase {
    Glyphs,
    Blueprints,
    Systems,
    Gamemode,
}

pub type EventListeners = HashMap<GameModeEventKey, Vec<GameModeListener>>;
pub type Scenes = HashMap<String, mlua::Function>;
pub type ScenePlugins = HashMap<PluginName, mlua::Table>;
pub type Yielder = Option<mlua::Function>;
pub type World = hecs::World;
pub type EventBus = Rc<EventBusStruct>;
