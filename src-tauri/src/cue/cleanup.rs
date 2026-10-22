use super::{context, input_paths, verify};
use crate::{copy, db::Store, model::*, paths, remote_transfer as remote, resources};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDisposition {
    #[default]
    Keep,
    Delete,
    Backup,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cleanup {
    pub disposition: SourceDisposition,
    pub started: bool,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    file: VerifiedFile,
    backup: String,
    intent: bool,
    done: bool,
}

pub fn configure_cleanup(
    store: &Store,
    id: &str,
    disposition: SourceDisposition,
    confirmed: bool,
) -> Result<()> {
    if store.get::<Plan>("plans", id)?.is_some() {
        return Ok(());
    }
    if disposition != SourceDisposition::Keep && !confirmed {
        return Err("errors.cue.originals.confirmation".into());
    }
    let mut ctx = context(store, id)?;
    let root = paths::normalize(&ctx.plan.backup_root)?;
    let mut seen = HashSet::new();
    let entries = std::iter::once(&ctx.analysis.cue)
        .chain(ctx.analysis.sources.iter().map(|s| &s.file))
        .filter(|f| seen.insert(paths::key(Path::new(&f.path))))
        .enumerate()
        .map(|(n, file)| Entry {
            file: file.clone(),
            backup: paths::display(&root.join(id).join("cue-originals").join(format!(
                "{n:03}-{}", Path::new(&file.path).file_name().unwrap_or_default().to_string_lossy()
            ))),
            intent: false,
            done: false,
        })
        .collect();
    ctx.cleanup = Cleanup {
        disposition,
        entries,
        started: false,
    };
    store.put("cue_tasks", id, &ctx)
}

pub(super) fn verify_outputs(store: &Store, plan: &Plan) -> Result<Vec<VerifiedFile>> {
    if plan.rows.is_empty()
        || plan
            .rows
            .iter()
            .any(|r| r.state != "complete" || r.action == "skip")
    {
        return Err("errors.cue.originals.incomplete".into());
    }
    plan.rows
        .iter()
        .map(|r| {
            let signature =
                resources::regular(Path::new(&r.target), Some(Path::new(&plan.music_root)))?;
            if r.digest.is_empty()
                || copy::measured_digest(store, Path::new(&r.target))? != r.digest
            {
                return Err("errors.cue.originals.outputChanged".into());
            }
            Ok(VerifiedFile {
                path: r.target.clone(),
                signature,
                digest: r.digest.clone(),
            })
        })
        .collect()
}

fn check_references(store: &Store, plan: &Plan, inputs: &HashSet<String>) -> Result<()> {
    for other in store.list::<Plan>("plans")? {
        if other.id != plan.id
            && !["complete", "cancelled"].contains(&other.state.as_str())
            && (!inputs.is_disjoint(&input_paths(store, &other)?)
                || other
                    .rows
                    .iter()
                    .any(|r| inputs.contains(&paths::key(Path::new(&r.target)))))
        {
            return Err("errors.cue.originals.shared".into());
        }
    }
    let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
    for asset in index.assets {
        if inputs.contains(&paths::key(Path::new(&asset.path))) {
            return Err("errors.cue.originals.registered".into());
        }
    }
    Ok(())
}

pub fn cleanup_in_progress(store: &Store, plan: &Plan) -> Result<bool> {
    if plan.kind != "cue_split" {
        return Ok(false);
    }
    let ctx = context(store, &plan.id)?;
    Ok(ctx.cleanup.started && ctx.cleanup.entries.iter().any(|e| !e.done))
}

pub fn finish_cleanup(store: &Store, plan: &mut Plan) -> Result<()> {
    if plan.kind != "cue_split"
        || plan.state != "complete"
        || !plan.metadata_errors.is_empty()
        || plan.rows.is_empty()
        || plan
            .rows
            .iter()
            .any(|r| r.state != "complete" || r.action == "skip")
    {
        return Ok(());
    }
    let mut ctx = context(store, &plan.id)?;
    if ctx.cleanup.disposition == SourceDisposition::Keep {
        return Ok(());
    }
    store.diagnostics.measure("cue.originals", serde_json::json!({"task_id":plan.id,"action":ctx.cleanup.disposition}), || {
        let _queue = store.queue_lock.lock().map_err(|e| e.to_string())?;
        let outputs = verify_outputs(store, plan)?;
        let inputs = input_paths(store, plan)?;
        check_references(store, plan, &inputs)?;
        // Check all inputs before the first irreversible operation.
        if !ctx.cleanup.started {
            for entry in &ctx.cleanup.entries { verify(store, &entry.file)?; }
        }
        plan.state = "running".into();
        copy::save(store, plan)?;
        ctx.cleanup.started = true;
        store.put("cue_tasks", &plan.id, &ctx)?;
        let root = paths::normalize(&plan.backup_root)?;
        for n in 0..ctx.cleanup.entries.len() {
            remote::check_cancel(&store.cancel)?;
            let entry = ctx.cleanup.entries[n].clone();
            if entry.done { continue; }
            let source = Path::new(&entry.file.path);
            let backup = Path::new(&entry.backup);
            paths::safe_target_entry(backup, &root)?;
            let source_exists = source.try_exists().map_err(|e| e.to_string())?;
            if !source_exists && !entry.intent {
                return Err("errors.cue.originals.missing".into());
            }
            if source_exists {
                if ctx.cleanup.disposition == SourceDisposition::Backup && !entry.intent
                    && (backup.try_exists().map_err(|e| e.to_string())?
                        || backup.with_extension("cue-partial").try_exists().map_err(|e| e.to_string())?) {
                    return Err("errors.cue.originals.occupied".into());
                }
                let locked = remote::LockedFile::open(source, true)?;
                verify(store, &entry.file)?;
                ctx.cleanup.entries[n].intent = true;
                store.put("cue_tasks", &plan.id, &ctx)?;
                if ctx.cleanup.disposition == SourceDisposition::Delete {
                    remote::check_cancel(&store.cancel)?;
                    locked.delete()?;
                } else {
                    fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
                    paths::safe_target_entry(backup, &root)?;
                    if backup.try_exists().map_err(|e| e.to_string())? {
                        if !entry.intent { return Err("errors.cue.originals.occupied".into()); }
                        verify_backup(store, backup, &entry.file, &root)?;
                        locked.delete()?;
                    } else {
                        match locked.rename(backup) {
                            Ok(()) => (),
                            Err(e) if matches!(e.raw_os_error(), Some(1 | 17 | 50 | 87)) => {
                                // Keep the source locked until the durable, verified copy is committed.
                                drop(locked);
                                let stage = backup.with_extension("cue-partial");
                                paths::safe_target_entry(&stage, &root)?;
                                if remote::pair(source, backup) {
                                    // An incomplete server-side copy is restartable; never relay SMB bytes.
                                    if stage.try_exists().map_err(|e| e.to_string())? {
                                        resources::regular(&stage, Some(&root))?;
                                        remote::LockedFile::open(&stage, true)?.delete()?;
                                    }
                                    let session = remote::prepare_copy(source, &stage, true, &entry.file.signature, None, true, &store.cancel, |_| Ok(()))?;
                                    verify(store, &entry.file)?;
                                    session.stage.rename(backup).map_err(|e| e.to_string())?;
                                    drop(session.stage);
                                    verify_backup(store, backup, &entry.file, &root)?;
                                    remote::check_cancel(&store.cancel)?;
                                    session.source.delete()?;
                                } else {
                                    let locked = remote::LockedFile::open(source, true)?;
                                    verify(store, &entry.file)?;
                                    if stage.try_exists().map_err(|e| e.to_string())? {
                                        resources::regular(&stage, Some(&root))?;
                                        remote::LockedFile::open(&stage, true)?.delete()?;
                                    }
                                    copy::stream_copy(store, source, &stage)?;
                                    verify_backup(store, &stage, &entry.file, &root)?;
                                    paths::commit_no_replace(&stage, backup)?;
                                    remote::check_cancel(&store.cancel)?;
                                    locked.delete()?;
                                }
                            }
                            Err(e) => return Err(serde_json::json!({"code":"errors.cue.originals.moveFailed","params":{"error":e.to_string()}}).to_string()),
                        }
                    }
                }
            }
            if ctx.cleanup.disposition == SourceDisposition::Backup {
                verify_backup(store, backup, &entry.file, &root)?;
                let id = format!("{}:cue-original:{n}", plan.id);
                store.put("backups", &id, &BackupRecord {
                    id: id.clone(), plan_id: plan.id.clone(), created_at: now(), root: paths::display(&root),
                    entry: BackupEntry { original: entry.file.path.clone(), backup: entry.backup.clone(), signature: entry.file.signature.clone(), digest: entry.file.digest.clone(), preserve_original: false, remote: None, probe: None },
                    signature: resources::regular(backup, Some(&root))?, replacements: outputs.clone(), complete: true, state: "available".into(), error: String::new(),
                })?;
            }
            ctx.cleanup.entries[n].done = true;
            store.put("cue_tasks", &plan.id, &ctx)?;
        }
        plan.state = "complete".into();
        copy::save(store, plan)
    })
}

fn verify_backup(store: &Store, path: &Path, original: &VerifiedFile, root: &Path) -> Result<()> {
    if resources::regular(path, Some(root))?.size != original.signature.size
        || copy::measured_digest(store, path)? != original.digest
    {
        return Err("errors.cue.originals.verification".into());
    }
    Ok(())
}
