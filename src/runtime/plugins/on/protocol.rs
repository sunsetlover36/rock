use color_eyre::eyre;

use crate::{
    runtime::app_data::ExecutionContext,
    rx::{RxSentry, RxSentryError},
};

pub(crate) mod event;
pub(crate) use event::*;

pub(crate) struct GameModeListenerParams {
    pub name: Option<String>,
    pub created_at_seq: u64,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub priority: u32,
    pub rx_sentry: RxSentry,
}

pub(crate) struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub priority: u32,
    created_at_seq: u64,
    rx_sentry: RxSentry,
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
            rx_sentry: params.rx_sentry,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.rx_sentry.is_exhausted()
    }

    pub fn call(
        &mut self,
        seq: u64,
        args: mlua::MultiValue,
    ) -> eyre::Result<Option<mlua::MultiValue>> {
        if self.created_at_seq >= seq {
            return Ok(None);
        }

        match self.rx_sentry.process(args) {
            Err(RxSentryError::Core(_)) => Ok(None),
            res => res.map_err(|e| {
                eyre::eyre!(
                    "Failed to process a chain for the event listener (name: {:?}): {}",
                    self.name,
                    e
                )
            }),
        }
    }

    pub fn get_seq(&self) -> u64 {
        self.created_at_seq
    }
}
