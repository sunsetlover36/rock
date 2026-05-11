use std::{collections::HashMap, rc::Rc, sync::Arc};

use color_eyre::eyre;
use rock_wire::{InputAction, InputKind};

use crate::runtime::{
    GameModeClientApi, event_bus,
    network_replicator::{
        self,
        protocol::{ReplicationMark, RoomId},
    },
    plugins::{
        entity::{BlueprintId, EntityBlueprint},
        input::protocol::InputEvent,
        layer::{LayerEntry, LayerId},
        on::{GameModeListener, protocol::GameModeEventKey},
        protocol::PluginName,
    },
    timer_manager,
};

pub struct EventListeners(pub HashMap<GameModeEventKey, Vec<GameModeListener>>);
pub struct Scenes(pub HashMap<String, Vec<mlua::Function>>);
pub struct ScenePlugins(pub HashMap<PluginName, mlua::Table>);
pub struct Yielder(pub Option<mlua::Function>);
pub struct World(pub hecs::World);
pub struct EventBus(pub Rc<event_bus::EventBus>);

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
pub struct ActiveLayers(pub Vec<LayerId>);
pub struct ClientApi(pub Arc<dyn GameModeClientApi>);
pub struct TimerManager(pub Rc<timer_manager::TimerManager>);
pub struct EntityCustoms(pub HashMap<hecs::Entity, mlua::Table>);

pub struct NetworkReplicator(pub Rc<network_replicator::NetworkReplicator>);
pub struct RoomIdToName(pub HashMap<RoomId, String>);

#[derive(Clone)]
pub struct ReplicatorMarkTx(pub flume::Sender<ReplicationMark>);
