use crate::storage::{self, Settings, Storage};
use legacists_core::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

pub fn id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

pub struct Application {
    pub inner: Arc<Mutex<Inner>>,
}
pub struct Inner {
    pub storage: Storage,
    project: Option<(String, PathBuf)>,
    snapshot: Option<Arc<AnalysisSnapshot>>,
    job: Option<Job>,
    selection: BTreeSet<String>,
    selection_revision: u64,
    stale: bool,
    plan: Option<Plan>,
    failures: Vec<Diagnostic>,
}
struct Job {
    id: String,
    project: String,
    cancel: Cancellation,
    sequence: u64,
    status: Value,
    terminal: bool,
}
struct Plan {
    id: String,
    plan: ExportPlan,
    settings_revision: u64,
    selection_revision: u64,
}
impl Application {
    pub fn new(storage: Storage) -> Self {
        let selection = storage.settings.selected.iter().cloned().collect();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                storage,
                project: None,
                snapshot: None,
                job: None,
                selection,
                selection_revision: 0,
                stale: false,
                plan: None,
                failures: Vec::new(),
            })),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    request_id: String,
    operation: String,
    project_id: Option<String>,
    snapshot_id: Option<String>,
    #[serde(default)]
    args: Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn core_error(e: CoreError) -> &'static str {
    match e {
        CoreError::Cancelled => "CANCELLED",
        CoreError::InvalidId => "INVALID_ID",
        CoreError::InvalidCursor => "INVALID_CURSOR",
        CoreError::EmptySelection => "EMPTY_SELECTION",
        CoreError::DecodeFailed => "LEG-2001",
        CoreError::InputChanged => "LEG-5001",
        CoreError::InvalidLimit => "INVALID_LIMIT",
        CoreError::InputBoundary => "LEG-1002",
        CoreError::InvalidRange => "INVALID_RANGE",
        _ => "LEG-9001",
    }
}
fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> storage::Result<T> {
    serde_json::from_value(args.get(name).cloned().ok_or("INVALID_REQUEST")?)
        .map_err(|_| "INVALID_REQUEST")
}

impl Inner {
    fn project(&self, request: &Request) -> storage::Result<&(String, PathBuf)> {
        let project = self.project.as_ref().ok_or("NO_PROJECT")?;
        if request.project_id.as_ref() != Some(&project.0) {
            return Err("INVALID_ID");
        }
        Ok(project)
    }
    fn snapshot(&self, request: &Request) -> storage::Result<Arc<AnalysisSnapshot>> {
        self.project(request)?;
        let s = self.snapshot.as_ref().ok_or("NO_SNAPSHOT")?;
        if request.snapshot_id.as_ref() != Some(&s.id) {
            return Err("STALE_SNAPSHOT");
        }
        Ok(s.clone())
    }
    fn running(&self) -> bool {
        self.job.as_ref().is_some_and(|j| !j.terminal)
    }
    fn invalidate(&mut self) {
        self.plan = None;
    }
    fn summary(&self) -> Value {
        json!({"projectId":self.project.as_ref().map(|p|&p.0),"snapshotId":self.snapshot.as_ref().map(|s|&s.id),"resources":self.snapshot.as_ref().map(|s|s.resources.len()),"diagnostics":self.snapshot.as_ref().map(|s|s.diagnostics.len()),"skipped":self.snapshot.as_ref().map(|s|&s.skipped),"webRoots":self.snapshot.as_ref().map(|s|&s.web_roots),"selection":self.selection,"selectionRevision":self.selection_revision,"stale":self.stale,"job":self.job.as_ref().map(|j|&j.status)})
    }
    fn settings(&self) -> Value {
        json!({"settings":self.storage.settings,"defaultRoot":self.storage.default_root,"currentRoot":self.storage.root,"bootstrap":self.storage.bootstrap_path,"notice":self.storage.notice,"pending":self.storage.pending})
    }
    fn report(&self) -> String {
        let mut entries = self
            .snapshot
            .as_ref()
            .map(|s| s.diagnostics.clone())
            .unwrap_or_default();
        entries.extend(self.failures.clone());
        let runtime = tauri::webview_version().unwrap_or_else(|_| "unavailable".into());
        diagnostics::render_diagnostics(
            &entries,
            env!("CARGO_PKG_VERSION"),
            &format!("Windows x64; WebView2 {runtime}"),
        )
    }
}

fn folder(app: &tauri::AppHandle) -> Option<PathBuf> {
    let mut dialog = app.dialog().file();
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&w);
    }
    dialog
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
}
fn destination(app: &tauri::AppHandle, name: &str) -> Option<PathBuf> {
    let mut dialog = app
        .dialog()
        .file()
        .set_file_name(name)
        .add_filter("UTF-8 TXT", &["txt"]);
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&w);
    }
    dialog.blocking_save_file().and_then(|p| p.into_path().ok())
}

pub fn execute(
    request: &Request,
    inner: Arc<Mutex<Inner>>,
    app: &tauri::AppHandle,
) -> storage::Result<Value> {
    match request.operation.as_str() {
        "project_open" => {
            let Some(path) = folder(app) else {
                return Ok(json!({"cancelled":true}));
            };
            let input = FileInput::new(&path).map_err(core_error)?;
            let path = input.root().to_path_buf();
            let mut state = inner.lock().map_err(|_| "LEG-9001")?;
            if storage::overlap(&path, &state.storage.root) {
                return Err("LEG-1002");
            }
            if let Some(job) = &state.job {
                job.cancel.cancel();
            }
            state.project = Some((id("project"), path.clone()));
            state.snapshot = None;
            state.job = None;
            state.invalidate();
            state.stale = false;
            state.failures.clear();
            let mut settings = state.storage.settings.clone();
            if settings.project.as_ref() != Some(&path) {
                settings.selected.clear();
                settings.analysis = AnalysisOptions::default();
            }
            settings.project = Some(path);
            let rev = settings.revision;
            state.storage.update(settings, rev)?;
            state.selection = state.storage.settings.selected.iter().cloned().collect();
            state.selection_revision += 1;
            Ok(state.summary())
        }
        "analysis_start" => {
            let (project, path, job_id, cancel, options) = {
                let mut s = inner.lock().map_err(|_| "LEG-9001")?;
                let (project, path) = s.project(request)?.clone();
                if s.running() || s.storage.pending {
                    return Err("BUSY");
                }
                let revision: u64 = arg(&request.args, "settingsRevision")?;
                if revision != s.storage.settings.revision {
                    return Err("SETTINGS_CONFLICT");
                }
                let job_id = id("job");
                let cancel = Cancellation::default();
                s.invalidate();
                s.stale = s.snapshot.is_some();
                s.job = Some(Job {
                    id: job_id.clone(),
                    project: project.clone(),
                    cancel: cancel.clone(),
                    sequence: 0,
                    status: json!({"stage":"enumerate","completed":0,"jobId":job_id,"terminal":false}),
                    terminal: false,
                });
                (
                    project,
                    path,
                    job_id,
                    cancel,
                    s.storage.settings.analysis.clone(),
                )
            };
            let state = inner.clone();
            let application = app.clone();
            let response_job = job_id.clone();
            std::thread::spawn(move || {
                let input = FileInput::new(path);
                let result=input.and_then(|input|analyze(options,&input,&cancel,|p|{
                    if let Ok(mut s)=state.lock(){if let Some(job)=s.job.as_mut().filter(|j|j.id==job_id&&j.project==project&&!j.terminal){job.sequence+=1;job.status=json!({"projectId":project,"jobId":job_id,"sequence":job.sequence,"stage":p.stage,"completed":p.completed,"total":p.total,"terminal":false});let _=application.emit("analysis-progress",&job.status);}}
                }));
                if let Ok(mut s) = state.lock() {
                    if s.project.as_ref().map(|p| &p.0) != Some(&project)
                        || s.job.as_ref().map(|j| &j.id) != Some(&job_id)
                    {
                        return;
                    }
                    let result = if cancel.is_cancelled() {
                        Err(CoreError::Cancelled)
                    } else {
                        result
                    };
                    let (stage, code) = match result {
                        Ok(mut snapshot) => {
                            snapshot.id = id("snapshot");
                            s.snapshot = Some(Arc::new(snapshot));
                            s.stale = false;
                            ("ready", None)
                        }
                        Err(CoreError::Cancelled) => ("cancelled", Some("CANCELLED")),
                        Err(e) => ("failed", Some(core_error(e))),
                    };
                    let snapshot_id = s.snapshot.as_ref().map(|s| s.id.clone());
                    if let Some(code) = code.filter(|code| code.starts_with("LEG-")) {
                        s.failures.push(Diagnostic {
                            id: id("diagnostic"),
                            code: code.into(),
                            stage: "analysis".into(),
                            resource_id: None,
                            path: None,
                            start: None,
                            end: None,
                            reason: "job-failed".into(),
                        });
                    }
                    if let Some(job) = s.job.as_mut() {
                        job.sequence += 1;
                        job.terminal = true;
                        job.status = json!({"projectId":project,"jobId":job_id,"sequence":job.sequence,"stage":stage,"terminal":true,"error":code,"snapshotId":snapshot_id});
                        let _ = application.emit("analysis-progress", &job.status);
                    }
                }
            });
            Ok(json!({"jobId":response_job}))
        }
        "export_prepare" => {
            let (snapshot, path, selected, options, revision, selection_revision) = {
                let s = inner.lock().map_err(|_| "LEG-9001")?;
                let snapshot = s.snapshot(request)?;
                if s.running() || s.stale || s.storage.pending {
                    return Err("LEG-5001");
                }
                let selected = s
                    .selection
                    .iter()
                    .map(|p| {
                        snapshot
                            .resources
                            .iter()
                            .find(|r| &r.path == p)
                            .map(|r| r.id.clone())
                            .ok_or("MISSING_SELECTION")
                    })
                    .collect::<storage::Result<Vec<_>>>()?;
                (
                    snapshot,
                    s.project(request)?.1.clone(),
                    selected,
                    ExportOptions {
                        app_version: env!("CARGO_PKG_VERSION").into(),
                        max_bytes: s.storage.settings.max_bytes,
                    },
                    s.storage.settings.revision,
                    s.selection_revision,
                )
            };
            let input = FileInput::new(path).map_err(core_error)?;
            let changes =
                verify_inputs(&snapshot, &input, &Cancellation::default()).map_err(core_error)?;
            if !changes.is_empty() {
                let mut s = inner.lock().map_err(|_| "LEG-9001")?;
                s.stale = true;
                s.invalidate();
                return Err("LEG-5001");
            }
            let plan = plan_export(
                snapshot.clone(),
                &selected,
                &options,
                &Cancellation::default(),
            )
            .map_err(core_error)?;
            let mut s = inner.lock().map_err(|_| "LEG-9001")?;
            if s.running()
                || s.storage.settings.revision != revision
                || s.selection_revision != selection_revision
                || s.snapshot.as_ref().map(|s| &s.id) != Some(&snapshot.id)
            {
                return Err("STALE_SNAPSHOT");
            }
            let plan_id = id("plan");
            let output = json!({"planId":plan_id,"parts":plan.parts});
            s.plan = Some(Plan {
                id: plan_id,
                plan,
                settings_revision: revision,
                selection_revision,
            });
            Ok(output)
        }
        "export_save" => {
            let plan_id: String = arg(&request.args, "planId")?;
            {
                let s = inner.lock().map_err(|_| "LEG-9001")?;
                s.snapshot(request)?;
                validate_plan(&s, &plan_id)?;
            }
            let Some(path) = destination(app, "legacists.txt") else {
                return Ok(json!({"cancelled":true}));
            };
            let mut s = inner.lock().map_err(|_| "LEG-9001")?;
            s.snapshot(request)?;
            validate_plan(&s, &plan_id)?;
            let snapshot = s.snapshot(request)?;
            let input = FileInput::new(&s.project(request)?.1).map_err(core_error)?;
            if !verify_inputs(&snapshot, &input, &Cancellation::default())
                .map_err(core_error)?
                .is_empty()
            {
                s.stale = true;
                s.invalidate();
                return Err("LEG-5001");
            }
            let plan = &s.plan.as_ref().unwrap().plan;
            let contents: Vec<_> = plan
                .parts
                .iter()
                .map(|p| render_export(plan, p.index).unwrap().to_vec())
                .collect();
            Ok(json!(save_parts(&path, &contents, &s)?))
        }
        "diagnostic_report_save" => {
            let report = {
                let s = inner.lock().map_err(|_| "LEG-9001")?;
                if request.project_id.is_some() {
                    s.project(request)?;
                }
                s.report()
            };
            let Some(path) = destination(app, "legacists-errors.txt") else {
                return Ok(json!({"cancelled":true}));
            };
            let s = inner.lock().map_err(|_| "LEG-9001")?;
            if request.project_id.is_some() {
                s.project(request)?;
            }
            Ok(json!(save_parts(&path, &[report.into_bytes()], &s)?))
        }
        "data_location_change" => {
            let Some(path) = folder(app) else {
                return Ok(json!({"cancelled":true}));
            };
            let mut s = inner.lock().map_err(|_| "LEG-9001")?;
            if s.running() {
                return Err("BUSY");
            }
            s.storage.migrate(&path)?;
            s.invalidate();
            Ok(s.settings())
        }
        _ => {
            let mut s = inner.lock().map_err(|_| "LEG-9001")?;
            match request.operation.as_str() {
                "startup_check" => {
                    Ok(json!({"version":env!("CARGO_PKG_VERSION"),"settings":s.settings()}))
                }
                "project_status" => Ok(s.summary()),
                "settings_read" => Ok(s.settings()),
                "settings_update" => {
                    if s.running() {
                        return Err("BUSY");
                    }
                    let settings: Settings = arg(&request.args, "settings")?;
                    if settings.project != s.storage.settings.project
                        || settings.selected != s.storage.settings.selected
                    {
                        return Err("INVALID_REQUEST");
                    }
                    let revision: u64 = arg(&request.args, "revision")?;
                    let stale = settings.analysis != s.storage.settings.analysis;
                    s.storage.update(settings, revision)?;
                    s.stale |= stale;
                    s.invalidate();
                    Ok(s.settings())
                }
                "project_close" => {
                    s.project(request)?;
                    if let Some(j) = &s.job {
                        j.cancel.cancel();
                    }
                    s.project = None;
                    s.job = None;
                    s.snapshot = None;
                    s.invalidate();
                    Ok(json!({"closed":true}))
                }
                "analysis_cancel" => {
                    s.project(request)?;
                    let job_id: String = arg(&request.args, "jobId")?;
                    let job = s
                        .job
                        .as_ref()
                        .filter(|j| j.id == job_id)
                        .ok_or("INVALID_ID")?;
                    if !job.terminal {
                        job.cancel.cancel();
                    }
                    Ok(job.status.clone())
                }
                "analysis_status" => {
                    s.project(request)?;
                    let job_id: String = arg(&request.args, "jobId")?;
                    Ok(s.job
                        .as_ref()
                        .filter(|j| j.id == job_id)
                        .ok_or("INVALID_ID")?
                        .status
                        .clone())
                }
                "query" => {
                    let snapshot = s.snapshot(request)?;
                    let query: Query = arg(&request.args, "query")?;
                    let cursor: Option<String> = request
                        .args
                        .get("cursor")
                        .and_then(Value::as_str)
                        .map(String::from);
                    let limit = request
                        .args
                        .get("limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize;
                    let output = legacists_core::query(&snapshot, &query, cursor.as_deref(), limit)
                        .map_err(core_error)?;
                    serde_json::to_value(output).map_err(|_| "LEG-9001")
                }
                "selection_update" => {
                    let snapshot = s.snapshot(request)?;
                    let resource_id: String = arg(&request.args, "resourceId")?;
                    let selected: bool = arg(&request.args, "selected")?;
                    let path = snapshot
                        .resource(&resource_id)
                        .map_err(core_error)?
                        .path
                        .clone();
                    if selected {
                        s.selection.insert(path);
                    } else {
                        s.selection.remove(&path);
                    }
                    s.selection_revision += 1;
                    s.invalidate();
                    let mut settings = s.storage.settings.clone();
                    settings.selected = s.selection.iter().cloned().collect();
                    let revision = settings.revision;
                    s.storage.update(settings, revision)?;
                    Ok(s.summary())
                }
                "export_read" => {
                    s.snapshot(request)?;
                    let plan_id: String = arg(&request.args, "planId")?;
                    validate_plan(&s, &plan_id)?;
                    let part: usize = arg(&request.args, "part")?;
                    let plan = &s.plan.as_ref().unwrap().plan;
                    let text = std::str::from_utf8(render_export(plan, part).map_err(core_error)?)
                        .map_err(|_| "LEG-9001")?;
                    let offset = request
                        .args
                        .get("offset")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    let length = request
                        .args
                        .get("length")
                        .and_then(Value::as_u64)
                        .unwrap_or(65536)
                        .min(1048576) as usize;
                    if offset > text.len() || !text.is_char_boundary(offset) {
                        return Err("INVALID_RANGE");
                    }
                    let mut end = (offset + length).min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    Ok(json!({"text":&text[offset..end],"end":end,"totalBytes":text.len()}))
                }
                "diagnostics_report" => {
                    if request.project_id.is_some() {
                        s.project(request)?;
                    }
                    Ok(json!({"text":s.report()}))
                }
                "data_reset" => {
                    if s.running() {
                        return Err("BUSY");
                    }
                    s.storage.reset()?;
                    s.invalidate();
                    Ok(s.settings())
                }
                _ => Err("INVALID_REQUEST"),
            }
        }
    }
}

fn validate_plan(s: &Inner, id: &str) -> storage::Result<()> {
    let p = s.plan.as_ref().filter(|p| p.id == id).ok_or("INVALID_ID")?;
    if s.running()
        || s.stale
        || s.storage.pending
        || s.storage.settings.revision != p.settings_revision
        || s.selection_revision != p.selection_revision
        || s.snapshot.as_ref().map(|s| &s.id) != Some(&p.plan.snapshot.id)
    {
        return Err("STALE_SNAPSHOT");
    }
    Ok(())
}

fn save_parts(
    destination: &Path,
    contents: &[Vec<u8>],
    state: &Inner,
) -> storage::Result<crate::output::Outcome> {
    let parent = destination.parent().ok_or("LEG-5002")?;
    let parent = std::fs::canonicalize(parent).map_err(|_| "LEG-5002")?;
    if legacists_core::input::has_link_component(&parent)
        || storage::overlap(&parent, &state.storage.root)
    {
        return Err("LEG-5002");
    }
    let file = destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("LEG-5002")?;
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("LEG-5002")?;
    let targets: Vec<_> = (0..contents.len())
        .map(|i| {
            parent.join(if contents.len() == 1 {
                file.into()
            } else {
                format!("{stem}-{:06}.txt", i + 1)
            })
        })
        .collect();
    for path in &targets {
        if path.exists()
            || legacists_core::input::has_link_component(path)
            || state.project.as_ref().is_some_and(|(_, project)| {
                state.snapshot.as_ref().is_some_and(|s| {
                    s.resources.iter().any(|r| {
                        storage::normalized(&project.join(&r.path)) == storage::normalized(path)
                    })
                })
            })
        {
            return Err("LEG-5002");
        }
    }
    Ok(crate::output::save(&parent, targets, contents))
}

#[tauri::command]
pub async fn ipc(
    request: Request,
    state: tauri::State<'_, Application>,
    app: tauri::AppHandle,
) -> std::result::Result<Response, ()> {
    let inner = state.inner.clone();
    let request_id = request.request_id.clone();
    let project_id = request.project_id.clone();
    let snapshot_id = request.snapshot_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = execute(&request, inner.clone(), &app);
        let code = match &result {
            Err(code) => Some(*code),
            Ok(data) => data.get("code").and_then(Value::as_str),
        };
        if let Some(code) = code.filter(|c| c.starts_with("LEG-")) {
            if let Ok(mut s) = inner.lock() {
                if request.project_id.is_none()
                    || s.project.as_ref().map(|p| &p.0) == request.project_id.as_ref()
                {
                    s.failures.push(Diagnostic {
                        id: id("diagnostic"),
                        code: code.into(),
                        stage: "application".into(),
                        resource_id: None,
                        path: None,
                        start: None,
                        end: None,
                        reason: "operation-failed".into(),
                    });
                }
            }
        }
        result
    })
    .await
    .unwrap_or(Err("LEG-9001"));
    Ok(match result {
        Ok(data) => Response {
            request_id,
            project_id,
            snapshot_id,
            data: Some(data),
            error: None,
        },
        Err(code) => Response {
            request_id,
            project_id,
            snapshot_id,
            data: None,
            error: Some(json!({"code":code,"retryable":true})),
        },
    })
}
