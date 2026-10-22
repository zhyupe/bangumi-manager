mod app_data;
mod backups;
mod catalog;
mod copy;
mod db;
mod extras;
mod diagnostics;
mod file_manager;
mod library_import;
mod library_extras;
mod metadata;
mod model;
mod multipart;
mod musicbrainz;
mod cue;
mod naming;
mod nfo;
mod nfo_sync;
mod organize_preview;
mod paths;
mod providers;
#[cfg(windows)]
mod remote_transfer;
mod resources;
mod renaming;
mod scan;
mod subtitles;
mod task_queue;
mod tmdb_groups;

use db::Store;
use model::*;
use std::sync::{atomic::Ordering, Arc};
use tauri::{Emitter, Manager, State};
type AppState = Arc<Store>;

async fn blocking<T: Send + 'static>(
    diagnostics: diagnostics::Diagnostics,
    operation: &'static str,
    trace_id: Option<String>,
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let mut span = diagnostics.request(operation, trace_id);
    let context = span.context();
    let queued = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _scope = diagnostics::enter(Some(context));
        let queue_ms = queued.elapsed().as_secs_f64() * 1000.0;
        diagnostics.measure(
            "ipc.worker",
            serde_json::json!({"queue_ms":queue_ms,"command":operation}),
            f,
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    span.finish_result(&result);
    result
}
fn idle(store: &Store) -> Result<()> {
    if store.running.load(Ordering::SeqCst) {
        Err("复制任务执行中，请先暂停任务".into())
    } else if store.list::<Plan>("plans")?.iter().any(renaming::in_progress) {
        Err("请先继续完成重命名任务，再修改资料或扫描资料库".into())
    } else {
        Ok(())
    }
}
#[tauri::command]
async fn snapshot(state: State<'_, AppState>, trace_id: Option<String>) -> Result<Snapshot> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.snapshot",
        trace_id,
        move || {
            Ok(Snapshot {
                queue: task_queue::state(&s)?,
                settings: s.settings()?,
                works: s.list("works")?,
                sources: s.list("sources")?,
                index: s.get("index", "main")?.unwrap_or_default(),
                plans: s.list("plans")?,
                data_dir: paths::display(&s.data_dir),
            })
        },
    )
    .await
}
#[tauri::command]
async fn save_settings(
    state: State<'_, AppState>,
    settings: Settings,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.save_settings",
        trace_id,
        move || {
            idle(&state)?;
            paths::validate_settings(&settings)?;
            state.put("settings", "main", &settings)
        },
    )
    .await
}
#[tauri::command]
async fn scan_files(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    paths: Vec<String>,
    trace_id: Option<String>,
) -> Result<ScanResult> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.scan_files",
        trace_id,
        move || {
            scan::scan_sources(&s, paths, |n| {
                let _ = app.emit("scan-progress", n);
            })
        },
    )
    .await
}
#[tauri::command]
async fn clear_sources(state: State<'_, AppState>, trace_id: Option<String>) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.clear_sources",
        trace_id,
        move || state.clear("sources"),
    )
    .await
}
#[tauri::command]
async fn probe_file(
    state: State<'_, AppState>,
    path: String,
    trace_id: Option<String>,
) -> Result<Probe> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.probe_file",
        trace_id,
        move || scan::probe(&s, &path),
    )
    .await
}
#[tauri::command]
async fn list_work_extras(app: tauri::AppHandle, state: State<'_, AppState>, work_id: String, trace_id: Option<String>) -> Result<library_extras::ExtraFiles> {
    let store = state.inner().clone();
    let result = blocking(state.diagnostics.clone(), "ipc.list_work_extras", trace_id, move || library_extras::list(&store, &work_id)).await?;
    for file in &result.files {
        if matches!(paths::ext(std::path::Path::new(&file.path)).as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp") {
            app.asset_protocol_scope().allow_file(&file.path).map_err(|e| e.to_string())?;
        }
    }
    Ok(result)
}
#[tauri::command]
async fn list_resources(
    state: State<'_, AppState>,
    trace_id: Option<String>,
) -> Result<Vec<ResourceFile>> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.list_resources",
        trace_id,
        move || resources::list(&s),
    )
    .await
}
#[tauri::command]
async fn probe_resources(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    paths: Vec<String>,
    force: bool,
    trace_id: Option<String>,
) -> Result<()> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.probe_resources",
        trace_id,
        move || {
            resources::probe_many(s, paths, force, |done, total, path, probe| {
                let _ = app.emit(
                    "resource-probe-progress",
                    serde_json::json!({"done":done,"total":total,"path":path,"probe":probe}),
                );
            })
        },
    )
    .await
}
#[tauri::command]
fn cancel_resource_probe(state: State<'_, AppState>, trace_id: Option<String>) {
    let mut span = state
        .diagnostics
        .request("ipc.cancel_resource_probe", trace_id);
    let _scope = span.enter();
    let result = (|| {
        state.probe_cancel.store(true, Ordering::SeqCst);
    })();
    span.finish("ok");
    result
}
#[tauri::command]
async fn scan_replacements(
    state: State<'_, AppState>,
    paths: Vec<String>,
    trace_id: Option<String>,
) -> Result<CandidateScan> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.scan_replacements",
        trace_id,
        move || resources::candidates(&s, paths),
    )
    .await
}
#[tauri::command]
async fn preview_replacements(
    state: State<'_, AppState>,
    requests: Vec<ReplacementRequest>,
    trace_id: Option<String>,
) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.preview_replacements",
        trace_id,
        move || resources::preview(&s, requests),
    )
    .await
}
#[tauri::command]
async fn create_replacement_plan(
    state: State<'_, AppState>,
    id: String,
    reviewed: Vec<String>,
    trace_id: Option<String>,
) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.create_replacement_plan",
        trace_id,
        move || resources::create(&s, &id, reviewed),
    )
    .await
}
#[tauri::command]
async fn preview_renames(state: State<'_, AppState>, paths: Vec<String>, trace_id: Option<String>) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.preview_renames", trace_id, move || {
        idle(&s)?;
        renaming::preview(&s, paths)
    }).await
}
#[tauri::command]
async fn create_rename_plan(state: State<'_, AppState>, id: String, trace_id: Option<String>) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.create_rename_plan", trace_id, move || {
        idle(&s)?;
        renaming::create(&s, &id)
    }).await
}
#[tauri::command]
async fn list_backups(
    state: State<'_, AppState>,
    trace_id: Option<String>,
) -> Result<Vec<BackupRecord>> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.list_backups",
        trace_id,
        move || backups::list(&s),
    )
    .await
}
#[tauri::command]
async fn cleanup_backups(
    state: State<'_, AppState>,
    ids: Vec<String>,
    trace_id: Option<String>,
) -> Result<Vec<BackupRecord>> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.cleanup_backups",
        trace_id,
        move || backups::cleanup(&s, ids),
    )
    .await
}
#[tauri::command]
async fn search_media(
    state: State<'_, AppState>,
    provider: String,
    query: String,
    kind: String,
    trace_id: Option<String>,
) -> Result<Vec<RemoteSubject>> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.search_media",
        trace_id,
        move || providers::search(&s, &provider, &query, &kind),
    )
    .await
}
#[tauri::command]
async fn musicbrainz_releases(
    state: State<'_, AppState>, query: String, offset: u32, refresh: bool, trace_id: Option<String>,
) -> Result<musicbrainz::ReleasePage> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.musicbrainz_releases", trace_id,
        move || musicbrainz::releases(&s, &query, offset, refresh)).await
}
#[tauri::command]
async fn media_detail(
    state: State<'_, AppState>,
    provider: String,
    id: String,
    kind: String,
    refresh: bool,
    episode_group_id: Option<String>,
    trace_id: Option<String>,
) -> Result<RemoteSubject> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.media_detail",
        trace_id,
        move || match episode_group_id.filter(|g| !g.is_empty()) {
            Some(group) => {
                let (id, kind) = providers::parse_reference(&id, &kind)?;
                if provider != "tmdb" || kind != "tv" {
                    return Err("剧集组仅适用于 TMDB 剧集".into());
                }
                providers::detail_ordered(&s, &provider, &id, &kind, refresh, None, Some(&group))
            }
            None => providers::detail(&s, &provider, &id, &kind, refresh),
        },
    )
    .await
}
#[tauri::command]
async fn list_episode_groups(
    state: State<'_, AppState>,
    id: String,
    refresh: bool,
    trace_id: Option<String>,
) -> Result<Vec<EpisodeGroup>> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.list_episode_groups",
        trace_id,
        move || tmdb_groups::list(&s, &id, refresh),
    )
    .await
}
#[tauri::command]
async fn bind_subject(
    state: State<'_, AppState>,
    request: BindRequest,
    trace_id: Option<String>,
) -> Result<Work> {
    idle(&state)?;
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.bind_subject",
        trace_id,
        move || {
            let mut w = {
                let _queue = s.queue_lock.lock().map_err(|e| e.to_string())?;
                idle(&s)?;
                providers::bind(&s, request)?
            };
            metadata::cache_work(&s, &mut w);
            s.put("works", &w.id, &w)?;
            Ok(w)
        },
    )
    .await
}
#[tauri::command]
async fn sync_work(
    state: State<'_, AppState>,
    work_id: String,
    binding_id: Option<String>,
    season: Option<u32>,
    trace_id: Option<String>,
) -> Result<Work> {
    idle(&state)?;
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.sync_work",
        trace_id,
        move || providers::sync(&s, &work_id, binding_id.as_deref(), season),
    )
    .await
}
#[tauri::command]
async fn preview_naming(
    state: State<'_, AppState>,
    settings: Settings,
    trace_id: Option<String>,
) -> Result<serde_json::Value> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.preview_naming",
        trace_id,
        move || naming::previews(&settings),
    )
    .await
}
#[tauri::command]
async fn delete_work(
    state: State<'_, AppState>,
    work_id: String,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.delete_work",
        trace_id,
        move || catalog::delete_work(&state, &work_id),
    )
    .await
}
#[tauri::command]
async fn save_work(state: State<'_, AppState>, work: Work, trace_id: Option<String>) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.save_work",
        trace_id,
        move || {
            idle(&state)?;
            providers::validate_work(&work)?;
            let old = state
                .get::<Work>("works", &work.id)?
                .ok_or("本地条目不存在")?;
            if old.folder_name != work.folder_name || old.kind != work.kind {
                return Err(
                    "已有条目的目录和类型不可直接更改；请通过目录绑定功能关联已有目录".into(),
                );
            }
            if old.episodes.iter().any(|ep| work.episodes.iter().find(|e| e.id == ep.id)
                .is_some_and(|e| (e.season, e.number) != (ep.season, ep.number))) {
                tmdb_groups::save_reordered_work(&state, &work, "")?;
            } else {
                state.put("works", &work.id, &work)?;
            }
            metadata::refresh_existing(&state, &work)
        },
    )
    .await
}
#[tauri::command]
async fn refresh_library(
    state: State<'_, AppState>,
    trace_id: Option<String>,
) -> Result<LibraryIndex> {
    let s = state.inner().clone();
    idle(&s)?;
    blocking(
        state.diagnostics.clone(),
        "ipc.refresh_library",
        trace_id,
        move || scan::index_library(&s),
    )
    .await
}
#[tauri::command]
async fn bind_library_folder(
    state: State<'_, AppState>,
    path: String,
    work_id: String,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.bind_library_folder",
        trace_id,
        move || {
            idle(&state)?;
            scan::bind_folder(&state, &path, &work_id)
        },
    )
    .await
}
#[tauri::command]
async fn preview_organize(
    state: State<'_, AppState>, app: tauri::AppHandle, mappings: Vec<Mapping>, trace_id: Option<String>,
) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(state.diagnostics.clone(), "ipc.preview_organize", trace_id, move || {
        organize_preview::preview(&s, mappings, |n, total| {
            let _ = app.emit("plan-progress", serde_json::json!({"done":n,"total":total}));
        })
    }).await
}
#[tauri::command]
async fn submit_organize(
    state: State<'_, AppState>, id: String, choices: Vec<Decision>, trace_id: Option<String>,
) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(state.diagnostics.clone(), "ipc.submit_organize", trace_id, move || {
        let _queue = s.queue_lock.lock().map_err(|e| e.to_string())?;
        organize_preview::submit(&s, &id, choices)
    }).await
}
#[tauri::command]
async fn analyze_cue(state: State<'_, AppState>, request: cue::AnalyzeRequest, trace_id: Option<String>) -> Result<cue::Analysis> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.analyze_cue", trace_id, move || cue::analyze(&s, request)).await
}
#[tauri::command]
fn cancel_cue_analysis(state: State<'_, AppState>) {
    state.cue_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
}
#[tauri::command]
async fn inspect_cue(state: State<'_, AppState>, request: cue::AnalyzeRequest, trace_id: Option<String>) -> Result<cue::CueInspection> {
    blocking(state.diagnostics.clone(), "ipc.inspect_cue", trace_id, move || cue::inspect(&request)).await
}
#[tauri::command]
async fn cue_candidates(state: State<'_, AppState>, id: String, query: Option<String>, offset: u32, trace_id: Option<String>) -> Result<cue::Candidates> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.cue_candidates", trace_id, move || cue::candidates(&s, &id, query, offset)).await
}
#[tauri::command]
async fn preview_cue(state: State<'_, AppState>, request: cue::SplitRequest, trace_id: Option<String>) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.preview_cue", trace_id, move || cue::preview(&s, request)).await
}
#[tauri::command]
async fn submit_cue(state: State<'_, AppState>, id: String, choices: Vec<Decision>, source_disposition: Option<cue::SourceDisposition>, source_disposition_confirmed: Option<bool>, trace_id: Option<String>) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.submit_cue", trace_id, move || {
        let _queue = s.queue_lock.lock().map_err(|e|e.to_string())?;
        cue::configure_cleanup(&s, &id, source_disposition.unwrap_or_default(), source_disposition_confirmed.unwrap_or(false))?;
        cue::submit(&s, &id, choices)
    }).await
}
#[tauri::command]
async fn check_ffmpeg(state: State<'_, AppState>, path: String, trace_id: Option<String>) -> Result<String> {
    let s = state.inner().clone();
    blocking(s.diagnostics.clone(), "ipc.check_ffmpeg", trace_id, move || cue::check_ffmpeg(&s, &path)).await
}
#[tauri::command]
async fn discard_organize_preview(
    state: State<'_, AppState>, id: String, trace_id: Option<String>,
) -> Result<()> {
    let s = state.inner().clone();
    blocking(state.diagnostics.clone(), "ipc.discard_organize_preview", trace_id, move || {
        s.remove("organize_previews", &id)
    }).await
}
#[tauri::command]
async fn create_plan(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    mappings: Vec<Mapping>,
    trace_id: Option<String>,
) -> Result<Plan> {
    let s = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.create_plan",
        trace_id,
        move || {
            copy::create_plan(&s, mappings, |n, total| {
                let _ = app.emit("plan-progress", serde_json::json!({"done":n,"total":total}));
            })
        },
    )
    .await
}
#[tauri::command]
async fn save_decisions(
    state: State<'_, AppState>,
    id: String,
    choices: Vec<Decision>,
    trace_id: Option<String>,
) -> Result<Plan> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.save_decisions",
        trace_id,
        move || {
            let _queue = state.queue_lock.lock().map_err(|e| e.to_string())?;
            copy::decisions(&state, &id, choices)
        },
    )
    .await
}
fn pump_queue(store: Arc<Store>, app: tauri::AppHandle) -> Result<()> {
    if task_queue::reserve_worker(&store)? {
        let context = diagnostics::context();
        std::thread::spawn(move || {
            let _scope = diagnostics::enter(context);
            task_queue::drain(store, |event| match event {
                task_queue::Event::Progress(p) => {
                    let _ = app.emit("job-progress", p);
                }
                task_queue::Event::Finished(p) => {
                    let _ = app.emit("job-finished", p);
                }
                task_queue::Event::Changed(q) => {
                    let _ = app.emit("queue-changed", q);
                }
                task_queue::Event::Error(e) => {
                    let _ = app.emit("job-error", e);
                }
            })
        });
    }
    Ok(())
}
#[tauri::command]
async fn run_plan(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    trace_id: Option<String>,
) -> Result<Plan> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.run_plan",
        trace_id,
        move || {
            let p = task_queue::enqueue(&state, &id)?;
            let _ = app.emit("job-progress", &p);
            let _ = app.emit("queue-changed", task_queue::state(&state)?);
            pump_queue(state.clone(), app)?;
            Ok(p)
        },
    )
    .await
}
#[tauri::command]
async fn resume_queue(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.resume_queue",
        trace_id,
        move || {
            let q = task_queue::set_paused(&state, false)?;
            let _ = app.emit("queue-changed", q);
            pump_queue(state.clone(), app)
        },
    )
    .await
}
#[tauri::command]
async fn cancel_job(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.cancel_job",
        trace_id,
        move || {
            let _ = id;
            let q = task_queue::set_paused(&state, true)?;
            let _ = app.emit("queue-changed", q);
            Ok(())
        },
    )
    .await
}
#[tauri::command]
async fn cancel_plan(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    trace_id: Option<String>,
) -> Result<Plan> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.cancel_plan",
        trace_id,
        move || {
            let p = task_queue::cancel(&state, &id)?;
            let _ = app.emit("queue-changed", task_queue::state(&state)?);
            Ok(p)
        },
    )
    .await
}
#[tauri::command]
async fn delete_plan(
    state: State<'_, AppState>,
    id: String,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.delete_plan",
        trace_id,
        move || copy::delete_plan(&state, &id),
    )
    .await
}
#[tauri::command]
async fn delete_plans(
    state: State<'_, AppState>,
    ids: Vec<String>,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.delete_plans",
        trace_id,
        move || copy::delete_plans(&state, &ids),
    )
    .await
}
#[tauri::command]
async fn export_plan(
    state: State<'_, AppState>,
    id: String,
    path: String,
    trace_id: Option<String>,
) -> Result<()> {
    let state = state.inner().clone();
    blocking(
        state.diagnostics.clone(),
        "ipc.export_plan",
        trace_id,
        move || copy::export_plan(&state, &id, &path),
    )
    .await
}

#[tauri::command]
fn diagnostics_report(state: State<'_, AppState>) -> diagnostics::Report {
    state.diagnostics.report()
}
#[tauri::command]
async fn record_frontend_timing(
    state: State<'_, AppState>,
    operation: String,
    trace_id: String,
    elapsed_ms: f64,
    status: String,
) -> Result<()> {
    let diagnostics = state.diagnostics.clone();
    tauri::async_runtime::spawn_blocking(move || {
        diagnostics.frontend(&operation, trace_id, elapsed_ms, &status)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn open_in_file_manager(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    path: String,
    trace_id: Option<String>,
) -> Result<serde_json::Value> {
    use tauri_plugin_opener::OpenerExt;
    blocking(
        state.diagnostics.clone(),
        "ipc.open_in_file_manager",
        trace_id,
        move || {
            let target = file_manager::resolve(&path)?;
            if target.directory {
                app.opener().open_path(paths::display(&target.path), None::<&str>)
            } else {
                app.opener().reveal_item_in_dir(&target.path)
            }
            .map_err(|e| format!("无法打开文件管理器：{e}"))?;
            Ok(serde_json::json!({"path": paths::display(&target.path), "fallback": target.fallback}))
        },
    )
    .await
}

#[tauri::command]
async fn open_log_directory(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let path = state.diagnostics.directory().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        app.opener()
            .open_path(paths::display(&path), None::<&str>)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = match std::env::var("BANGUMI_MANAGER_DATA_DIR") {
                Ok(path) => paths::normalize(&path).map_err(std::io::Error::other)?,
                Err(_) => app_data::resolve(app.path().app_local_data_dir()?)?,
            };
            let store = Store::open(data_dir).map_err(std::io::Error::other)?;
            std::fs::create_dir_all(store.data_dir.join("artwork"))?;
            app.asset_protocol_scope()
                .allow_directory(store.data_dir.join("artwork"), true)?;
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            diagnostics_report,
            record_frontend_timing,
            open_log_directory,
            open_in_file_manager,
            snapshot,
            save_settings,
            scan_files,
            nfo_sync::inspect_source_nfos,
            nfo_sync::sync_source_nfos,
            clear_sources,
            probe_file,
            list_resources,
            list_work_extras,
            probe_resources,
            cancel_resource_probe,
            scan_replacements,
            preview_replacements,
            create_replacement_plan,
            preview_renames,
            create_rename_plan,
            list_backups,
            cleanup_backups,
            search_media,
            musicbrainz_releases,
            media_detail,
            list_episode_groups,
            bind_subject,
            sync_work,
            preview_naming,
            save_work,
            delete_work,
            refresh_library,
            bind_library_folder,
            create_plan,
            preview_organize,
            submit_organize,
            analyze_cue,
            cancel_cue_analysis,
            inspect_cue,
            cue_candidates,
            preview_cue,
            submit_cue,
            check_ffmpeg,
            discard_organize_preview,
            save_decisions,
            run_plan,
            resume_queue,
            cancel_job,
            cancel_plan,
            delete_plan,
            delete_plans,
            export_plan
        ])
        .run(tauri::generate_context!())
        .expect("Bangumi Manager could not start");
}
