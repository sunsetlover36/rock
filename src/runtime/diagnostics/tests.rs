use super::*;
use crate::runtime::test_support::Fixture;

fn store(f: &Fixture) -> Arc<DiagnosticStore> {
    if let Some(store) = f.lua.app_data_ref::<Arc<DiagnosticStore>>() {
        return store.clone();
    }
    let store = Arc::new(DiagnosticStore::default());
    f.lua.set_app_data(store.clone());
    store
}
fn inject(f: &Fixture, name: &str, code: &str) -> SnippetSource {
    let source = store(f).submit(Some(name.into())).unwrap();
    execute(
        &f.lua,
        &f.bus,
        &source,
        &rock_wire::ImpromptuRequest {
            name: Some(name.into()),
            code: code.into(),
        },
    );
    source
}

#[tokio::test(start_paused = true)]
async fn immediate_success_compile_and_runtime_errors_have_actual_outcomes() {
    let f = Fixture::new();
    let success = inject(&f, "working", "results.calls = 1");
    assert_eq!(
        store(&f).report(&success.request_id, 0).unwrap().status,
        RequestStatus::Executed
    );
    let syntax = inject(&f, "broken syntax", "local x =\n)");
    let report = store(&f).report(&syntax.request_id, 0).unwrap();
    assert_eq!(report.status, RequestStatus::Failed);
    assert!(matches!(report.diagnostics[0].phase, Phase::Compile));
    assert_eq!(report.diagnostics[0].line, Some(2));
    let failure = inject(&f, "broken code", "local rock = nil\nrock:despawn()");
    let report = store(&f).report(&failure.request_id, 0).unwrap();
    assert_eq!(report.name, "broken code");
    assert_eq!(report.status, RequestStatus::Failed);
    assert!(matches!(report.diagnostics[0].phase, Phase::Execute));
    assert_eq!(report.diagnostics[0].line, Some(2));
    assert!(report.diagnostics[0].message.contains("stack traceback"));
    assert!(current_source(&f.lua).is_none());
    assert!(matches!(
        *f.lua.app_data_ref::<ExecutionContext>().unwrap(),
        ExecutionContext::Global
    ));
}

#[tokio::test(start_paused = true)]
async fn deferred_operator_and_handler_errors_are_attributed_and_do_not_stop_other_listeners() {
    let mut f = Fixture::new();
    let op = inject(
        &f,
        "bad filter",
        r#"
        on.world.impromptu():priority(10)
          :where(function() error("Filter failed") end)
          :each(function() end)
    "#,
    );
    let handler = inject(
        &f,
        "bad handler",
        r#"
        on.world.impromptu():each(function() error("Handler failed") end)
    "#,
    );
    f.eval("on.world.impromptu():each(function() results.calls = results.calls + 1 end)");
    f.fire("trigger");
    f.settle().await;
    let op_report = store(&f).report(&op.request_id, 0).unwrap();
    assert_eq!(op_report.status, RequestStatus::Executed);
    assert!(
        op_report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::EventOperator)
                && d.message.contains("Filter failed")
                && d.event.as_deref() == Some("World(Impromptu)"))
    );
    let handler_report = store(&f).report(&handler.request_id, 0).unwrap();
    assert!(
        handler_report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::EventHandler)
                && d.message.contains("Handler failed")
                && d.line.is_some())
    );
    assert!(f.calls() > 0);
    assert!(current_source(&f.lua).is_none());
}

#[tokio::test(start_paused = true)]
async fn async_scene_error_after_yield_keeps_snippet_origin() {
    let mut f = Fixture::new();
    let source = inject(
        &f,
        "async failure",
        r#"
        scene.run(function(ctx)
            ctx:sleep(1)
            error("Resumed scene failed")
        end)
    "#,
    );
    f.settle().await;
    f.advance(1).await;
    let report = store(&f).report(&source.request_id, 0).unwrap();
    assert_eq!(report.status, RequestStatus::Executed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::Scene)
                && d.message.contains("Resumed scene failed")
                && d.line == Some(4))
    );
    assert!(current_source(&f.lua).is_none());
}

#[tokio::test(start_paused = true)]
async fn scene_created_by_deferred_handler_keeps_origin_and_named_scene_can_run_later() {
    let mut f = Fixture::new();
    let source = inject(
        &f,
        "deferred scene",
        r#"
        on.world.impromptu():take(1):each(function()
            scene.run(function(ctx)
                ctx:sleep(1)
                error("Nested failure")
            end)
        end)
        scene.create():script(function() error("Named failure") end):register("late")
    "#,
    );
    f.settle().await;
    f.fire("trigger");
    f.settle().await;
    f.eval("scene.play('late')");
    f.settle().await;
    f.advance(1).await;
    let report = store(&f).report(&source.request_id, 0).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::Scene) && d.message.contains("Nested failure"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::Scene) && d.message.contains("Named failure"))
    );
}

#[test]
fn bounded_reports_support_cursor_polling_and_duplicate_names() {
    let store = DiagnosticStore::default();
    let first = store.submit(Some("same".into())).unwrap();
    let second = store.submit(Some("same".into())).unwrap();
    assert_ne!(first.request_id, second.request_id);
    for n in 0..40 {
        store.error(&first, Phase::EventHandler, format!("Failure {n}"), None);
    }
    let report = store.report(&first.request_id, 0).unwrap();
    assert_eq!(report.diagnostics.len(), DIAGNOSTIC_LIMIT);
    assert_eq!(report.dropped_diagnostics, 8);
    let cursor = report.next_cursor;
    assert!(
        store
            .report(&first.request_id, cursor)
            .unwrap()
            .diagnostics
            .is_empty()
    );
    store.error(&first, Phase::Scene, "New failure".into(), None);
    assert_eq!(
        store
            .report(&first.request_id, cursor)
            .unwrap()
            .diagnostics
            .len(),
        1
    );
    assert!(
        store
            .report(&second.request_id, 0)
            .unwrap()
            .diagnostics
            .is_empty()
    );
}

#[test]
fn request_limit_does_not_evict_pending_work_and_finished_reports_can_expire() {
    let store = DiagnosticStore::default();
    let first = store.submit(None).unwrap();
    for _ in 1..REQUEST_LIMIT {
        store.submit(None).unwrap();
    }
    assert!(store.submit(None).is_none());
    store.finish(&first, RequestStatus::Executed);
    assert!(store.submit(None).is_some());
    assert!(store.report(&first.request_id, 0).is_none());
}

#[test]
fn diagnostics_limit_is_utf8_safe() {
    let store = DiagnosticStore::default();
    let source = store.submit(None).unwrap();
    store.error(&source, Phase::Execute, "☃".repeat(MESSAGE_LIMIT), None);
    let report = store.report(&source.request_id, 0).unwrap();
    assert!(
        report.diagnostics[0]
            .message
            .ends_with("[Diagnostic truncated]")
    );
    assert!(report.diagnostics[0].message.len() < MESSAGE_LIMIT + 64);
}

#[tokio::test(start_paused = true)]
async fn unnamed_request_preserves_nil_in_impromptu_event() {
    let mut f = Fixture::new();
    f.eval("on.world.impromptu():each(function(name) results.was_nil = name == nil end)");
    let source = store(&f).submit(None).unwrap();
    execute(
        &f.lua,
        &f.bus,
        &source,
        &rock_wire::ImpromptuRequest {
            name: None,
            code: "print('ok')".into(),
        },
    );
    f.settle().await;
    f.eval("assert(results.was_nil)");
}

#[tokio::test(start_paused = true)]
async fn report_expiry_does_not_change_live_code_error_isolation() {
    let mut f = Fixture::new();
    let source = inject(
        &f,
        "persistent template",
        r#"
        scene.create():script(function()
            on.world.impromptu():each(function() error("Late callback failed") end)
        end):register("persistent")
    "#,
    );
    f.settle().await;
    let history = store(&f);
    for _ in 0..REQUEST_LIMIT {
        let other = history.submit(None).unwrap();
        history.finish(&other, RequestStatus::Executed);
    }
    assert!(history.report(&source.request_id, 0).is_none());
    f.eval("scene.play('persistent')");
    f.settle().await;
    f.fire("after-expiry");
    f.settle().await;
}
