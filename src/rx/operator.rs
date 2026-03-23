use mlua::{UserData, UserDataMethods};

use super::HasPipeline;

#[derive(Debug, Clone)]
pub(crate) enum RxOp {
    Filter(mlua::Function),
    Map(mlua::Function),
    Changed,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OpPipeline {
    pub operators: Vec<RxOp>,
}

pub(crate) fn add_op_pipeline_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("where", |_, this, predicate: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut()
            .op
            .operators
            .push(RxOp::Filter(predicate));
        Ok(next)
    });

    methods.add_method("select", |_, this, selector: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut().op.operators.push(RxOp::Map(selector));
        Ok(next)
    });

    methods.add_method("changed", |_, this, _: ()| {
        let mut next = this.clone();
        next.pipeline_mut().op.operators.push(RxOp::Changed);
        Ok(next)
    });
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OpSentry {
    pipeline: OpPipeline,
    last_value: Option<mlua::MultiValue>,
}
impl OpSentry {
    pub fn new(pipeline: OpPipeline) -> Self {
        Self {
            pipeline,
            last_value: None,
        }
    }

    pub fn process(
        &mut self,
        mut args: mlua::MultiValue,
    ) -> mlua::Result<Option<mlua::MultiValue>> {
        for op in &self.pipeline.operators {
            match op {
                RxOp::Filter(predicate) => {
                    let result = predicate.call::<bool>(args.clone())?;

                    if !result {
                        return Ok(None);
                    }
                }
                RxOp::Map(mapper) => {
                    let new_args = mapper.call::<mlua::MultiValue>(args)?;
                    args = new_args;
                }
                RxOp::Changed => match self.last_value.clone() {
                    Some(v) => {
                        // WARNING: tables are being compared by a ref (not shallow eq)
                        if v.into_vec() == args.clone().into_vec() {
                            return Ok(None);
                        } else {
                            self.last_value = Some(args.clone());
                        }
                    }
                    None => {
                        self.last_value = Some(args.clone());
                    }
                },
            }
        }

        Ok(Some(args))
    }
}
