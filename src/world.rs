use std::rc::Rc;

use shared::Position;

pub mod protocol;
pub mod state;
pub use state::*;

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
