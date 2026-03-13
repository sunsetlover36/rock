use color_eyre::eyre;

use crate::{
    runtime::{app_data::ExecutionContext, utils::LuaResultExt},
    rx::{
        core::{CorePipeline, CoreSentry, CoreSentryError},
        operator::OpPipeline,
    },
};

pub(crate) mod event;
pub(crate) use event::*;

pub(crate) enum ListenerCallError {
    LimitReached(u32),
    OpError(eyre::Report),
}

pub(crate) struct GameModeListenerParams {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub core_pipeline: CorePipeline,
    pub op_pipeline: OpPipeline,
    pub priority: u32,
}

pub(crate) struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub priority: u32,
    created_at_seq: u64,
    core_sentry: CoreSentry,
    op_pipeline: OpPipeline,
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
            core_sentry: params.core_sentry,
            op_pipeline: params.op_pipeline,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.core_sentry.is_exhausted()
    }

    pub fn call(
        &self,
        seq: u64,
        args: mlua::MultiValue,
    ) -> Result<Option<mlua::MultiValue>, ListenerCallError> {
        if self.created_at_seq >= seq {
            return Ok(None);
        }

        if let Err(err) = self.core_sentry.process() {
            match err {
                CoreSentryError::LimitReached(limit) => {
                    return Err(ListenerCallError::LimitReached(limit));
                }
                CoreSentryError::Skipping | CoreSentryError::Throttled => {
                    return Ok(None);
                }
            }
        }

        match self.op_pipeline.process(args).wrap_err(&format!(
            "Failed to process a chain for the event listener (name: {:?})",
            self.name
        )) {
            Ok(args) => Ok(args),
            Err(err) => Err(ListenerCallError::OpError(err)),
        }
    }

    pub fn get_seq(&self) -> u64 {
        self.created_at_seq
    }
}
