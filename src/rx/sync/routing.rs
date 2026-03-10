use mlua::{UserData, UserDataMethods};

use crate::{
    runtime::{
        EyreResultExt, app_data, get_app_data, get_str_hash,
        network_replicator::protocol::{PolicyFieldUpdate, PolicyRouting},
    },
    rx::sync::{HasPolicy, PolicyHandle},
};

pub(crate) fn add_routing_rx_sync_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPolicy + Clone + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("room", |_, this, name: String| {
        let mut next = this.clone();
        let id = get_str_hash(&name);
        next.policy_mut().routing = PolicyRouting::Pinned(id);
        Ok(next)
    });
}

pub(crate) fn add_routing_rx_handle_methods<T, M>(methods: &mut M)
where
    T: UserData + PolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("room", |lua, this, name: String| {
        get_app_data::<app_data::NetworkReplicator>(lua)?
            .update_policy(
                this.policy_id(),
                PolicyFieldUpdate::Room {
                    id: get_str_hash(&name),
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });
}
