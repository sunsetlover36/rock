use color_eyre::eyre;

use crate::runtime::utils::LuaResultExt;

pub(crate) mod event;
pub(crate) use event::*;

pub struct GameModeListener {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub handle: mlua::Function,
    pub call_count: u32,
    pub limit: Option<u32>,
    pub predicates: Vec<mlua::Function>,
}
impl GameModeListener {
    pub fn limit_reached(&self) -> bool {
        match self.limit {
            Some(limit) => limit == self.call_count,
            None => false,
        }
    }
    pub fn passes_filters(&self, args: &mlua::MultiValue) -> eyre::Result<bool> {
        self.predicates.iter().try_fold(true, |_, predicate| {
            predicate
                .call::<bool>(args)
                .wrap_err("Error when filtering a chain for the event listener")
        })
    }
}
