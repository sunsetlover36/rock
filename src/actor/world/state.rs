pub mod player;
use std::collections::HashMap;

use player::PlayerState;

pub struct WorldState {
    pub tick: u64,
    pub player_state: PlayerState,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            tick: 0,
            player_state: PlayerState {
                players: HashMap::new(),
            },
        }
    }
}
