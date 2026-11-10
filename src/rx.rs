use std::time::{Duration, Instant};

use thiserror::Error;

pub(crate) mod core;
use core::CoreSentryError;

pub(crate) mod operator;
use operator::RxOp;

mod cursor;
pub(crate) use cursor::CursorRx;

pub(crate) mod sync;

// Builders describe an ordered chain; each consumer gets independent state.
#[derive(Debug, Clone, Default)]
pub(crate) struct RxPipeline {
    operators: Vec<RxOp>,
}
impl RxPipeline {
    pub fn add_operator(&mut self, op: RxOp) {
        self.operators.push(op);
    }

    fn remove_throttles(&mut self) {
        self.operators.retain(|op| !matches!(op, RxOp::Throttle(_)));
    }
}

pub(crate) trait HasPipeline: Clone + 'static {
    fn pipeline(&self) -> &RxPipeline;
    fn pipeline_mut(&mut self) -> &mut RxPipeline;
}

#[derive(Debug, Error)]
pub(crate) enum RxSentryError {
    #[error(transparent)]
    Core(CoreSentryError),

    #[error("failed to process a chain, Lua operator error: {0}")]
    Op(mlua::Error),
}

#[derive(Debug, Clone)]
enum OperatorState {
    Take {
        limit: u32,
        remaining: u32,
    },
    Skip {
        remaining: u32,
    },
    Throttle {
        duration: Duration,
        last_call_at: Option<Instant>,
    },
    Filter(mlua::Function),
    Map(mlua::Function),
    Changed {
        last_value: Option<mlua::MultiValue>,
    },
}
impl From<RxOp> for OperatorState {
    fn from(op: RxOp) -> Self {
        match op {
            RxOp::Take(limit) => Self::Take {
                limit,
                remaining: limit,
            },
            RxOp::Skip(remaining) => Self::Skip { remaining },
            RxOp::Throttle(duration) => Self::Throttle {
                duration,
                last_call_at: None,
            },
            RxOp::Filter(predicate) => Self::Filter(predicate),
            RxOp::Map(mapper) => Self::Map(mapper),
            RxOp::Changed => Self::Changed { last_value: None },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RxSentry {
    operators: Vec<OperatorState>,
}
impl RxSentry {
    pub fn new(pipeline: RxPipeline) -> Self {
        Self {
            operators: pipeline
                .operators
                .into_iter()
                .map(OperatorState::from)
                .collect(),
        }
    }

    fn exhausted_limit(&self) -> Option<u32> {
        self.operators.iter().find_map(|op| match op {
            OperatorState::Take {
                limit,
                remaining: 0,
            } => Some(*limit),
            _ => None,
        })
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted_limit().is_some()
    }

    pub fn process(
        &mut self,
        args: mlua::MultiValue,
    ) -> Result<Option<mlua::MultiValue>, RxSentryError> {
        self.process_at(args, Instant::now())
    }

    pub fn process_while(
        &mut self,
        args: mlua::MultiValue,
        active: impl FnMut() -> bool,
    ) -> Result<Option<mlua::MultiValue>, RxSentryError> {
        self.process_at_while(args, Instant::now(), active)
    }

    fn process_at(
        &mut self,
        args: mlua::MultiValue,
        now: Instant,
    ) -> Result<Option<mlua::MultiValue>, RxSentryError> {
        self.process_at_while(args, now, || true)
    }

    fn process_at_while(
        &mut self,
        mut args: mlua::MultiValue,
        now: Instant,
        mut active: impl FnMut() -> bool,
    ) -> Result<Option<mlua::MultiValue>, RxSentryError> {
        if let Some(limit) = self.exhausted_limit() {
            return Err(RxSentryError::Core(CoreSentryError::LimitReached(limit)));
        }

        for op in &mut self.operators {
            if !active() {
                return Ok(None);
            }
            match op {
                OperatorState::Take { remaining, .. } => *remaining -= 1,
                OperatorState::Skip { remaining } => {
                    if *remaining > 0 {
                        *remaining -= 1;
                        return Ok(None);
                    }
                }
                OperatorState::Throttle {
                    duration,
                    last_call_at,
                } => {
                    if last_call_at.is_some_and(|last| now.duration_since(last) < *duration) {
                        return Ok(None);
                    }
                    *last_call_at = Some(now);
                }
                OperatorState::Filter(predicate) => {
                    if !predicate
                        .call::<bool>(args.clone())
                        .map_err(RxSentryError::Op)?
                    {
                        return Ok(None);
                    }
                }
                OperatorState::Map(mapper) => {
                    args = mapper.call(args).map_err(RxSentryError::Op)?;
                }
                OperatorState::Changed { last_value } => {
                    // Lua tables and userdata retain identity-based comparison.
                    if last_value
                        .as_ref()
                        .is_some_and(|last| last.iter().eq(args.iter()))
                    {
                        return Ok(None);
                    }
                    *last_value = Some(args.clone());
                }
            }
        }

        Ok(active().then_some(args))
    }
}

#[cfg(test)]
mod tests;
