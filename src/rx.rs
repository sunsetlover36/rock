use thiserror::Error;

pub(crate) mod core;
pub(crate) use core::CorePipeline;
use core::{CoreSentry, CoreSentryError};

pub(crate) mod operator;
pub(crate) use operator::OpPipeline;
use operator::{OpSentry, RxOp};

pub(crate) mod sync;

// -- Pipeline -> stateless blueprint
#[derive(Clone, Default)]
pub(crate) struct RxPipeline {
    core: CorePipeline,
    op: OpPipeline,
}
impl RxPipeline {
    pub fn add_operator(&mut self, op: RxOp) {
        self.op.operators.push(op);
    }
}

pub(crate) trait HasPipeline: Clone + 'static {
    fn pipeline(&self) -> &RxPipeline;
    fn pipeline_mut(&mut self) -> &mut RxPipeline;
}

// -- Sentry -> stateful instance based on consumed pipeline
#[derive(Debug, Error)]
pub(crate) enum RxSentryError {
    #[error(transparent)]
    Core(CoreSentryError),

    #[error("failed to process a chain, Lua operator error: {0}")]
    Op(mlua::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct RxSentry {
    core: CoreSentry,
    op: OpSentry,
}
impl RxSentry {
    pub fn new(pipeline: RxPipeline) -> Self {
        Self {
            core: CoreSentry::new(pipeline.core),
            op: OpSentry::new(pipeline.op),
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.core.is_exhausted()
    }

    pub fn process(
        &mut self,
        args: mlua::MultiValue,
    ) -> Result<Option<mlua::MultiValue>, RxSentryError> {
        if let Err(err) = self.core.process() {
            match err {
                CoreSentryError::LimitReached(limit) => {
                    return Err(RxSentryError::Core(CoreSentryError::LimitReached(limit)));
                }
                CoreSentryError::Skipping | CoreSentryError::Throttled => {
                    return Ok(None);
                }
            }
        }

        self.op.process(args).map_err(RxSentryError::Op)
    }
}
