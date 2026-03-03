use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use color_eyre::eyre;
use cron::Schedule;
use smallvec::smallvec;
use strum::{AsRefStr, Display, EnumDiscriminants, EnumString};
use tokio::{
    runtime::Handle,
    task::AbortHandle,
    time::{interval, sleep},
};

use crate::runtime::{
    EventBus,
    api::on::{EventScope, GameModeEvent, GameModeEventData, TimerEventData},
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

#[derive(Debug, Clone)]
pub struct TimerEvent {
    id: String,
    data: Option<serde_json::Value>,
}

pub(crate) struct TimerManagerParams {
    pub tokio_handle: Handle,
    pub event_bus: Rc<EventBus>,
}

struct TimerManagerInner {
    timers: Vec<TimerData>,
    handles: HashMap<String, Vec<AbortHandle>>,
}
pub(crate) struct TimerManager {
    inner: RefCell<TimerManagerInner>,
    tx: flume::Sender<TimerEvent>,
    rx: flume::Receiver<TimerEvent>,
    tokio_handle: Handle,
    event_bus: Rc<EventBus>,
}
impl TimerManager {
    pub fn new(params: TimerManagerParams) -> Self {
        let (tx, rx) = flume::unbounded::<TimerEvent>();

        let manager = Self {
            inner: RefCell::new(TimerManagerInner {
                timers: Vec::new(),
                handles: HashMap::new(),
            }),
            tx,
            rx,
            tokio_handle: params.tokio_handle,
            event_bus: params.event_bus,
        };

        manager
    }

    fn contains_timer(&self, id: &str) -> eyre::Result<()> {
        let handles = &self.inner.borrow().handles;
        if handles.contains_key(id) {
            return Err(eyre::eyre!(
                "Cannot create a timer with name {}: timer already exists",
                id
            ));
        }

        Ok(())
    }
    pub fn create_timer(&self, timer: TimerData) -> eyre::Result<()> {
        {
            match &timer {
                TimerData::Interval { id, .. } => {
                    self.contains_timer(id)?;
                }
                TimerData::Cron { id, .. } => {
                    self.contains_timer(id)?;
                }
                _ => {}
            }
        }

        self.inner.borrow_mut().timers.push(timer);
        Ok(())
    }

    pub fn cancel_timer(&self, id: String) {
        if let Some(abort_handles) = self.inner.borrow_mut().handles.remove(&id) {
            for h in abort_handles {
                h.abort();
            }
        }
    }

    pub fn tick(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            let timers = &mut inner.timers;

            if !timers.is_empty() {
                let timers = std::mem::take(timers);
                for timer in timers {
                    let tx = self.tx.clone();
                    match timer {
                        TimerData::Timeout { id, seconds, data } => {
                            let cloned_id = id.clone();
                            let handle = self.tokio_handle.spawn(async move {
                                sleep(Duration::from_secs(seconds)).await;
                                let _ = tx.send(TimerEvent {
                                    id: cloned_id,
                                    data,
                                });
                            });

                            inner
                                .handles
                                .entry(id)
                                .or_default()
                                .push(handle.abort_handle());
                        }
                        TimerData::Interval { id, seconds, data } => {
                            let cloned_id = id.clone();
                            let handle = self.tokio_handle.spawn(async move {
                                let mut ticker = interval(Duration::from_secs(seconds));
                                ticker.tick().await;

                                loop {
                                    ticker.tick().await;
                                    if tx
                                        .send(TimerEvent {
                                            id: cloned_id.clone(),
                                            data: data.clone(),
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            });

                            inner
                                .handles
                                .entry(id)
                                .or_default()
                                .push(handle.abort_handle());
                        }
                        TimerData::Cron { id, schedule, data } => {
                            let cloned_id = id.clone();
                            let handle = self.tokio_handle.spawn(async move {
                                let mut upcoming = schedule.upcoming(chrono::Utc);
                                while let Some(next) = upcoming.next() {
                                    let now = chrono::Utc::now();
                                    let duration = next.signed_duration_since(now);

                                    if let Ok(duration) = duration.to_std() {
                                        sleep(duration).await;
                                    }

                                    if tx
                                        .send(TimerEvent {
                                            id: cloned_id.clone(),
                                            data: data.clone(),
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            });

                            inner
                                .handles
                                .entry(id)
                                .or_default()
                                .push(handle.abort_handle());
                        }
                    }
                }
            }
        }

        for event in self.rx.drain() {
            self.event_bus.schedule_event(GameModeEvent {
                scopes: smallvec![EventScope::Global],
                data: GameModeEventData::Timer(TimerEventData::Fire {
                    id: event.id,
                    data: event.data,
                }),
            });
        }
    }
}
