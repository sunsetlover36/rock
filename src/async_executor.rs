use crate::gamemode::{AsyncTaskWithId, SchedulerMessage};

pub struct AsyncExecutorParams {
    pub channel_buffer: usize,
    pub scheduler_tx: flume::Sender<SchedulerMessage>,
}
pub fn create_async_executor(params: AsyncExecutorParams) -> flume::Sender<AsyncTaskWithId> {
    let AsyncExecutorParams {
        channel_buffer,
        scheduler_tx,
    } = params;
    let (tx, rx) = flume::bounded::<AsyncTaskWithId>(channel_buffer);

    tokio::spawn(async move {
        while let Ok(task) = rx.recv_async().await {
            let scheduler_tx = scheduler_tx.clone();
            tokio::spawn(handle_task(task, scheduler_tx));
        }
    });

    tx
}

async fn handle_task(task: AsyncTaskWithId, scheduler_tx: flume::Sender<SchedulerMessage>) {
    let AsyncTaskWithId { id, future } = task;

    match future.await {
        Ok(result) => {
            let _ = scheduler_tx.send(SchedulerMessage::Wake {
                task_id: id,
                result,
            });
        }
        Err(err) => {
            let _ = scheduler_tx.send(SchedulerMessage::Error {
                task_id: id,
                err: err.to_string(),
            });
        }
    }
}
