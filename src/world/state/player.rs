use std::collections::HashMap;

use shared::{PlayerData, PlayerId, Position};

pub struct PlayerState {
    pub players: HashMap<PlayerId, PlayerData>,
}
impl PlayerState {
    pub fn get_player(&self, id: PlayerId) -> Option<&PlayerData> {
        self.players.get(&id)
    }

    pub fn set_player_position(&mut self, id: PlayerId, position: Position) {
        self.players.entry(id).and_modify(|p| {
            p.position = position;
        });
    }
}
