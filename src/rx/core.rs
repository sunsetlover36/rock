use std::time::Duration;

use mlua::{UserData, UserDataMethods};
use thiserror::Error;

use crate::rx::{HasPipeline, operator::RxOp};

#[derive(Debug, Clone, Error)]
pub(crate) enum CoreSentryError {
    #[error("limit of {0} reached")]
    LimitReached(u32),
}

fn event_count(value: mlua::Value, operator: &str) -> mlua::Result<u32> {
    let count = match value {
        mlua::Value::Integer(n) => u32::try_from(n).ok(),
        mlua::Value::Number(n)
            if n.is_finite() && n >= 0.0 && n <= u32::MAX as f64 && n.fract() == 0.0 =>
        {
            Some(n as u32)
        }
        _ => None,
    };
    count.ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{operator}: expected an integer between 0 and {}",
            u32::MAX
        ))
    })
}

pub(crate) fn add_core_pipeline_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("take", |_, this, n: mlua::Value| {
        let mut next = this.clone();
        next.pipeline_mut()
            .add_operator(RxOp::Take(event_count(n, "take")?));
        Ok(next)
    });

    methods.add_method("skip", |_, this, n: mlua::Value| {
        let mut next = this.clone();
        next.pipeline_mut()
            .add_operator(RxOp::Skip(event_count(n, "skip")?));
        Ok(next)
    });

    methods.add_method("throttle", |_, this, secs: Option<f64>| {
        let mut next = this.clone();
        match secs {
            Some(secs) => {
                let duration = Duration::try_from_secs_f64(secs).map_err(|_| {
                    mlua::Error::runtime(
                        "throttle: expected finite, non-negative seconds within Duration range",
                    )
                })?;
                next.pipeline_mut().add_operator(RxOp::Throttle(duration));
            }
            // Preserve the existing builder API for disabling throttling.
            None => next.pipeline_mut().remove_throttles(),
        }
        Ok(next)
    });
}
