use std::{cell::RefCell, rc::Rc, time::Duration};

use color_eyre::eyre;
use cron::Schedule;
use smallvec::smallvec;
use strum::{AsRefStr, Display, EnumDiscriminants, EnumString};
use tokio::{
    runtime::Handle,
    time::{interval, sleep},
};

use crate::runtime::{
    EventBus,
    plugins::on::protocol::{EventScope, GameModeEvent, GameModeEventData, TimerEventData},
};

#[derive(Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(name(TimerKind))]
#[strum_discriminants(derive(EnumString, AsRefStr, Display))]
pub enum TimerData {
    Timeout {
        id: String,
        seconds: u64,
        data: Option<serde_json::Value>,
    },
    Interval {
        id: String,
        seconds: u64,
        data: Option<serde_json::Value>,
    },
    Cron {
        id: String,
        schedule: Schedule,
        data: Option<serde_json::Value>,
    },
}

slotmap::new_key_type! { pub(crate) struct TimerKey; }

#[derive(Debug, Clone)]
struct TimerEvent {
    key: TimerKey,
    data: Option<serde_json::Value>,
}

pub(crate) struct TimerManagerParams {
    pub tokio_handle: Handle,
    pub event_bus: Rc<EventBus>,
}
struct TimerEntry {
    timer: TimerData,
    work: super::lifetime::OwnedWork,
    started: bool,
}
pub(crate) struct TimerManager {
    timers: RefCell<slotmap::SlotMap<TimerKey, TimerEntry>>,
    tx: flume::Sender<TimerEvent>,
    rx: flume::Receiver<TimerEvent>,
    tokio_handle: Handle,
    event_bus: Rc<EventBus>,
}
impl TimerData {
    fn id(&self) -> &str {
        match self {
            Self::Timeout { id, .. } | Self::Interval { id, .. } | Self::Cron { id, .. } => id,
        }
    }
}
impl TimerManager {
    pub fn new(params: TimerManagerParams) -> Self {
        let (tx, rx) = flume::unbounded();
        Self {
            timers: RefCell::new(slotmap::SlotMap::with_key()),
            tx,
            rx,
            tokio_handle: params.tokio_handle,
            event_bus: params.event_bus,
        }
    }
    pub fn create_timer(
        &self,
        timer: TimerData,
        work: super::lifetime::OwnedWork,
    ) -> eyre::Result<()> {
        if matches!(&timer, TimerData::Interval { seconds: 0, .. }) {
            return Err(eyre::eyre!("Timer interval must be greater than zero"));
        }
        if let TimerData::Timeout { seconds, .. } | TimerData::Interval { seconds, .. } = &timer {
            if std::time::Instant::now()
                .checked_add(Duration::from_secs(*seconds))
                .is_none()
            {
                return Err(eyre::eyre!("Timer duration is too large"));
            }
        }
        let mut timers = self.timers.borrow_mut();
        if !matches!(timer, TimerData::Timeout { .. })
            && timers
                .values()
                .any(|t| t.work.cancel.get() && t.timer.id() == timer.id())
        {
            return Err(eyre::eyre!(
                "Cannot create a timer with name {}: timer already exists",
                timer.id()
            ));
        }
        timers.insert(TimerEntry {
            timer,
            work,
            started: false,
        });
        Ok(())
    }
    pub fn cancel_timer(&self, id: String) {
        for timer in self.timers.borrow().values().filter(|t| t.timer.id() == id) {
            timer.work.cancel.cancel();
        }
    }
    pub fn tick(&self, lua: &mlua::Lua) {
        {
            let mut timers = self.timers.borrow_mut();
            timers.retain(|_, timer| {
                if timer.work.is_alive(lua).unwrap_or(false) {
                    true
                } else {
                    timer.work.cancel.cancel();
                    false
                }
            });
            for (key, entry) in timers.iter_mut() {
                if entry.started {
                    continue;
                }
                entry.started = true;
                let tx = self.tx.clone();
                let timer = entry.timer.clone();
                let handle = self.tokio_handle.spawn(async move {
                    match timer {
                        TimerData::Timeout { seconds, data, .. } => {
                            sleep(Duration::from_secs(seconds)).await;
                            let _ = tx.send(TimerEvent { key, data });
                        }
                        TimerData::Interval { seconds, data, .. } => {
                            let mut ticker = interval(Duration::from_secs(seconds));
                            ticker.tick().await;
                            loop {
                                ticker.tick().await;
                                if tx
                                    .send(TimerEvent {
                                        key,
                                        data: data.clone(),
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        TimerData::Cron { schedule, data, .. } => {
                            for next in schedule.upcoming(chrono::Utc) {
                                if let Ok(duration) =
                                    next.signed_duration_since(chrono::Utc::now()).to_std()
                                {
                                    sleep(duration).await;
                                }
                                if tx
                                    .send(TimerEvent {
                                        key,
                                        data: data.clone(),
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                });
                entry.work.cancel.set_abort(handle.abort_handle());
            }
        }
        for event in self.rx.drain() {
            let timers = self.timers.borrow();
            let Some(timer) = timers.get(event.key) else {
                continue;
            };
            if !timer.work.is_alive(lua).unwrap_or(false) {
                continue;
            }
            self.event_bus.schedule_owned_event(
                GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Timer(TimerEventData::Fire {
                        id: timer.timer.id().into(),
                        data: event.data,
                    }),
                },
                timer.work.clone(),
                matches!(timer.timer, TimerData::Timeout { .. }),
            );
        }
    }
}
impl Drop for TimerManager {
    fn drop(&mut self) {
        for entry in self.timers.get_mut().values() {
            entry.work.cancel.cancel();
        }
    }
}

#[cfg(test)]
mod tests;
