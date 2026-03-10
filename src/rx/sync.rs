use std::time::Duration;

use mlua::{UserData, UserDataMethods};

use crate::runtime::{
    EyreResultExt, app_data, get_app_data,
    network_replicator::protocol::{
        PolicyFieldUpdate, PolicyId, PolicyRouting, ReplicationPolicy, ReplicationTarget,
    },
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
    T: UserData + HasPolicy + ToPolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("commit", |lua, this, _: ()| {
        let policy = this.policy().clone();
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
        get_app_data::<app_data::NetworkReplicator>(lua)?.revoke_policy(this.policy_id());
        Ok(())
    });

    methods.add_method("throttle", |lua, this, secs: Option<f64>| {
        get_app_data::<app_data::NetworkReplicator>(lua)?
            .update_policy(
                this.policy_id(),
                PolicyFieldUpdate::Throttle {
                    throttle: secs.map(Duration::from_secs_f64),
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });
}
