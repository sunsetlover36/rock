use cron::Schedule;
use mlua::{LuaSerdeExt, UserData};
use std::str::FromStr;

use super::handle::TimerHandle;
use crate::runtime::{
    app_data,
    timer_manager::{TimerData, TimerKind},
    utils::get_app_data,
};

#[derive(Clone, Default)]
pub(super) struct TimerRx {
    kind: Option<TimerKind>,
    data: Option<mlua::Value>,
    schedule: Option<Schedule>,
    seconds: Option<u64>,
}
impl UserData for TimerRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("timeout", |_, this, seconds: u64| {
            let mut next = this.clone();
            next.kind = Some(TimerKind::Timeout);
            next.seconds = Some(seconds);
            Ok(next)
        });

        methods.add_method("interval", |_, this, seconds: u64| {
            let mut next = this.clone();
            next.kind = Some(TimerKind::Interval);
            next.seconds = Some(seconds);
            Ok(next)
        });

        methods.add_method("cron", |_, this, schedule: String| {
            let cron_schedule = Schedule::from_str(&schedule).map_err(|e| {
                mlua::Error::runtime(format!(
                    "timer.cron: failed to parse a cron schedule ({})",
                    e.to_string()
                ))
            })?;

            let mut next = this.clone();
            next.kind = Some(TimerKind::Cron);
            next.schedule = Some(cron_schedule);
            Ok(next)
        });

        methods.add_method("data", |_, this, data: Option<mlua::Value>| {
            let mut next = this.clone();
            next.data = data;
            Ok(next)
        });

        methods.add_method("register", |lua, this, id: String| {
            let timer_manager_data = get_app_data::<app_data::TimerManager>(lua)?;
            let timer_manager = &timer_manager_data.0;

            let cloned_id = id.clone();
            let seconds = this.seconds.unwrap_or(0);
            let data = this
                .data
                .as_ref()
                .and_then(|data| lua.from_value(data.clone()).ok());

            let result = match this.kind {
                Some(kind) => match kind {
                    TimerKind::Timeout => timer_manager.create_timer(TimerData::Timeout {
                        id: cloned_id,
                        seconds,
                        data,
                    }),
                    TimerKind::Interval => timer_manager.create_timer(TimerData::Interval {
                        id: cloned_id,
                        seconds,
                        data,
                    }),
                    TimerKind::Cron => {
                        let schedule = this
                            .schedule
                            .as_ref()
                            .ok_or_else(|| {
                                mlua::Error::runtime(
                                    "Cannot register a cron timer without a cron schedule",
                                )
                            })?
                            .clone();
                        timer_manager.create_timer(TimerData::Cron {
                            id: cloned_id,
                            schedule,
                            data,
                        })
                    }
                },
                None => {
                    return Err(mlua::Error::runtime(
                        "Cannot register a timer without specifying a timer kind",
                    ));
                }
            };
            result.map_err(|e| mlua::Error::runtime(e.to_string()))?;

            Ok(TimerHandle { id })
        });
    }
}
