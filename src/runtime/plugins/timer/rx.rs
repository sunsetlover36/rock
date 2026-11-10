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
    players: Vec<rock_wire::PlayerKey>,
}
impl UserData for TimerRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "while_connected",
            |_, this, p: mlua::UserDataRef<crate::runtime::plugins::player::PlayerHandle>| {
                let mut next = this.clone();
                if !next.players.contains(&p.key()) {
                    next.players.push(p.key());
                }
                Ok(next)
            },
        );
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
            let timer_manager = get_app_data::<app_data::TimerManager>(lua)?.0.clone();

            let cloned_id = id.clone();
            let seconds = this.seconds.unwrap_or(0);
            let data = this
                .data
                .as_ref()
                .and_then(|data| lua.from_value(data.clone()).ok());

            let timer = match this.kind {
                Some(kind) => match kind {
                    TimerKind::Timeout => TimerData::Timeout {
                        id: cloned_id,
                        seconds,
                        data,
                    },
                    TimerKind::Interval => TimerData::Interval {
                        id: cloned_id,
                        seconds,
                        data,
                    },
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
                        TimerData::Cron {
                            id: cloned_id,
                            schedule,
                            data,
                        }
                    }
                },
                None => {
                    return Err(mlua::Error::runtime(
                        "Cannot register a timer without specifying a timer kind",
                    ));
                }
            };
            let context = crate::runtime::lifetime::WorkContext::capture(lua, &this.players)?;
            let work = crate::runtime::lifetime::OwnedWork::register(lua, context)?;
            let cancel = work.cancel.clone();
            timer_manager.create_timer(timer, work).map_err(|e| {
                cancel.cancel();
                mlua::Error::runtime(e.to_string())
            })?;

            Ok(TimerHandle { cancel })
        });
    }
}
