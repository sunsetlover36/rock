use std::time::{Duration, Instant};

use mlua::{UserData, UserDataMethods};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub(crate) enum CoreSentryError {
    #[error("limit of {0} reached")]
    LimitReached(u32),

    #[error("skipped by an order rule")]
    Skipping,

    #[error("throttled")]
    Throttled,
}

enum OrderRule {
    Take(u32),
    Skip(u32),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CorePipeline {
    pub order: Vec<OrderRule>,
    pub throttle: Option<Duration>,
}

pub(crate) trait HasCorePipeline: Clone + 'static {
    fn core_pipeline_mut(&mut self) -> &mut CorePipeline;
}

pub(crate) fn add_core_pipeline_methods<T, M>(methods: &mut M)
where
    T: UserData + HasCorePipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("take", |_, this, n: u32| {
        let mut next = this.clone();
        next.core_pipeline_mut().order.push(OrderRule::Take(n));
        Ok(next)
    });

    methods.add_method("skip", |_, this, n: u32| {
        let mut next = this.clone();
        next.core_pipeline_mut().order.push(OrderRule::Skip(n));
        Ok(next)
    });

    methods.add_method("throttle", |_, this, secs: Option<f64>| {
        let mut next = this.clone();
        next.core_pipeline_mut().throttle = secs.map(Duration::from_secs_f64);
        Ok(next)
    });
}

enum CycleBehavior {
    FiniteTake(u32),
    InfiniteSkip,
    Repeat,
}

#[derive(Debug, Default)]
struct OrderState {
    current_rule_idx: usize,
    call_diff: i32,
    cycle_behavior: CycleBehavior,
}
pub(crate) struct CoreSentry {
    pipeline: CorePipeline,
    order_state: OrderState,
    last_call_at: Option<Instant>,
}
impl CoreSentry {
    pub fn new(pipeline: CorePipeline) -> Self {
        // By default, order pipeline is repeated
        // For example, :take(2):skip(1) -> take 2, then skip 1 and repeat
        let mut order_state = OrderState {
            current_rule_idx: 0,
            call_diff: 0,
            cycle_behavior: CycleBehavior::Repeat,
        };

        let mut takes = 0;
        let mut skips = 0;
        for rule in pipeline.order {
            match rule {
                OrderRule::Take(n) => {
                    takes += n;
                }
                OrderRule::Skip(n) => {
                    skips += n;
                }
            }
        }

        let takes_only = takes > 0 && skips == 0;
        let skips_only = takes == 0 && skips > 0;

        // Finite cycle (take N, then delete immediately)
        if takes_only {
            order_state.cycle_behavior = CycleBehavior::FiniteTake(takes);
        }

        // Infinite cycle (skip N, then emit forever)
        if skips_only {
            order_state.cycle_behavior = CycleBehavior::InfiniteSkip;
        }

        Self {
            pipeline,
            order_state,
            last_call_at: None,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        match self.order_state.cycle_behavior {
            CycleBehavior::FiniteTake(_) => {
                self.order_state.current_rule_idx >= self.pipeline.order.len()
                    && self.order_state.call_diff == 0
            }
            _ => false,
        }
    }

    pub fn process(&mut self) -> Result<(), CoreSentryError> {
        let now = Instant::now();

        if let Some(throttle) = self.pipeline.throttle {
            if let Some(last_call_at) = self.last_call_at {
                if now.duration_since(last_call_at) < throttle {
                    return Err(CoreSentryError::Throttled);
                }
            }
        }

        loop {
            // We skip
            if self.order_state.call_diff < 0 {
                self.order_state.call_diff += 1;
                return Err(CoreSentryError::Skipping);
            }

            // We take
            if self.order_state.call_diff > 0 {
                self.order_state.call_diff -= 1;
                self.last_call_at = Some(now);
                return Ok(());
            }

            // Call diff is zero, need to process a next rule or get a decision
            if let Some(rule) = self.pipeline.order.get(self.order_state.current_rule_idx) {
                // Immediately increment the rule index, since we're already processed the current one
                self.order_state.current_rule_idx += 1;

                // Immediate rule execution (n - 1)
                match rule {
                    OrderRule::Take(n) => {
                        // Next rule commands to take N times
                        self.order_state.call_diff += n - 1;
                        self.last_call_at = Some(now);
                        return Ok(());
                    }
                    OrderRule::Skip(n) => {
                        // Next rule commands to skip N times
                        self.order_state.call_diff -= n - 1;
                        return Err(CoreSentryError::Skipping);
                    }
                }
            }

            // There are no rules left, need a decision
            match self.order_state.cycle_behavior {
                CycleBehavior::FiniteTake(total) => {
                    return Err(CoreSentryError::LimitReached(total));
                }
                CycleBehavior::InfiniteSkip => {
                    return Ok(());
                }
                CycleBehavior::Repeat => {
                    self.order_state.current_rule_idx = 0;
                    continue;
                }
            }
        }
    }
}
