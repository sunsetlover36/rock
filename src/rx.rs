use mlua::{UserData, UserDataMethods};

mod sync;
pub(crate) use sync::RxSync;

#[derive(Debug, Clone)]
pub(crate) enum RxOperator {
    Filter(mlua::Function),
    Map(mlua::Function),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RxPipeline {
    pub limit: Option<u32>,
    pub operators: Vec<RxOperator>,
}
impl RxPipeline {
    pub fn process(&self, mut args: mlua::MultiValue) -> mlua::Result<Option<mlua::MultiValue>> {
        for op in &self.operators {
            match op {
                RxOperator::Filter(predicate) => {
                    let result = predicate.call::<bool>(&args)?;

                    if !result {
                        return Ok(None);
                    }
                }
                RxOperator::Map(mapper) => {
                    let new_args = mapper.call::<mlua::MultiValue>(args)?;
                    args = new_args;
                }
            }
        }

        Ok(Some(args))
    }
}

pub(crate) trait HasRxPipeline: Clone + 'static {
    fn pipeline_mut(&mut self) -> &mut RxPipeline;
}

pub(crate) fn add_rx_methods<T, M>(methods: &mut M)
where
    T: UserData + HasRxPipeline,
    M: UserDataMethods<T>,
{
    methods.add_method("take", |_, this, n: u32| {
        let mut next = this.clone();
        next.pipeline_mut().limit = Some(n);
        Ok(next)
    });

    methods.add_method("where", |_, this, predicate: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut()
            .operators
            .push(RxOperator::Filter(predicate));
        Ok(next)
    });

    methods.add_method("select", |_, this, selector: mlua::Function| {
        let mut next = this.clone();
        next.pipeline_mut()
            .operators
            .push(RxOperator::Map(selector));
        Ok(next)
    });
}
