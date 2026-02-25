use color_eyre::eyre;

use crate::{runtime::utils::LuaResultExt, rx::RxPipeline};

pub(crate) mod event;
pub(crate) use event::*;

pub(crate) struct GameModeListenerParams {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub handle: mlua::Function,
    pub pipeline: RxPipeline,
}
pub struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub handle: mlua::Function,
    created_at_seq: u64,
    call_count: u32,
    pipeline: RxPipeline,
}
impl GameModeListener {
    pub fn new(params: GameModeListenerParams) -> Self {
        Self {
            name: params.name,
            scope: params.scope,
            handle: params.handle,
            created_at_seq: params.created_at_seq,
            call_count: 0,
            pipeline: params.pipeline,
        }
    }

    pub fn limit_reached(&self) -> bool {
        match self.pipeline.limit {
            Some(limit) => limit == self.call_count,
            None => false,
        }
    }

    pub fn can_process(&self, seq: u64) -> bool {
        self.created_at_seq < seq && !self.limit_reached()
    }

    pub fn increment_call_count(&mut self) {
        self.call_count += 1;
    }

    pub fn process_pipeline(
        &self,
        args: mlua::MultiValue,
    ) -> eyre::Result<Option<mlua::MultiValue>> {
        self.pipeline.process(args).wrap_err(&format!(
            "Failed to process a chain for the event listener (name: {:?})",
            self.name
        ))
    }
}
