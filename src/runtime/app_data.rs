use std::{collections::HashMap, rc::Rc};

use color_eyre::eyre;
use shared::{InputAction, InputKind};

use crate::runtime::{
    EventBus as EventBusStruct,
    api::{
        EntityBlueprint, InputEvent,
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
pub type Blueprints = HashMap<String, EntityBlueprint>;

#[derive(Default, Debug, Clone)]
pub struct InputEventRegistry {
    pub events: Vec<InputEvent>,
    pub name_to_id: HashMap<String, usize>,
}
impl InputEventRegistry {
    pub fn get_action_name(&self, action: InputAction) -> eyre::Result<Rc<str>> {
        if let Some(event) = self.events.get(action.id as usize) {
            let action_kind: InputKind = action.data.into();
            let event_kind = event.bindings.kind();
            if action_kind != event_kind {
                return Err(eyre::eyre!(
                    "Type mismatch for event '{}': expected {:?}, got {:?}",
                    event.name,
                    event_kind,
                    action_kind
                ));
            }

            Ok(event.name.clone())
        } else {
            Err(eyre::eyre!("Unknown input event name"))
        }
    }
}
