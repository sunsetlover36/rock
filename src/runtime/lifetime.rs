//! Synchronous ownership context and cancellation shared by async work.
use std::{
    collections::HashMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use parking_lot::Mutex;
use rock_wire::PlayerKey;
use tokio::task::AbortHandle;

use super::{
    app_data::{self, ExecutionContext},
    get_app_data, get_app_data_mut,
    plugins::layer::{LayerContextGuard, LayerId, current_layer, layer_is_alive},
};

#[derive(Debug)]
struct CancellationState {
    active: AtomicBool,
    abort: Mutex<Option<AbortHandle>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Cancellation(Arc<CancellationState>);
impl Cancellation {
    pub fn new() -> Self {
        Self(Arc::new(CancellationState {
            active: AtomicBool::new(true),
            abort: Mutex::new(None),
        }))
    }
    pub fn get(&self) -> bool {
        self.0.active.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.active.store(false, Ordering::Release);
        if let Some(abort) = self.0.abort.lock().take() {
            abort.abort();
        }
    }
    pub fn set_abort(&self, abort: AbortHandle) {
        let mut slot = self.0.abort.lock();
        if self.get() {
            *slot = Some(abort);
        } else {
            abort.abort();
        }
    }
    pub fn clear_abort(&self) {
        self.0.abort.lock().take();
    }
}

#[derive(Default)]
pub(crate) struct PlayerWorkRegistry(HashMap<PlayerKey, Vec<Weak<CancellationState>>>);
#[derive(Default)]
pub(crate) struct ActivePlayers(pub Vec<PlayerKey>);

pub(crate) fn current_players(lua: &mlua::Lua) -> Vec<PlayerKey> {
    lua.app_data_ref::<ActivePlayers>()
        .map(|p| p.0.clone())
        .unwrap_or_default()
}
pub(crate) fn players_online(lua: &mlua::Lua, players: &[PlayerKey]) -> mlua::Result<bool> {
    if players.is_empty() {
        return Ok(true);
    }
    let api = get_app_data::<app_data::ClientApi>(lua)?;
    Ok(players.iter().all(|pk| api.0.has(*pk)))
}
pub(crate) fn register_players(lua: &mlua::Lua, players: &[PlayerKey], cancel: &Cancellation) {
    if players.is_empty() {
        return;
    }
    if lua.app_data_ref::<PlayerWorkRegistry>().is_none() {
        lua.set_app_data(PlayerWorkRegistry::default());
    }
    let mut registry = lua.app_data_mut::<PlayerWorkRegistry>().unwrap();
    for pk in players {
        let entries = registry.0.entry(*pk).or_default();
        entries.retain(|entry| {
            entry
                .upgrade()
                .is_some_and(|state| state.active.load(Ordering::Acquire))
        });
        entries.push(Arc::downgrade(&cancel.0));
    }
}
pub(crate) fn cancel_player(lua: &mlua::Lua, pk: PlayerKey) {
    let entries = lua
        .app_data_mut::<PlayerWorkRegistry>()
        .and_then(|mut r| r.0.remove(&pk));
    for entry in entries.into_iter().flatten() {
        if let Some(state) = entry.upgrade() {
            Cancellation(state).cancel();
        }
    }
    if let Some(mut listeners) = lua.app_data_mut::<app_data::EventListeners>() {
        for listeners in listeners.0.values_mut() {
            listeners.retain(|l| l.active.get());
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WorkContext {
    pub layer: Option<LayerId>,
    pub players: Vec<PlayerKey>,
    pub execution: ExecutionContext,
    pub origin: Option<super::diagnostics::SnippetSource>,
}
impl WorkContext {
    pub fn capture(lua: &mlua::Lua, explicit: &[PlayerKey]) -> mlua::Result<Self> {
        let mut players = current_players(lua);
        for pk in explicit {
            if !players.contains(pk) {
                players.push(*pk);
            }
        }
        let context = Self {
            layer: current_layer(lua)?,
            players,
            execution: *get_app_data::<ExecutionContext>(lua)?,
            origin: super::diagnostics::current_source(lua),
        };
        if !context.is_alive(lua)? {
            return Err(mlua::Error::runtime(
                "Cannot start work: a dependent player is disconnected or layer is cleared",
            ));
        }
        Ok(context)
    }
    pub fn is_alive(&self, lua: &mlua::Lua) -> mlua::Result<bool> {
        if let Some(layer) = self.layer {
            if !layer_is_alive(lua, layer)? {
                return Ok(false);
            }
        }
        players_online(lua, &self.players)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OwnedWork {
    pub context: WorkContext,
    pub cancel: Cancellation,
}
impl OwnedWork {
    pub fn register(lua: &mlua::Lua, context: WorkContext) -> mlua::Result<Self> {
        let cancel = Cancellation::new();
        if let Some(layer) = context.layer {
            let weak = Arc::downgrade(&cancel.0);
            let cleanup = lua.create_function(move |_, ()| {
                if let Some(state) = weak.upgrade() {
                    Cancellation(state).cancel();
                }
                Ok(())
            })?;
            get_app_data_mut::<app_data::LayerRegistry>(lua)?
                .layers
                .get_mut(&layer)
                .ok_or_else(|| mlua::Error::runtime("Owning layer is cleared"))?
                .cleaners
                .push(cleanup);
        }
        register_players(lua, &context.players, &cancel);
        Ok(Self { context, cancel })
    }
    pub fn is_alive(&self, lua: &mlua::Lua) -> mlua::Result<bool> {
        Ok(self.cancel.get() && self.context.is_alive(lua)?)
    }
}

pub(crate) struct WorkContextGuard<'lua> {
    lua: &'lua mlua::Lua,
    previous_players: Vec<PlayerKey>,
    previous_execution: ExecutionContext,
    previous_origin: Option<super::diagnostics::SnippetSource>,
    _layer: LayerContextGuard<'lua>,
}
impl<'lua> WorkContextGuard<'lua> {
    pub fn enter(lua: &'lua mlua::Lua, context: &WorkContext) -> mlua::Result<Self> {
        let previous_execution = *get_app_data::<ExecutionContext>(lua)?;
        let previous_players = current_players(lua);
        let previous_origin = super::diagnostics::current_source(lua);
        let layer = LayerContextGuard::enter(lua, context.layer)?;
        lua.set_app_data(ActivePlayers(context.players.clone()));
        lua.set_app_data(context.execution);
        lua.set_app_data(super::diagnostics::ActiveSnippet(context.origin.clone()));
        Ok(Self {
            lua,
            previous_players,
            previous_execution,
            previous_origin,
            _layer: layer,
        })
    }
}
impl Drop for WorkContextGuard<'_> {
    fn drop(&mut self) {
        self.lua
            .set_app_data(ActivePlayers(std::mem::take(&mut self.previous_players)));
        self.lua.set_app_data(self.previous_execution);
        self.lua.set_app_data(super::diagnostics::ActiveSnippet(
            self.previous_origin.take(),
        ));
    }
}

#[cfg(test)]
mod tests;
