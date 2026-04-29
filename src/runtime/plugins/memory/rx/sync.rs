use mlua::UserData;

use crate::{
    runtime::network_replicator::protocol::{PolicyId, ReplicationPolicy, ReplicationTarget},
    rx::{
        HasPipeline, RxPipeline,
        core::add_core_pipeline_methods,
        operator::add_op_pipeline_methods,
        sync::{
            HasPolicy, ToPolicyHandle, add_sync_consumer_methods,
            routing::add_routing_rx_sync_methods, spatial::add_area_rx_sync_methods,
        },
    },
};

mod handle;
use handle::SyncRxHandle;

#[derive(Clone)]
pub(in crate::runtime::plugins::memory) struct SyncRx {
    policy: ReplicationPolicy,
    pipeline: RxPipeline,
}
impl SyncRx {
    pub fn new(key: String) -> Self {
        Self {
            policy: ReplicationPolicy::new(ReplicationTarget::MemoryNode(key)),
            pipeline: RxPipeline::default(),
        }
    }
}

impl HasPolicy for SyncRx {
    fn policy(&self) -> &ReplicationPolicy {
        &self.policy
    }
    fn policy_mut(&mut self) -> &mut ReplicationPolicy {
        &mut self.policy
    }
}
impl ToPolicyHandle for SyncRx {
    type Handle = SyncRxHandle;
    fn to_policy_handle(&self, id: PolicyId) -> Self::Handle {
        SyncRxHandle::new(id)
    }
}
impl HasPipeline for SyncRx {
    fn pipeline(&self) -> &RxPipeline {
        &self.pipeline
    }
    fn pipeline_mut(&mut self) -> &mut RxPipeline {
        &mut self.pipeline
    }
}

impl UserData for SyncRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_core_pipeline_methods(methods);
        add_op_pipeline_methods(methods);

        add_routing_rx_sync_methods(methods);

        add_area_rx_sync_methods(methods);

        add_sync_consumer_methods(methods);
    }
}
