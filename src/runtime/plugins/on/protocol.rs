use std::{cell::RefCell, rc::Rc};

use color_eyre::eyre;

use crate::{
    runtime::{app_data::ExecutionContext, lifetime::Cancellation, plugins::layer::LayerId},
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
    pub owner: Option<LayerId>,
    pub active: Cancellation,
    pub players: Vec<rock_wire::PlayerKey>,
}

#[derive(Clone)]
pub(crate) struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub context: ExecutionContext,
    pub handle: mlua::Function,
    pub priority: u32,
    created_at_seq: u64,
    rx_sentry: Rc<RefCell<RxSentry>>,
    pub owner: Option<LayerId>,
    pub active: Cancellation,
    pub players: Vec<rock_wire::PlayerKey>,
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
            rx_sentry: Rc::new(RefCell::new(params.rx_sentry)),
            owner: params.owner,
            active: params.active,
            players: params.players,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.rx_sentry
            .try_borrow()
            .is_ok_and(|sentry| sentry.is_exhausted())
    }

    pub fn call(
        &self,
        seq: u64,
        args: mlua::MultiValue,
        mut active: impl FnMut() -> bool,
    ) -> eyre::Result<Option<mlua::MultiValue>> {
        if self.created_at_seq >= seq {
            return Ok(None);
        }

        let mut sentry = self.rx_sentry.try_borrow_mut().map_err(|_| {
            eyre::eyre!("Cannot re-enter an event chain while its operators are running")
        })?;
        match sentry.process_while(args, || self.active.get() && active()) {
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
