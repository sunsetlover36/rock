use std::sync::Arc;

use arc_swap::ArcSwap;
use shared::{MovementDirection, Position};
use tokio::sync::mpsc;

use crate::{actor::Actor, router::CommitRouter};
use state::WorldState;

mod state;

pub enum GameIntent {
    MovePlayer(MovementDirection),
}
pub enum WorldCommit {
    PlayerMoved { fid: u32, x: i32, y: i32 },
    BiomeExplored,
}

pub struct WorldGetters {
    state: Arc<ArcSwap<WorldState>>,
}
impl WorldGetters {
    pub fn get_player_pos(&self, id: &u64) -> Option<Position> {
        let state_guard = self.state.load();
        if let Some(player) = state_guard.player_state.players.get(id) {
            return Some(player.position.clone());
        }

        None
    }
}

pub struct World {
    game_intent_rx: mpsc::Receiver<GameIntent>,
    commit_router: CommitRouter,
    state: Arc<ArcSwap<WorldState>>,
}

#[async_trait::async_trait]
impl Actor for World {
    async fn run(mut self: Box<Self>) {
        while let Some(intent) = self.game_intent_rx.recv().await {
            match intent {
                GameIntent::MovePlayer(direction) => {
                    println!("[world] game intent: move direction = {:?}", direction);

                    // TODO: change state
                    // self.state.store()

                    self.commit_router
                        .emit(WorldCommit::PlayerMoved { fid: 0, x: 0, y: 0 });
                }
            }
        }
    }
}

pub fn create_world_actor(
    buffer: usize,
    commit_router: CommitRouter,
) -> (mpsc::Sender<GameIntent>, World, WorldGetters) {
    let (game_intent_tx, game_intent_rx) = mpsc::channel(buffer);

    let state = Arc::new(ArcSwap::from_pointee(WorldState::new()));
    let world_actor = World {
        game_intent_rx,
        commit_router,
        state: Arc::clone(&state),
    };

    let world_getters = WorldGetters {
        state: Arc::clone(&state),
    };

    return (game_intent_tx, world_actor, world_getters);
}
