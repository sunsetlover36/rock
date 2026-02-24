use color_eyre::eyre;

use crate::runtime::{api::on::rx::RxOperator, utils::LuaResultExt};

pub(crate) mod event;
pub(crate) use event::*;

pub struct GameModeListener {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub handle: mlua::Function,
    pub call_count: u32,
    pub limit: Option<u32>,
    pub operators: Vec<RxOperator>,
}
impl GameModeListener {
    pub fn limit_reached(&self) -> bool {
        match self.limit {
            Some(limit) => limit == self.call_count,
            None => false,
        }
    }
    pub fn process_chain(
        &self,
        mut args: mlua::MultiValue,
    ) -> eyre::Result<Option<mlua::MultiValue>> {
        for op in &self.operators {
            match op {
                RxOperator::Filter(predicate) => {
                    let result = predicate
                        .call::<bool>(&args)
                        .wrap_err("Error when filtering a chain for the event listener")?;

                    if !result {
                        return Ok(None);
                    }
                }
                RxOperator::Map(mapper) => {
                    let new_args = mapper
                        .call::<mlua::MultiValue>(args)
                        .wrap_err("Error when mapping a chain for the event listener")?;
                    args = new_args;
                }
            }
        }

        Ok(Some(args))
    }
}
