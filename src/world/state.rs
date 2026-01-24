pub mod player;
use std::{cell::RefCell, collections::HashMap};

use player::PlayerState;

pub struct WorldState {
    pub tick: RefCell<u64>,
    pub player_state: RefCell<PlayerState>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            tick: RefCell::new(0),
            player_state: RefCell::new(PlayerState {
                players: HashMap::new(),
            }),
        }
    }
}
