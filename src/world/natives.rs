use std::rc::Rc;

use shared::Position;

use crate::world::WorldState;

pub struct WorldNatives {
    pub state: Rc<WorldState>,
}
impl WorldNatives {
    pub fn get_player_pos(&self, id: &u64) -> Option<Position> {
        let player_state = self.state.player_state.borrow();
        if let Some(player) = player_state.players.get(id) {
            return Some(player.position.clone());
        }

        None
    }
}
