use mlua::{UserData, UserDataMethods};

use crate::{
    runtime::{
        app_data, get_app_data,
        network_replicator::protocol::{
            PolicyId, PolicyRouting, ReplicationPolicy, ReplicationTarget,
        },
    },
    rx::HasPipeline,
};

pub(crate) mod entity;
pub(crate) mod routing;
pub(crate) mod spatial;

pub(crate) trait PolicyHandle {
    fn policy_id(&self) -> PolicyId;
}
pub(crate) trait ToPolicyHandle {
    type Handle: mlua::UserData + PolicyHandle + Send + 'static;
    fn to_policy_handle(&self, id: PolicyId) -> Self::Handle;
}
pub(crate) trait HasPolicy {
    fn policy(&self) -> &ReplicationPolicy;
    fn policy_mut(&mut self) -> &mut ReplicationPolicy;
}

pub(crate) fn add_sync_consumer_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPipeline + HasPolicy + ToPolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("commit", |lua, this, _: ()| {
        let mut policy = this.policy().clone();
        match &policy.target {
            ReplicationTarget::MemoryNode(node) => {
                if policy.routing == PolicyRouting::DynamicFollow {
                    return Err(mlua::Error::runtime(format!(
                        "Failed to commit a policy: memory node '{}' requires a target room",
                        node
                    )));
                }
            }
            _ => {}
        }

        policy.pipeline = this.pipeline().clone();

        let id = get_app_data::<app_data::NetworkReplicator>(lua)?.commit_policy(policy);
        Ok(this.to_policy_handle(id))
    });
}

pub(crate) fn add_base_handle_methods<T, M>(methods: &mut M)
where
    T: UserData + PolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("revoke", |lua, this, _: ()| {
        get_app_data::<app_data::NetworkReplicator>(lua)?.revoke_policy_by_id(this.policy_id());
        Ok(())
    });
}
