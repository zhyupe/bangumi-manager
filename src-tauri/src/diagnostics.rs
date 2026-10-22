use serde::Serialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const RECENT_LIMIT: usize = 200;
thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }

#[derive(Clone)]
pub struct Context {
    pub trace_id: String,
    pub span_id: String,
}
pub fn context() -> Option<Context> {
    CONTEXT.with(|c| c.borrow().clone())
}
pub struct Scope(Option<Context>);
pub fn enter(next: Option<Context>) -> Scope {
    Scope(CONTEXT.with(|c| c.replace(next)))
}
impl Drop for Scope {
    fn drop(&mut self) {
        CONTEXT.with(|c| c.replace(self.0.take()));
    }
}

#[derive(Clone, Serialize)]
pub struct Event {
    pub timestamp: String,
    pub session_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_id: Option<String>,
    pub operation: String,
    pub phase: String,
    pub status: String,
    pub elapsed_ms: f64,
    pub fields: Value,
}
#[derive(Clone, Default, Serialize)]
pub struct Statistics {
    pub operation: String,
    pub count: u64,
    pub failures: u64,
    pub total_ms: f64,
    pub max_ms: f64,
    pub last_ms: f64,
}
#[derive(Serialize)]
pub struct Report {
    pub directory: String,
    pub session_id: String,
    pub write_error: Option<String>,
    pub statistics: Vec<Statistics>,
    pub recent: Vec<Event>,
    pub active: Vec<Event>,
}
#[derive(Default)]
struct State {
    statistics: BTreeMap<String, Statistics>,
    recent: VecDeque<Event>,
    active: BTreeMap<String, (Event, Instant)>,
    write_error: Option<String>,
}
struct Inner {
    directory: PathBuf,
    session_id: String,
    state: Mutex<State>,
    max_bytes: u64,
}
#[derive(Clone)]
pub struct Diagnostics(Arc<Inner>);
impl Diagnostics {
    pub fn new(directory: PathBuf) -> Self {
        Self::with_limit(directory, MAX_LOG_BYTES)
    }
    fn with_limit(directory: PathBuf, max_bytes: u64) -> Self {
        Self(Arc::new(Inner {
            directory,
            session_id: crate::model::id(),
            state: Mutex::new(State::default()),
            max_bytes,
        }))
    }
    pub fn directory(&self) -> &Path {
        &self.0.directory
    }
    pub fn span(&self, operation: &str, fields: Value) -> Span {
        self.start(operation, fields, None, false)
    }
    pub fn request(&self, operation: &str, trace_id: Option<String>) -> Span {
        self.start(operation, json!({}), trace_id, false)
    }
    fn start(&self, operation: &str, fields: Value, trace_id: Option<String>, quiet: bool) -> Span {
        let parent = context();
        let event = Event {
            timestamp: crate::model::now(),
            session_id: self.0.session_id.clone(),
            trace_id: trace_id
                .filter(|s| uuid::Uuid::parse_str(s).is_ok())
                .or_else(|| parent.as_ref().map(|c| c.trace_id.clone()))
                .unwrap_or_else(crate::model::id),
            span_id: crate::model::id(),
            parent_id: parent.map(|c| c.span_id),
            operation: operation.into(),
            phase: "start".into(),
            status: "running".into(),
            elapsed_ms: 0.0,
            fields,
        };
        let started = Instant::now();
        if !quiet {
            self.record(event.clone(), Some(started), true);
        }
        Span {
            diagnostics: self.clone(),
            event,
            started,
            quiet,
            finished: false,
        }
    }
    pub fn measure<T, E: ToString>(
        &self,
        operation: &str,
        fields: Value,
        f: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        self.measure_with(operation, fields, |_| f())
    }
    pub fn measure_with<T, E: ToString>(
        &self,
        operation: &str,
        fields: Value,
        f: impl FnOnce(&mut Span) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut span = self.span(operation, fields);
        let _scope = span.enter();
        let result = f(&mut span);
        span.finish_result(&result);
        result
    }
    pub fn database<T, E: ToString>(
        &self,
        operation: &str,
        f: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut span = self.start(operation, json!({}), None, true);
        let _scope = span.enter();
        let result = f();
        span.finish_result(&result);
        result
    }
    pub fn frontend(
        &self,
        operation: &str,
        trace_id: String,
        elapsed_ms: f64,
        status: &str,
    ) -> crate::model::Result<()> {
        if operation.len() > 120
            || !operation
                .chars()
                .all(|c| c.is_alphanumeric() || "._ ·".contains(c))
            || !["ok", "error", "cancelled"].contains(&status)
            || !elapsed_ms.is_finite()
            || !(0.0..=604_800_000.0).contains(&elapsed_ms)
            || uuid::Uuid::parse_str(&trace_id).is_err()
        {
            return Err("无效的耗时记录".into());
        }
        self.record(
            Event {
                timestamp: crate::model::now(),
                session_id: self.0.session_id.clone(),
                trace_id,
                span_id: crate::model::id(),
                parent_id: None,
                operation: format!("ui.{operation}"),
                phase: "finish".into(),
                status: status.into(),
                elapsed_ms,
                fields: json!({}),
            },
            None,
            true,
        );
        Ok(())
    }
    fn record(&self, event: Event, started: Option<Instant>, persist: bool) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        if let Some(started) = started {
            state
                .active
                .insert(event.span_id.clone(), (event.clone(), started));
        } else {
            state.active.remove(&event.span_id);
            let stats = state
                .statistics
                .entry(event.operation.clone())
                .or_insert_with(|| Statistics {
                    operation: event.operation.clone(),
                    ..Default::default()
                });
            stats.count += 1;
            stats.failures += u64::from(event.status != "ok");
            stats.total_ms += event.elapsed_ms;
            stats.max_ms = stats.max_ms.max(event.elapsed_ms);
            stats.last_ms = event.elapsed_ms;
            if persist {
                state.recent.push_front(event.clone());
                state.recent.truncate(RECENT_LIMIT);
            }
        }
        if persist {
            // Never turn a logging failure into a media, database or network failure.
            if let Err(e) = self.write(&event) {
                state.write_error = Some(format!("日志写入失败：{e}"));
            } else {
                state.write_error = None;
            }
        }
    }
    fn write(&self, event: &Event) -> std::io::Result<()> {
        fs::create_dir_all(&self.0.directory)?;
        let path = self.0.directory.join("performance.jsonl");
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        if fs::metadata(&path).is_ok_and(|m| m.len() + line.len() as u64 > self.0.max_bytes) {
            let oldest = self.0.directory.join("performance.2.jsonl");
            if oldest.exists() {
                fs::remove_file(&oldest)?;
            }
            let previous = self.0.directory.join("performance.1.jsonl");
            if previous.exists() {
                fs::rename(&previous, oldest)?;
            }
            fs::rename(&path, previous)?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&line)
    }
    pub fn report(&self) -> Report {
        let state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut statistics: Vec<_> = state.statistics.values().cloned().collect();
        statistics.sort_by(|a, b| b.max_ms.total_cmp(&a.max_ms));
        Report {
            directory: crate::paths::display(&self.0.directory),
            session_id: self.0.session_id.clone(),
            write_error: state.write_error.clone(),
            statistics,
            recent: state.recent.iter().cloned().collect(),
            active: state
                .active
                .values()
                .map(|(e, t)| {
                    let mut e = e.clone();
                    e.elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
                    e
                })
                .collect(),
        }
    }
}
pub struct Span {
    diagnostics: Diagnostics,
    event: Event,
    started: Instant,
    quiet: bool,
    finished: bool,
}
impl Span {
    pub fn context(&self) -> Context {
        Context {
            trace_id: self.event.trace_id.clone(),
            span_id: self.event.span_id.clone(),
        }
    }
    pub fn enter(&self) -> Scope {
        enter(Some(self.context()))
    }
    pub fn field(&mut self, name: &str, value: impl Serialize) {
        self.event.fields[name] = serde_json::to_value(value).unwrap_or(Value::Null);
    }
    pub fn finish_result<T, E: ToString>(&mut self, result: &std::result::Result<T, E>) {
        let status = match result {
            Ok(_) => "ok",
            Err(e) if e.to_string().contains("取消") || e.to_string().contains("暂停") => {
                "cancelled"
            }
            Err(_) => "error",
        };
        self.finish(status);
    }
    pub fn finish(&mut self, status: &str) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.event.phase = "finish".into();
        self.event.timestamp = crate::model::now();
        self.event.elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.event.status = status.into();
        self.diagnostics.record(
            self.event.clone(),
            None,
            !self.quiet || status != "ok" || self.event.elapsed_ms >= 20.0,
        );
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        self.finish("interrupted");
    }
}

// Never log tokens, request bodies, arbitrary query values or local media paths.
pub fn endpoint(raw: &str) -> Value {
    let Ok(url) = url::Url::parse(raw) else {
        return json!({"endpoint":"invalid"});
    };
    let host = url.host_str().unwrap_or("");
    let trusted = matches!(host, "api.themoviedb.org" | "api.bgm.tv" | "musicbrainz.org");
    let query: BTreeMap<_, _> = url
        .query_pairs()
        .filter(|(k, _)| {
            ["language", "page", "limit", "offset", "subject_id"].contains(&k.as_ref())
        })
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    json!({"host":host,"route":if trusted {url.path()} else {"[image-or-other]"}, "query":if trusted {json!(query)} else {json!({})}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_timings_keep_trace_and_record_errors_without_error_contents() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().into());
        let trace = crate::model::id();
        let mut parent = d.request("ipc.media_detail", Some(trace.clone()));
        let scope = parent.enter();
        let error: Result<(), &str> = d.measure("http.request", json!({"cache":"miss"}), || {
            Err("secret-token")
        });
        assert!(error.is_err());
        assert_eq!(d.report().active.len(), 1);
        drop(scope);
        parent.finish("error");
        let report = d.report();
        assert!(report.active.is_empty());
        assert!(report
            .recent
            .iter()
            .all(|e| e.trace_id == trace && e.elapsed_ms >= 0.0));
        assert_eq!(
            report.recent[1].parent_id,
            Some(report.recent[0].span_id.clone())
        );
        let logs = fs::read_to_string(dir.path().join("performance.jsonl")).unwrap();
        assert!(!logs.contains("secret-token"));
        assert_eq!(logs.lines().count(), 4);
    }
    #[test]
    fn unfinished_spans_report_interruption_and_restore_thread_context() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().into());
        {
            let span = d.span("test", json!({}));
            let _scope = span.enter();
            assert!(context().is_some());
        }
        assert!(context().is_none());
        assert_eq!(d.report().recent[0].status, "interrupted");
        assert_eq!(d.report().statistics[0].failures, 1);
    }
    #[test]
    fn rotation_is_bounded_and_each_line_is_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::with_limit(dir.path().into(), 1000);
        for _ in 0..25 {
            d.span("test", json!({})).finish("ok");
        }
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 3);
        for file in files {
            for line in fs::read_to_string(file.unwrap().path()).unwrap().lines() {
                serde_json::from_str::<Value>(line).unwrap();
            }
        }
        assert_eq!(d.report().statistics[0].count, 25);
    }
    #[test]
    fn log_io_failure_does_not_change_business_result() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "occupied").unwrap();
        let d = Diagnostics::new(file);
        let result: Result<u32, &str> = d.measure("test", json!({}), || Ok(42));
        assert_eq!(result, Ok(42));
        assert!(d.report().write_error.is_some());
    }
    #[test]
    fn endpoint_and_frontend_validation_do_not_accept_arbitrary_payloads() {
        let safe = endpoint(
            "https://api.themoviedb.org/3/search/multi?api_key=secret&query=private&language=zh-CN",
        )
        .to_string();
        assert!(!safe.contains("secret") && !safe.contains("private"));
        assert!(safe.contains("zh-CN"));
        let image = endpoint("https://image.tmdb.org/private-file?token=secret").to_string();
        assert!(!image.contains("private-file") && !image.contains("secret"));
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().into());
        assert!(d
            .frontend("ipc.test", crate::model::id(), 1.0, "ok")
            .is_ok());
        assert!(d
            .frontend("secret\nbody", crate::model::id(), 1.0, "ok")
            .is_err());
        assert!(d.frontend("ipc.test", "invalid".into(), 1.0, "ok").is_err());
        assert!(d
            .frontend("ipc.test", crate::model::id(), f64::NAN, "ok")
            .is_err());
    }
    #[test]
    fn concurrent_requests_have_independent_contexts_and_complete_statistics() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().into());
        std::thread::scope(|s| {
            for _ in 0..8 {
                let d = d.clone();
                s.spawn(move || {
                    let mut root = d.span("ipc.test", json!({}));
                    let _scope = root.enter();
                    let _: Result<(), &str> = d.measure("http.test", json!({}), || Ok(()));
                    root.finish("ok");
                });
            }
        });
        let r = d.report();
        assert!(r.active.is_empty());
        assert_eq!(
            r.recent
                .iter()
                .filter(|e| e.operation == "ipc.test")
                .map(|e| &e.trace_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            8
        );
        assert!(r.statistics.iter().all(|s| s.count == 8));
    }
}
