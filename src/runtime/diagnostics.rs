//! Bounded, authenticated-API-facing diagnostics for live-code requests.
use super::{
    EventBus,
    app_data::ExecutionContext,
    lifetime::{WorkContext, WorkContextGuard},
};
use parking_lot::Mutex;
use serde::Serialize;
use std::{collections::VecDeque, sync::Arc};

const REQUEST_LIMIT: usize = 64;
const DIAGNOSTIC_LIMIT: usize = 32;
const MESSAGE_LIMIT: usize = 8192;

#[derive(Clone, Debug)]
pub(crate) struct SnippetSource {
    pub request_id: String,
    pub name: String,
}
#[derive(Default)]
pub(crate) struct ActiveSnippet(pub Option<SnippetSource>);
pub(crate) fn current_source(lua: &mlua::Lua) -> Option<SnippetSource> {
    lua.app_data_ref::<ActiveSnippet>()
        .and_then(|s| s.0.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestStatus {
    Queued,
    Executed,
    Failed,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Compile,
    Execute,
    EventOperator,
    EventHandler,
    Scene,
    Queue,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Diagnostic {
    pub cursor: u64,
    pub phase: Phase,
    pub message: String,
    pub line: Option<u32>,
    pub event: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct RequestReport {
    pub request_id: String,
    pub name: String,
    pub status: RequestStatus,
    pub diagnostics: VecDeque<Diagnostic>,
    pub next_cursor: u64,
    pub dropped_diagnostics: u64,
}
#[derive(Default)]
struct StoreInner {
    requests: VecDeque<RequestReport>,
    cursor: u64,
}
#[derive(Default)]
pub(crate) struct DiagnosticStore {
    inner: Mutex<StoreInner>,
}

fn truncate(mut value: String) -> String {
    if value.len() > MESSAGE_LIMIT {
        let mut end = MESSAGE_LIMIT;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        value.push_str("\n[Diagnostic truncated]");
    }
    value
}
impl DiagnosticStore {
    pub fn submit(&self, name: Option<String>) -> Option<SnippetSource> {
        let mut inner = self.inner.lock();
        if inner.requests.len() == REQUEST_LIMIT {
            let index = inner
                .requests
                .iter()
                .position(|r| r.status != RequestStatus::Queued)?;
            inner.requests.remove(index);
        }
        let source = SnippetSource {
            request_id: nanoid::nanoid!(),
            name: name
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "anonymous impromptu".into())
                .chars()
                .take(256)
                .collect(),
        };
        let cursor = inner.cursor;
        inner.requests.push_back(RequestReport {
            request_id: source.request_id.clone(),
            name: source.name.clone(),
            status: RequestStatus::Queued,
            diagnostics: VecDeque::new(),
            next_cursor: cursor,
            dropped_diagnostics: 0,
        });
        Some(source)
    }
    pub fn source(&self, id: &str) -> Option<SnippetSource> {
        let inner = self.inner.lock();
        let report = inner.requests.iter().find(|r| r.request_id == id)?;
        Some(SnippetSource {
            request_id: report.request_id.clone(),
            name: report.name.clone(),
        })
    }
    pub fn finish(&self, source: &SnippetSource, status: RequestStatus) {
        if let Some(report) = self
            .inner
            .lock()
            .requests
            .iter_mut()
            .find(|r| r.request_id == source.request_id)
        {
            report.status = status;
        }
    }
    pub fn report(&self, id: &str, after: u64) -> Option<RequestReport> {
        let mut report = self
            .inner
            .lock()
            .requests
            .iter()
            .find(|r| r.request_id == id)?
            .clone();
        report.diagnostics.retain(|d| d.cursor > after);
        Some(report)
    }
    pub fn error(
        &self,
        source: &SnippetSource,
        phase: Phase,
        message: String,
        event: Option<String>,
    ) -> Diagnostic {
        let marker = format!("impromptu/{}:", source.request_id);
        let line = message
            .split(&marker)
            .nth(1)
            .and_then(|tail| tail.split(':').next())
            .and_then(|n| n.parse().ok());
        let message = truncate(message);
        let mut inner = self.inner.lock();
        inner.cursor += 1;
        let cursor = inner.cursor;
        let diagnostic = Diagnostic {
            cursor,
            phase,
            message,
            line,
            event,
        };
        if let Some(report) = inner
            .requests
            .iter_mut()
            .find(|r| r.request_id == source.request_id)
        {
            if report.diagnostics.len() == DIAGNOSTIC_LIMIT {
                report.diagnostics.pop_front();
                report.dropped_diagnostics += 1;
            }
            report.diagnostics.push_back(diagnostic.clone());
            report.next_cursor = cursor;
        }
        diagnostic
    }
}

pub(crate) fn function_source(lua: &mlua::Lua, function: &mlua::Function) -> Option<SnippetSource> {
    let source = function.info().source?;
    let id = source.strip_prefix("@impromptu/")?;
    // History expiry must not turn live-code callbacks into fatal gamemode code.
    Some(
        lua.app_data_ref::<Arc<DiagnosticStore>>()
            .and_then(|store| store.source(id))
            .unwrap_or_else(|| SnippetSource {
                request_id: id.into(),
                name: format!("impromptu/{id}"),
            }),
    )
}

pub(crate) fn record_error(
    lua: &mlua::Lua,
    source: Option<&SnippetSource>,
    phase: Phase,
    message: String,
    event: Option<String>,
) {
    if let (Some(source), Some(store)) = (source, lua.app_data_ref::<Arc<DiagnosticStore>>()) {
        store.error(source, phase, message, event);
    }
}

pub(crate) fn execute(
    lua: &mlua::Lua,
    bus: &EventBus,
    source: &SnippetSource,
    request: &rock_wire::ImpromptuRequest,
) {
    use super::plugins::on::protocol::{
        EventScope, GameModeEvent, GameModeEventData, WorldEventData,
    };
    bus.schedule_event(GameModeEvent {
        scopes: smallvec::smallvec![EventScope::Global],
        data: GameModeEventData::World(WorldEventData::Impromptu {
            name: request.name.clone(),
        }),
    });
    let result = (|| -> Result<(), (Phase, mlua::Error)> {
        let mut context = WorkContext::capture(lua, &[]).map_err(|e| (Phase::Execute, e))?;
        context.execution = ExecutionContext::Impromptu;
        context.origin = Some(source.clone());
        let _guard = WorkContextGuard::enter(lua, &context).map_err(|e| (Phase::Execute, e))?;
        let env = lua.create_table().map_err(|e| (Phase::Execute, e))?;
        env.set("_G", env.clone())
            .map_err(|e| (Phase::Execute, e))?;
        let mt = lua.create_table().map_err(|e| (Phase::Execute, e))?;
        mt.set("__index", lua.globals())
            .map_err(|e| (Phase::Execute, e))?;
        env.set_metatable(Some(mt))
            .map_err(|e| (Phase::Execute, e))?;
        let function = lua
            .load(&request.code)
            .set_name(format!("@impromptu/{}", source.request_id))
            .set_environment(env)
            .into_function()
            .map_err(|e| (Phase::Compile, e))?;
        function.call::<()>(()).map_err(|e| (Phase::Execute, e))
    })();
    let status = match result {
        Ok(()) => RequestStatus::Executed,
        Err((phase, error)) => {
            record_error(lua, Some(source), phase, error.to_string(), None);
            eprintln!(
                "[IMPROMPTU] {} ({}): {error}",
                source.name, source.request_id
            );
            RequestStatus::Failed
        }
    };
    if let Some(store) = lua.app_data_ref::<Arc<DiagnosticStore>>() {
        store.finish(source, status);
    }
}

#[cfg(test)]
mod tests;
