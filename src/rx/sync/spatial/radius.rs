use mlua::{UserData, UserDataMethods};

use crate::{
    runtime::{
        EyreResultExt, app_data, get_app_data,
        network_replicator::protocol::{PolicyFieldUpdate, SpatialFilter},
    },
    rx::sync::{HasPolicy, PolicyHandle},
};

pub(crate) fn add_radius_sync_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPolicy + Clone + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("radius", |_, this, radius: f32| {
        let mut next = this.clone();
        next.policy_mut().spatial = SpatialFilter::Radius(radius);
        Ok(next)
    });
}

pub(crate) fn add_radius_handle_methods<T, M>(methods: &mut M)
where
    T: UserData + PolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("radius", |lua, this, radius: f32| {
        get_app_data::<app_data::NetworkReplicator>(lua)?
            .update_policy(
                this.policy_id(),
                PolicyFieldUpdate::Spatial {
                    filter: SpatialFilter::Radius(radius),
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });
}
