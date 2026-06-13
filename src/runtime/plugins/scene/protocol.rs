use slotmap::new_key_type;
use strum::{AsRefStr, EnumString};

use crate::runtime::plugins::protocol::AsyncTaskResult;

new_key_type! {
    pub(crate) struct TaskId;
}

#[derive(Debug)]
pub(crate) enum SceneManagerMessage {
    AddTask {
        thread_rk: mlua::RegistryKey,
        label: String,
    },
    Wake {
        task_id: TaskId,
        result: AsyncTaskResult,
    },
    Cancel(TaskId),
    Error {
        task_id: TaskId,
        err: String,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, AsRefStr, EnumString)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum SceneYieldOp {
    Sleep,
}
