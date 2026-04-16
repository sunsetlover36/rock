use std::{cmp::Reverse, collections::BinaryHeap};

use shared::PlayerKey;

pub struct Slot {
    pub generation: u32,
    pub occupied: bool,
}

pub struct PlayerPool {
    slots: Vec<Slot>,
    free: BinaryHeap<Reverse<u32>>,
}

impl Default for PlayerPool {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: BinaryHeap::new(),
        }
    }
}
impl PlayerPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim(&mut self) -> PlayerKey {
        if let Some(Reverse(i)) = self.free.pop() {
            let s = &mut self.slots[i as usize];
            s.occupied = true;

            PlayerKey {
                slot_idx: i,
                generation: s.generation,
            }
        } else {
            let i = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 0,
                occupied: true,
            });

            PlayerKey {
                slot_idx: i,
                generation: 0,
            }
        }
    }
    pub fn release(&mut self, pk: &PlayerKey) {
        let Some(s) = self.slots.get_mut(pk.slot_idx as usize) else {
            return;
        };

        if s.occupied && s.generation == pk.generation {
            s.occupied = false;
            s.generation += 1;
            self.free.push(Reverse(pk.slot_idx));
        }
    }
}
