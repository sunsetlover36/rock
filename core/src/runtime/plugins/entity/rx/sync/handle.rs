use mlua::UserData;

use crate::{
    runtime::network_replicator::protocol::PolicyId,
    rx::sync::{
        PolicyHandle, add_base_handle_methods,
        routing::add_routing_rx_handle_methods,
        spatial::{add_area_rx_handle_methods, add_radius_handle_methods},
    },
};

pub(in crate::runtime::plugins::entity) struct SyncRxHandle {
    id: PolicyId,
}
impl SyncRxHandle {
    pub fn new(id: PolicyId) -> Self {
        Self { id }
    }
}

impl PolicyHandle for SyncRxHandle {
    fn policy_id(&self) -> PolicyId {
        self.id
    }
}

impl UserData for SyncRxHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        add_routing_rx_handle_methods(methods);

        add_area_rx_handle_methods(methods);
        add_radius_handle_methods(methods);

        add_base_handle_methods(methods);
    }
}
