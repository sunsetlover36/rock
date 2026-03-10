use color_eyre::eyre;

use crate::{
    runtime::{app_data::ExecutionContext, utils::LuaResultExt},
    rx::{CoreRxPipeline, operator::OpRxPipeline},
};

pub(crate) mod event;
pub(crate) use event::*;

pub(crate) struct GameModeListenerParams {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub core_pipeline: CoreRxPipeline,
    pub op_pipeline: OpRxPipeline,
    pub priority: u32,
}
pub(crate) struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub priority: u32,
    created_at_seq: u64,
    call_count: u32,
    core_pipeline: CoreRxPipeline,
    op_pipeline: OpRxPipeline,
}
impl GameModeListener {
    pub fn new(params: GameModeListenerParams) -> Self {
        Self {
            name: params.name,
            scope: params.scope,
            context: params.context,
            handle: params.handle,
            priority: params.priority,
            created_at_seq: params.created_at_seq,
            call_count: 0,
            core_pipeline: params.core_pipeline,
            op_pipeline: params.op_pipeline,
        }
    }

    pub fn limit_reached(&self) -> bool {
        match self.core_pipeline.limit {
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
        self.op_pipeline.process(args).wrap_err(&format!(
            "Failed to process a chain for the event listener (name: {:?})",
            self.name
        ))
    }

    pub fn get_seq(&self) -> u64 {
        self.created_at_seq
    }
}
