use mlua::{LuaSerdeExt, UserData, UserDataMethods};

use crate::{
    runtime::{
        EyreResultExt, app_data, get_app_data,
        network_replicator::protocol::{Area, PolicyFieldUpdate, SpatialFilter},
    },
    rx::sync::{HasPolicy, PolicyHandle},
};

mod radius;
pub(crate) use radius::{add_radius_handle_methods, add_radius_sync_methods};

pub(crate) fn add_area_rx_sync_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPolicy + Clone + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("global", |_, this, _: ()| {
        let mut next = this.clone();
        next.policy_mut().spatial = SpatialFilter::Global;
        Ok(next)
    });

    methods.add_method("area", |lua, this, area: mlua::Value| {
        let area: Area = lua.from_value(area)?;
        let mut next = this.clone();
        next.policy_mut().spatial = SpatialFilter::Area(area);
        Ok(next)
    });
}

pub(crate) fn add_area_rx_handle_methods<T, M>(methods: &mut M)
where
    T: UserData + PolicyHandle,
    M: UserDataMethods<T>,
{
    methods.add_method("global", |lua, this, _: ()| {
        get_app_data::<app_data::NetworkReplicator>(lua)?
            .0
            .update_policy(
                this.policy_id(),
                PolicyFieldUpdate::Spatial {
                    filter: SpatialFilter::Global,
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });

    methods.add_method("area", |lua, this, area: mlua::Table| {
        let area: Area = lua.from_value(mlua::Value::Table(area))?;
        get_app_data::<app_data::NetworkReplicator>(lua)?
            .0
            .update_policy(
                this.policy_id(),
                PolicyFieldUpdate::Spatial {
                    filter: SpatialFilter::Area(area),
                },
            )
            .wrap_eyre_err()?;
        Ok(())
    });
}
