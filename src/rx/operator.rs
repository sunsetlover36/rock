use mlua::{UserData, UserDataMethods};

#[derive(Debug, Clone)]
pub(crate) enum RxOp {
    Filter(mlua::Function),
    Map(mlua::Function),
}

#[derive(Clone, Default)]
pub(crate) struct OpPipeline {
    pub operators: Vec<RxOp>,
}
impl OpPipeline {
    pub fn process(&self, mut args: mlua::MultiValue) -> mlua::Result<Option<mlua::MultiValue>> {
        for op in &self.operators {
            match op {
                RxOp::Filter(predicate) => {
                    let result = predicate.call::<bool>(&args)?;

                    if !result {
                        return Ok(None);
                    }
                }
                RxOp::Map(mapper) => {
                    let new_args = mapper.call::<mlua::MultiValue>(args)?;
                    args = new_args;
                }
            }
        }

        Ok(Some(args))
    }
}

pub(crate) trait HasOpPipeline: Clone + 'static {
    fn op_pipeline_mut(&mut self) -> &mut OpPipeline;
}

pub(crate) fn add_op_pipeline_methods<T, M>(methods: &mut M)
where
    T: UserData + HasOpPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("where", |_, this, predicate: mlua::Function| {
        let mut next = this.clone();
        next.op_pipeline_mut()
            .operators
            .push(RxOp::Filter(predicate));
        Ok(next)
    });

    methods.add_method("select", |_, this, selector: mlua::Function| {
        let mut next = this.clone();
        next.op_pipeline_mut().operators.push(RxOp::Map(selector));
        Ok(next)
    });
}

pub(crate) struct OpSentry {
    pipeline: OpPipeline,
}
