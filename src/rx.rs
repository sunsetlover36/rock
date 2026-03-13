pub(crate) mod core;
pub(crate) use core::CorePipeline;

pub(crate) mod operator;
pub(crate) use operator::OpPipeline;

pub(crate) mod sync;

pub(crate) struct RxPipeline<T> {
    core: CorePipeline,
    op: OpPipeline,
}
