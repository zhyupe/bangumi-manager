use crate::{copy, db::Store, model::*, paths};
use std::sync::{atomic::Ordering, Arc};

pub enum Event {
    Progress(Plan),
    Finished(Plan),
    Changed(QueueState),
    Error(String),
}
pub fn state(store: &Store) -> Result<QueueState> {
    Ok(store.get("queue", "main")?.unwrap_or_default())
}
fn persist(store: &Store, queue: &QueueState, plan: &Plan) -> Result<()> {
    let documents = [
        (
            "queue",
            "main",
            serde_json::to_string(queue).map_err(|e| e.to_string())?,
        ),
        (
            "plans",
            plan.id.as_str(),
            serde_json::to_string(plan).map_err(|e| e.to_string())?,
        ),
    ];
    store.diagnostics.database("db.queue_transaction", || {
    let mut conn = store.connection()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (collection, id, data) in documents {
        tx.execute("INSERT INTO documents(collection,id,data) VALUES(?1,?2,?3) ON CONFLICT(collection,id) DO UPDATE SET data=excluded.data",
            rusqlite::params![collection, id, data]).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
    })
}
pub fn enqueue(store: &Store, id: &str) -> Result<Plan> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let mut queue = state(store)?;
    let mut plan = store.get::<Plan>("plans", id)?.ok_or("任务不存在")?;
    if ["queued", "running"].contains(&plan.state.as_str()) {
        return Ok(plan);
    }
    copy::preflight(store, &plan)?;
    if store.list::<Plan>("plans")?.iter().any(|p| p.id != id && crate::renaming::in_progress(p)) {
        return Err("请先继续完成已有重命名任务，再启动其他文件操作".into());
    }
    if ["complete", "cancelled", "cancelling"].contains(&plan.state.as_str()) {
        return Err("任务已结束或正在取消".into());
    }
    // Reserve both source files and destinations across pending/running tasks.
    let inputs = crate::cue::input_paths(store, &plan)?;
    for other in store
        .list::<Plan>("plans")?
        .iter()
        .filter(|p| p.id != id && (["queued", "running", "cancelling"].contains(&p.state.as_str()) || crate::renaming::in_progress(p)
            || (p.kind == "cue_split" && !["complete", "cancelled"].contains(&p.state.as_str()))))
    {
        let other_inputs = crate::cue::input_paths(store, other)?;
        if !inputs.is_disjoint(&other_inputs)
            || plan.rows.iter().any(|r| r.action != "skip" && other_inputs.contains(&paths::key(std::path::Path::new(&r.target))))
            || other.rows.iter().any(|r| r.action != "skip" && inputs.contains(&paths::key(std::path::Path::new(&r.target)))) {
            return Err("An audio source or destination is reserved by another task".into());
        }
        for row in plan.rows.iter().filter(|r| r.action != "skip") {
            if other.rows.iter().filter(|r| r.action != "skip").any(|r| {
                paths::key(std::path::Path::new(&r.source))
                    == paths::key(std::path::Path::new(&row.source))
                    || paths::key(std::path::Path::new(&r.target))
                        == paths::key(std::path::Path::new(&row.target))
                    || (r.work_id == row.work_id && crate::renaming::in_progress(other))
                    || paths::key(std::path::Path::new(&r.target)) == paths::key(std::path::Path::new(&row.source))
                    || paths::key(std::path::Path::new(&r.source)) == paths::key(std::path::Path::new(&row.target))
                    || (r.work_id == row.work_id
                        && !r.extra
                        && !row.extra
                        && !r.attachment
                        && !row.attachment
                        && (r.part == 0 || row.part == 0 || r.part == row.part)
                        && r.episode_ids.iter().any(|id| row.episode_ids.contains(id)))
            }) {
                return Err(format!("文件或集数与队列中的其他任务重复：{}", row.source));
            }
        }
    }
    plan.state = "queued".into();
    if crate::renaming::in_progress(&plan) {
        queue.ids.insert(0, id.into());
    } else {
        queue.ids.push(id.into());
    }
    persist(store, &queue, &plan)?;
    Ok(plan)
}
pub fn set_paused(store: &Store, paused: bool) -> Result<QueueState> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let mut queue = state(store)?;
    if !paused && store.list::<Plan>("plans")?.iter().any(|p|
        crate::renaming::in_progress(p) && !queue.ids.contains(&p.id)) {
        return Err("请先在重命名任务中继续执行，再继续后续队列".into());
    }
    queue.paused = paused;
    queue.error.clear();
    store.put("queue", "main", &queue)?;
    if paused {
        let active = store.active_plan.lock().map_err(|e| e.to_string())?.clone();
        if let Some(id) = active {
            copy::pause_plan(store, &id)?;
        }
    }
    Ok(queue)
}
pub fn cancel(store: &Store, id: &str) -> Result<Plan> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let plan = copy::cancel_plan(store, id)?;
    let mut queue = state(store)?;
    queue.ids.retain(|candidate| candidate != id);
    persist(store, &queue, &plan)?;
    Ok(plan)
}
fn halt(store: &Store, message: String) -> Result<QueueState> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let mut queue = state(store)?;
    queue.paused = true;
    queue.error = message;
    store.put("queue", "main", &queue)?;
    Ok(queue)
}
// Empty-queue exit and worker reservation share the same lock, preventing lost wakeups.
pub fn reserve_worker(store: &Store) -> Result<bool> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let queue = state(store)?;
    if queue.paused || queue.ids.is_empty() || store.queue_worker.load(Ordering::SeqCst) {
        return Ok(false);
    }
    store.queue_worker.store(true, Ordering::SeqCst);
    Ok(true)
}
fn next(store: &Store) -> Result<Option<Plan>> {
    let _guard = store.queue_lock.lock().map_err(|e| e.to_string())?;
    let mut queue = state(store)?;
    loop {
        if queue.paused || queue.ids.is_empty() {
            store.queue_worker.store(false, Ordering::SeqCst);
            return Ok(None);
        }
        let id = queue.ids.remove(0);
        let Some(mut plan) = store.get::<Plan>("plans", &id)? else {
            store.put("queue", "main", &queue)?;
            continue;
        };
        if plan.state != "queued" {
            store.put("queue", "main", &queue)?;
            continue;
        }
        if let Err(error) = copy::start_plan(store, &id) {
            plan.state = "failed".into();
            plan.metadata_errors.push(error.clone());
            queue.paused = true;
            queue.error = error;
            persist(store, &queue, &plan)?;
            return Ok(Some(plan));
        }
        store.put("queue", "main", &queue)?;
        return store.get("plans", &id);
    }
}
pub fn drain(store: Arc<Store>, notify: impl Fn(Event)) {
    loop {
        let plan = match next(&store) {
            Ok(Some(plan)) => plan,
            Ok(None) => break,
            Err(error) => {
                // next() may have claimed the execution slot before persistence failed.
                let active = store.active_plan.lock().ok().and_then(|id| id.clone());
                if let Some(id) = active {
                    let _ = copy::finish_plan(&store, &id, Some(&error));
                }
                if let Ok(queue) = halt(&store, error.clone()) {
                    notify(Event::Changed(queue));
                }
                store.queue_worker.store(false, Ordering::SeqCst);
                notify(Event::Error(error));
                break;
            }
        };
        if let Ok(queue) = state(&store) {
            notify(Event::Changed(queue));
        }
        if plan.state == "failed" {
            notify(Event::Finished(plan));
            continue;
        }
        notify(Event::Progress(plan.clone()));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            copy::execute(store.clone(), plan.id.clone(), |p| {
                notify(Event::Progress(p.clone()))
            })
        }))
        .unwrap_or_else(|_| Err("后台任务异常终止，请检查任务后重试".into()));
        match copy::finish_plan(&store, &plan.id, result.as_ref().err().map(String::as_str)) {
            Ok(finished) => {
                if !["complete", "cancelled"].contains(&finished.state.as_str()) {
                    let reason = finished
                        .rows
                        .iter()
                        .find(|r| !r.error.is_empty())
                        .map(|r| r.error.clone())
                        .or_else(|| finished.metadata_errors.first().cloned())
                        .unwrap_or_else(|| "任务已暂停".into());
                    if let Ok(queue) = halt(&store, reason) {
                        notify(Event::Changed(queue));
                    }
                }
                notify(Event::Finished(finished));
            }
            Err(error) => {
                if let Ok(queue) = halt(&store, error.clone()) {
                    notify(Event::Changed(queue));
                }
                notify(Event::Error(error));
            }
        }
    }
}
