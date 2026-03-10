use std::time::Duration;

use mlua::{UserData, UserDataMethods};

pub(crate) mod operator;
pub(crate) mod sync;

#[derive(Debug, Clone, Default)]
pub(crate) struct CoreRxPipeline {
    pub limit: Option<u32>,
    pub throttle: Option<Duration>,
}

pub(crate) trait HasCoreRxPipeline: Clone + 'static {
    fn core_pipeline_mut(&mut self) -> &mut CoreRxPipeline;
}

pub(crate) fn add_core_rx_methods<T, M>(methods: &mut M)
where
    T: UserData + HasCoreRxPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("take", |_, this, n: u32| {
        let mut next = this.clone();
        next.core_pipeline_mut().limit = Some(n);
        Ok(next)
    });

    methods.add_method("throttle", |_, this, secs: Option<f64>| {
        let mut next = this.clone();
        next.core_pipeline_mut().throttle = secs.map(Duration::from_secs_f64);
        Ok(next)
    });
}
