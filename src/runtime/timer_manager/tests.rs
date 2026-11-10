use super::*;
use crate::runtime::test_support::Fixture;

#[tokio::test(start_paused = true)]
async fn layer_cleanup_cancels_pending_and_running_timers() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.timer.fire():each(function() results.calls = results.calls + 1 end)
        pending = layer.create():with(function()
            timer.create():timeout(0):register("pending")
        end):commit()
        pending:clear()
        running = layer.create():with(function()
            timer.create():interval(1):register("running")
            timer.create():cron("*/1 * * * * *"):register("cron")
        end):commit()
    "#,
    );
    f.settle().await;
    f.eval("running:clear()");
    f.advance(5).await;
    assert_eq!(f.calls(), 0);
    assert!(f.timers.timers.borrow().is_empty());
}

#[tokio::test(start_paused = true)]
async fn queued_timer_event_is_suppressed_after_layer_clear() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.timer.fire():each(function() results.calls = results.calls + 1 end)
        world = layer.create():with(function()
            timer.create():timeout(1):register("queued")
        end):commit()
    "#,
    );
    f.settle().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    f.timers.tick(&f.lua); // Put the event into EventBus, but do not dispatch it.
    f.eval("world:clear()");
    f.bus.flush(&f.lua).unwrap();
    assert_eq!(f.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn named_cancel_covers_pending_timer_and_old_handle_cannot_cancel_new_instance() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.timer.fire():each(function() results.calls = results.calls + 1 end)
        old = timer.create():timeout(1):register("reused")
        timer.cancel("reused")
        timer.create():timeout(1):register("reused")
        old:cancel()
    "#,
    );
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.calls(), 1);
    assert!(f.timers.timers.borrow().is_empty());
}

#[tokio::test(start_paused = true)]
async fn clearing_one_owner_does_not_cancel_other_timeout_with_same_name() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        on.timer.fire():each(function() results.calls = results.calls + 1 end)
        world = layer.create():with(function()
            timer.create():timeout(1):register("shared")
        end):commit()
        timer.create():timeout(1):register("shared")
        world:clear()
    "#,
    );
    f.settle().await;
    f.advance(1).await;
    assert_eq!(f.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn stale_channel_event_does_not_target_reused_name() {
    let mut f = Fixture::new();
    f.eval("old = timer.create():timeout(10):register('shared')");
    f.tick();
    let old_key = f.timers.timers.borrow().keys().next().unwrap();
    f.eval("old:cancel()");
    f.tick();
    f.eval(
        r#"
        timer.create():timeout(10):register("shared")
        on.timer.fire():each(function() results.calls = results.calls + 1 end)
    "#,
    );
    f.timers
        .tx
        .send(TimerEvent {
            key: old_key,
            data: None,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.calls(), 0);
    f.advance(10).await;
    assert_eq!(f.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn interval_duplicates_are_rejected_before_start_and_zero_is_a_lua_error() {
    let mut f = Fixture::new();
    f.eval(
        r#"
        local first = timer.create():interval(1):register("duplicate")
        assert(not pcall(function() timer.create():interval(1):register("duplicate") end))
        assert(not pcall(function() timer.create():interval(0):register("zero") end))
        first:cancel()
    "#,
    );
    f.settle().await;
    assert!(f.timers.timers.borrow().is_empty());
}
