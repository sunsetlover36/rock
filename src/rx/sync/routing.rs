use mlua::{UserData, UserDataMethods};

use crate::{
    runtime::{
        EyreResultExt, app_data, get_app_data,
        network_replicator::protocol::{PolicyFieldUpdate, PolicyRouting, ReplicationTarget},
        room_str_to_id,
    },
    rx::sync::{HasPolicy, PolicyHandle},
};

pub(crate) fn add_routing_rx_sync_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPolicy + Clone + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("room", |lua, this, name: String| {
        match this.policy().target {
            ReplicationTarget::Blueprint(_) | ReplicationTarget::Entity(_) => {
                return Err(mlua::Error::runtime("Cannot pin a policy to the room if a policy has a blueprint or an entity target"));
            }
            _ => {}
        }

        let mut next = this.clone();
        let id = room_str_to_id(lua, &name)?;
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
                    id: room_str_to_id(lua, &name)?,
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });
}
