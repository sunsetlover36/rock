use std::{collections::HashMap, rc::Rc, sync::Arc};

use color_eyre::eyre;
use shared::{InputAction, InputKind};

use crate::runtime::{
    GameModeClientApi, event_bus, network_replicator,
    plugins::{
        entity::{BlueprintId, EntityBlueprint},
        input::protocol::InputEvent,
        layer::{LayerEntry, LayerId},
        on::{GameModeListener, protocol::GameModeEventKey},
        protocol::PluginName,
    },
    timer_manager,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimePhase {
    Glyphs,
    Blueprints,
    Systems,
    Gamemode,
}

pub type EventListeners = HashMap<GameModeEventKey, Vec<GameModeListener>>;
pub type Scenes = HashMap<String, Vec<mlua::Function>>;
pub type ScenePlugins = HashMap<PluginName, mlua::Table>;
pub type Yielder = Option<mlua::Function>;
pub type World = hecs::World;
pub type EventBus = Rc<event_bus::EventBus>;

pub struct BlueprintRegistry {
    last_id: BlueprintId,
    pub blueprints: HashMap<String, EntityBlueprint>,
}
impl BlueprintRegistry {
    pub fn new() -> Self {
        Self {
            last_id: 0,
            blueprints: HashMap::new(),
        }
    }

    pub fn increment_id(&mut self) -> BlueprintId {
        self.last_id += 1;
        self.last_id
    }
}

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

#[derive(Debug, Clone, Copy)]
pub enum ExecutionContext {
    Global,
    Impromptu,
}

// Layer management
// -- Registry
pub struct LayerRegistry {
    last_id: LayerId,
    pub layers: HashMap<LayerId, LayerEntry>,
    pub aliases: HashMap<String, LayerId>,
}
impl LayerRegistry {
    pub fn new() -> Self {
        Self {
            last_id: 0,
            layers: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn increment_id(&mut self) -> LayerId {
        self.last_id += 1;
        self.last_id
    }
}

// -- Active layers at initialization phase
pub type ActiveLayers = Vec<LayerId>;

pub type ClientApi = Arc<dyn GameModeClientApi>;
pub type TimerManager = Rc<timer_manager::TimerManager>;
pub type NetworkReplicator = Rc<network_replicator::NetworkReplicator>;
