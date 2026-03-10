use mlua::UserData;
use shared::PlayerKey;

use crate::{
    runtime::network_replicator::protocol::{PolicyId, ReplicationPolicy, ReplicationTarget},
    rx::{
        CoreRxPipeline, HasCoreRxPipeline, add_core_rx_methods,
        sync::{
            HasPolicy, ToPolicyHandle, add_sync_consumer_methods,
            routing::add_routing_rx_sync_methods,
            spatial::{add_area_rx_sync_methods, add_radius_sync_methods},
        },
    },
};

mod handle;
use handle::SyncRxHandle;

#[derive(Clone)]
pub(in crate::runtime::plugins::player) struct SyncRx {
    policy: ReplicationPolicy,
    core_pipeline: CoreRxPipeline,
}
impl SyncRx {
    pub fn new(pk: PlayerKey) -> Self {
        Self {
            policy: ReplicationPolicy::new(ReplicationTarget::Player(pk)),
            core_pipeline: CoreRxPipeline::default(),
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
impl HasCoreRxPipeline for SyncRx {
    fn core_pipeline_mut(&mut self) -> &mut CoreRxPipeline {
        &mut self.core_pipeline
    }
}

impl UserData for SyncRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_core_rx_methods(methods);

        add_routing_rx_sync_methods(methods);

        add_area_rx_sync_methods(methods);
        add_radius_sync_methods(methods);

        add_sync_consumer_methods(methods);
    }
}
