use super::*;
use crate::runtime::test_support::Fixture;

#[tokio::test(start_paused = true)]
async fn cancelled_slot_does_not_accept_stale_wake_or_error() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        task = scene.run(function(ctx) ctx:sleep(10); results.calls = results.calls + 100 end)
    "#,
    );
    f.settle().await;
    let old = f.scenes.threads.keys().next().unwrap();
    f.eval(
        r#"
        task:cancel()
        scene.run(function(ctx) ctx:sleep(10); results.calls = results.calls + 1 end)
    "#,
    );
    f.scene_tx
        .send(SceneManagerMessage::Wake {
            task_id: old,
            result: AsyncTaskResult::Nil,
        })
        .unwrap();
    f.scene_tx
        .send(SceneManagerMessage::Error {
            task_id: old,
            err: "stale failure".into(),
        })
        .unwrap();
    f.settle().await;
    assert_eq!(f.calls(), 0);
    assert_eq!(f.scenes.threads.len(), 1);
    assert_ne!(f.scenes.threads.keys().next().unwrap(), old);
    f.advance(10).await;
    assert_eq!(f.calls(), 1);
    assert!(f.scenes.threads.is_empty());
}

#[tokio::test(start_paused = true)]
async fn completed_async_result_is_not_delivered_after_disconnect() {
    let mut f = Fixture::new();
    let pk = rock_wire::PlayerKey {
        slot_idx: 0,
        generation: 0,
    };
    f.connect(pk);
    f.eval(
        r#"
        scene.create():while_connected(p):script(function()
            memory.fetch("profile")
            results.calls = results.calls + 1
        end):play()
    "#,
    );
    f.settle().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(f.completed.load(std::sync::atomic::Ordering::SeqCst), 1);
    f.disconnect(pk);
    f.tick();
    assert_eq!(f.calls(), 0);
    assert!(f.scenes.threads.is_empty());
}

#[tokio::test(start_paused = true)]
async fn self_clearing_scene_does_not_start_yielded_operation() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        world = layer.create():with(function()
            scene.run(function()
                world:clear()
                memory.fetch("must-not-start")
                results.calls = results.calls + 1
            end)
        end):commit()
    "#,
    );
    f.advance(20).await;
    assert_eq!(f.calls(), 0);
    assert_eq!(f.completed.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(f.scenes.threads.is_empty());
}

#[tokio::test(start_paused = true)]
async fn full_scene_queue_returns_a_lua_error_instead_of_blocking_runtime() {
    use crate::runtime::plugins::{ScenePlugin, protocol::GameModePlugin};
    let f = Fixture::new();
    let (tx, _rx) = flume::bounded(1);
    let plugin = ScenePlugin { manager_tx: tx };
    f.lua
        .globals()
        .set("scene", plugin.create_api(&f.lua).unwrap().unwrap())
        .unwrap();
    f.eval(
        r#"
        scene.run(function() end)
        local ok, err = pcall(function() scene.run(function() end) end)
        assert(not ok and tostring(err):find("failed to queue task", 1, true))
    "#,
    );
}
