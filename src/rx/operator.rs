use std::time::Duration;

use mlua::{UserData, UserDataMethods};

use super::HasPipeline;

#[derive(Debug, Clone)]
pub(crate) enum RxOp {
    Take(u32),
    Skip(u32),
    Throttle(Duration),
    Filter(mlua::Function),
    Map(mlua::Function),
    Changed,
}

pub(crate) fn add_op_pipeline_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("where", |_, this, predicate: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut().add_operator(RxOp::Filter(predicate));
        Ok(next)
    });

    methods.add_method("select", |_, this, selector: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut().add_operator(RxOp::Map(selector));
        Ok(next)
    });

    methods.add_method("changed", |_, this, _: ()| {
        let mut next = this.clone();
        next.pipeline_mut().add_operator(RxOp::Changed);
        Ok(next)
    });
}
