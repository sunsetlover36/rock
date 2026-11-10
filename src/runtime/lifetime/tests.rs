use super::*;
use crate::runtime::test_support::Fixture;
use std::sync::atomic::Ordering;

fn pk(generation: u32) -> PlayerKey {
    PlayerKey {
        slot_idx: 0,
        generation,
    }
}

#[tokio::test(start_paused = true)]
async fn queued_and_sleeping_scenes_are_cancelled_with_layer() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        queued = layer.create():with(function()
            scene.run(function() results.calls = results.calls + 1 end)
        end):commit()
        queued:clear()
        sleeping = layer.create():with(function()
            scene.run(function(ctx)
                ctx:sleep(10)
                results.calls = results.calls + 10
            end)
        end):commit()
    "#,
    );
    f.settle().await;
    f.eval("sleeping:clear()");
    f.advance(20).await;
    assert_eq!(f.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn cancelling_layer_aborts_inflight_plugin_future() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        world = layer.create():with(function()
            scene.run(function()
                memory.fetch("profile")
                results.calls = results.calls + 1
            end)
        end):commit()
    "#,
    );
    f.settle().await;
    f.eval("world:clear()");
    f.settle().await;
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
    f.advance(20).await;
    assert_eq!(f.completed.load(Ordering::SeqCst), 0);
    assert_eq!(f.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn resumed_scene_keeps_ownership_across_yields() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        world = layer.create():with(function()
            scene.run(function(ctx)
                ctx:sleep(1)
                ctx:sleep(1)
                entity.blueprint():custom({ health = 100 }):spawn()
                on.world.impromptu():each(function() results.calls = results.calls + 1 end)
                timer.create():timeout(5):register("late")
                scene.run(function(child)
                    child:sleep(5)
                    results.calls = results.calls + 100
                end)
            end)
        end):commit()
    "#,
    );
    f.settle().await;
    f.advance(1).await;
    f.advance(1).await;
    assert_eq!(f.lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    assert!(
        f.lua
            .app_data_ref::<app_data::ActiveLayers>()
            .unwrap()
            .0
            .is_empty()
    );
    f.eval("world:clear()");
    f.fire("late");
    f.advance(10).await;
    assert_eq!(f.calls(), 0);
    assert!(
        f.lua
            .app_data_ref::<app_data::World>()
            .unwrap()
            .0
            .is_empty()
    );
    assert!(
        f.lua
            .app_data_ref::<app_data::EntityCustoms>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[tokio::test(start_paused = true)]
async fn manual_scene_cancel_and_unowned_scene_are_independent() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        local cancelled = scene.run(function() results.calls = results.calls + 100 end)
        cancelled:cancel()
        world = layer.create():commit()
        scene.run(function(ctx)
            ctx:sleep(1)
            results.calls = results.calls + 1
        end)
        world:clear()
    "#,
    );
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn player_event_work_is_cancelled_but_entities_and_global_listener_remain() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        results.connections = 0
        world = layer.create():with(function()
            on.player.online():each(function(p)
                results.connections = results.connections + 1
                entity.blueprint():spawn()
                scene.run(function(ctx)
                    ctx:sleep(10)
                    p:signal("loaded"):send()
                end)
                timer.create():interval(1):register("timer/" .. p:id())
                on.world.impromptu():each(function() results.calls = results.calls + 1 end)
            end)
        end):commit()
    "#,
    );
    f.online(pk(0));
    f.settle().await;
    f.disconnect(pk(0));
    f.fire("after-disconnect");
    f.advance(20).await;
    assert_eq!(f.calls(), 0);
    assert_eq!(f.clients.sent.load(Ordering::SeqCst), 0);
    assert_eq!(f.lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    f.online(pk(1));
    f.settle().await;
    f.advance(10).await;
    assert_eq!(f.clients.sent.load(Ordering::SeqCst), 1);
    f.eval("assert(results.connections == 2); world:clear()");
}

#[tokio::test(start_paused = true)]
async fn stale_online_event_is_skipped_but_offline_snapshot_work_runs() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.player.online():each(function() error("stale online event") end)
        on.player.offline():each(function(snapshot)
            local id = snapshot:id()
            scene.run(function(ctx)
                ctx:sleep(1)
                results.calls = results.calls + 1
                results.saved_id = id
            end)
        end)
    "#,
    );
    f.online(pk(0));
    f.disconnect(pk(0));
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.calls(), 1);
    f.eval("assert(results.saved_id == 0)");
}

#[tokio::test(start_paused = true)]
async fn disconnect_cancels_queued_player_scene_and_pending_async_operation() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.player.online():each(function()
            scene.run(function()
                memory.fetch("profile")
                results.calls = results.calls + 1
            end)
        end)
    "#,
    );
    f.online(pk(0));
    f.tick(); // Queued from the event, not started yet.
    f.disconnect(pk(0));
    f.advance(20).await;
    assert_eq!(f.calls(), 0);
    assert_eq!(f.completed.load(Ordering::SeqCst), 0);
    f.online(pk(1));
    f.settle().await;
    f.disconnect(pk(1));
    f.settle().await;
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
    f.advance(20).await;
    assert_eq!(f.completed.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn explicit_while_connected_applies_to_scenes_timers_and_subscriptions() {
    let mut f = Fixture::new();
    f.connect(pk(0));
    f.eval(r#"
        scene.create():while_connected(p):script(function(ctx)
            ctx:sleep(2)
            results.calls = results.calls + 100
        end):play()
        timer.create():while_connected(p):timeout(2):register("explicit")
        on.timer.fire():each(function() results.calls = results.calls + 10 end)
        on.world.impromptu():while_connected(p):each(function() results.calls = results.calls + 1 end)
    "#);
    f.settle().await;
    f.disconnect(pk(0));
    f.fire("after-disconnect");
    f.advance(5).await;
    assert_eq!(f.calls(), 0);
    f.eval(r#"
        assert(not pcall(function() scene.create():while_connected(p):script(function() end):play() end))
        assert(not pcall(function() timer.create():while_connected(p):timeout(1):register("stale") end))
        assert(not pcall(function() on.world.impromptu():while_connected(p):each(function() end) end))
    "#);
}

#[tokio::test(start_paused = true)]
async fn multiple_explicit_player_dependencies_cancel_on_either_player() {
    let mut f = Fixture::new();
    f.connect(pk(0));
    f.eval("first = p");
    let second = PlayerKey {
        slot_idx: 1,
        generation: 0,
    };
    f.connect(second);
    f.eval(
        r#"
        scene.create():while_connected(first):while_connected(p):script(function(ctx)
            ctx:sleep(1)
            results.calls = results.calls + 1
        end):play()
    "#,
    );
    f.settle().await;
    f.disconnect(second);
    f.advance(2).await;
    assert_eq!(f.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn named_scene_captures_automatic_dependency_at_play_not_definition() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.player.online():each(function()
            scene.create():script(function(ctx)
                ctx:sleep(1)
                results.calls = results.calls + 1
            end):register("shared")
        end)
    "#,
    );
    f.online(pk(0));
    f.settle().await;
    f.disconnect(pk(0));
    f.eval("scene.play('shared')");
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn stale_player_handles_and_prebuilt_signals_fail_only_their_scene() {
    let mut f = Fixture::new();
    f.connect(pk(0));
    f.eval("old = p; signal = p:signal('stale'); presence = p:presence(); vision = p:vision()");
    f.disconnect(pk(0));
    f.connect(pk(1));
    f.eval(
        r#"
        assert(not old:is_online() and p:is_online())
        assert(old:id() ~= p:id())
        assert(not pcall(function() presence:enter("world") end))
        assert(not pcall(function() vision:detach() end))
        scene.run(function()
            signal:send()
            results.calls = results.calls + 100
        end)
        scene.run(function() results.calls = results.calls + 1 end)
    "#,
    );
    f.settle().await;
    assert_eq!(f.calls(), 1);
    assert_eq!(f.clients.sent.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn timeout_events_inherit_source_player_and_layer_for_new_work() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.timer.fire():named("source"):each(function()
            entity.blueprint():spawn()
            scene.run(function(ctx)
                ctx:sleep(10)
                results.calls = results.calls + 1
            end)
        end)
        world = layer.create():with(function()
            on.player.online():each(function()
                timer.create():timeout(1):register("source")
            end)
        end):commit()
    "#,
    );
    f.online(pk(0));
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.lua.app_data_ref::<app_data::World>().unwrap().0.len(), 1);
    f.disconnect(pk(0));
    f.eval("world:clear()");
    f.advance(20).await;
    assert_eq!(f.calls(), 0);
    assert!(
        f.lua
            .app_data_ref::<app_data::World>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[tokio::test(start_paused = true)]
async fn disconnect_during_dispatch_suppresses_prepared_player_handler() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.player.online():priority(100):each(function(p) p:kick() end)
        on.player.online():select(function(p) return p:id() end):each(function()
            results.calls = results.calls + 1
        end)
    "#,
    );
    f.online(pk(0));
    f.settle().await;
    assert_eq!(f.calls(), 0);
}
