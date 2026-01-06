use std::collections::HashMap;

use shared::{PlayerData, Position};

pub struct PlayerState {
    pub players: HashMap<u64, PlayerData>,
}
impl PlayerState {
    pub fn get_player(&self, id: &u64) -> Option<&PlayerData> {
        self.players.get(&id)
    }

    pub fn set_player_position(&mut self, id: u64, position: Position) {
        self.players.entry(id).and_modify(|p| {
            p.position = position;
        });
    }
}
