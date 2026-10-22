use crate::{copy, db::Store, model::*, paths, resources};
use std::{fs, path::Path};

// Backup records outlive plans. Never update the proofs of an already completed replacement.
pub fn register_plan(store: &Store, p: &Plan) -> Result<()> {
    let root = paths::normalize(&p.backup_root);
    for row in &p.rows {
        for (n, b) in row.backups.iter().enumerate() {
            if b.backup.is_empty()
                || (b.digest.is_empty() && !b.remote.as_ref().is_some_and(|r| r.ready))
            {
                continue;
            }
            let id = format!("{}:{}:{n}", p.id, row.id);
            let previous = store.get::<BackupRecord>("backups", &id)?;
            if previous
                .as_ref()
                .is_some_and(|r| r.complete || r.state == "removed")
            {
                continue;
            }
            let Ok(root) = &root else {
                continue;
            };
            let path = Path::new(&b.backup);
            let Ok(signature) = resources::regular(path, Some(root)) else {
                continue;
            };
            if signature.size != b.signature.size {
                continue;
            }
            if b.digest.is_empty()
                && b.remote
                    .as_ref()
                    .is_some_and(|r| r.sha256.is_empty() && !same_remote_file(path, &r.target))
            {
                // Legacy size-only checkpoints cannot authorize deletion of an unrecognized file.
                continue;
            }
            let mut replacements = vec![];
            if p.state == "complete" {
                for r in p
                    .rows
                    .iter()
                    .filter(|r| {
                        r.state == "complete" && (!r.attachment || r.subtitle.is_some()) && !r.extra
                    })
                {
                    let target = Path::new(&r.target);
                    if let Ok(signature) = resources::regular(target, None) {
                        let proven = !r.digest.is_empty()
                            || r.remote.as_ref().is_some_and(|proof| {
                                !proof.sha256.is_empty() || same_remote_file(target, &proof.target)
                            })
                            || (r.operation == "adopt" && signature == r.signature);
                        if signature.size == r.signature.size && proven {
                            replacements.push(VerifiedFile {
                                path: r.target.clone(),
                                signature,
                                digest: if !r.digest.is_empty() {
                                    r.digest.clone()
                                } else {
                                    r.remote
                                        .as_ref()
                                        .map(|v| v.sha256.clone())
                                        .unwrap_or_default()
                                },
                            });
                        }
                    }
                }
            }
            let complete = p.state == "complete"
                && !replacements.is_empty()
                && replacements.len()
                    == p.rows
                        .iter()
                        .filter(|r| {
                            r.state == "complete" && (!r.attachment || r.subtitle.is_some()) && !r.extra
                        })
                        .count();
            let record = BackupRecord {
                id: id.clone(),
                plan_id: p.id.clone(),
                created_at: previous.map(|r| r.created_at).unwrap_or_else(now),
                root: paths::display(root),
                entry: b.clone(),
                signature,
                replacements,
                complete,
                state: if complete { "available" } else { "protected" }.into(),
                error: String::new(),
            };
            store.put("backups", &id, &record)?;
        }
    }
    Ok(())
}
fn same_remote_file(path: &Path, stamp: &FileStamp) -> bool {
    #[cfg(windows)]
    {
        crate::remote_transfer::LockedFile::open(path, false)
            .and_then(|f| f.stamp())
            .is_ok_and(|s| s == *stamp)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, stamp);
        false
    }
}
fn validate(store: &Store, record: &BackupRecord, hashes: bool) -> Result<()> {
    if !record.complete || record.replacements.is_empty() {
        return Err("替换尚未完整成功，保留备份".into());
    }
    let root = paths::normalize(&record.root)?;
    let path = paths::normalize(&record.entry.backup)?;
    if resources::regular(&path, Some(&root))? != record.signature {
        return Err("备份文件已变化，拒绝清理".into());
    }
    let hash = if !record.entry.digest.is_empty() {
        &record.entry.digest
    } else {
        record
            .entry
            .remote
            .as_ref()
            .map(|r| &r.sha256)
            .ok_or("缺少备份校验记录")?
    };
    if hashes && !hash.is_empty() && copy::measured_digest(store, &path)? != *hash {
        return Err("备份内容校验失败，拒绝清理".into());
    }
    for file in &record.replacements {
        if paths::key(Path::new(&file.path)) == paths::key(&path) {
            return Err("备份路径与新资源重叠".into());
        }
        if resources::regular(Path::new(&file.path), None)? != file.signature {
            return Err("新资源已变化或不可用，保留备份".into());
        }
        if hashes
            && !file.digest.is_empty()
            && copy::measured_digest(store, Path::new(&file.path))? != file.digest
        {
            return Err("新资源内容校验失败，保留备份".into());
        }
    }
    Ok(())
}
pub fn list(store: &Store) -> Result<Vec<BackupRecord>> {
    store
        .diagnostics
        .measure("backup.list", serde_json::json!({}), || {
            for p in store.list::<Plan>("plans")? {
                register_plan(store, &p)?;
            }
            let mut records = store.list::<BackupRecord>("backups")?;
            for r in &mut records {
                if r.state == "removed" {
                    continue;
                }
                if deletion_finished(r) {
                    r.state = "removed".into();
                    r.error.clear();
                    store.put("backups", &r.id, r)?;
                    continue;
                }
                match validate(store, r, false) {
                    Ok(()) => {
                        if r.state != "error" {
                            r.state = "available".into();
                            r.error.clear();
                        }
                    }
                    Err(e) => {
                        r.state = if r.complete {
                            "unavailable"
                        } else {
                            "protected"
                        }
                        .into();
                        r.error = e;
                    }
                }
            }
            records.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
            Ok(records)
        })
}
fn deletion_finished(record: &BackupRecord) -> bool {
    if record.state != "deleting" {
        return false;
    }
    let path = Path::new(&record.entry.backup);
    // A missing file on an offline share is not evidence of successful deletion.
    path.parent()
        .is_some_and(|parent| fs::read_dir(parent).is_ok())
        && fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
pub fn cleanup(store: &Store, ids: Vec<String>) -> Result<Vec<BackupRecord>> {
    store.diagnostics.measure(
        "backup.cleanup",
        serde_json::json!({"files":ids.len()}),
        || {
            // Same ordering as queue operations: prevent a task from starting during cleanup.
            let _queue = store.queue_lock.lock().map_err(|e| e.to_string())?;
            let active = store.active_plan.lock().map_err(|e| e.to_string())?;
            if active.is_some() {
                return Err("请等待当前任务停止后清理备份".into());
            }
            let mut results = vec![];
            for id in ids {
                let mut record: BackupRecord =
                    store.get("backups", &id)?.ok_or("备份记录不存在")?;
                if record.state == "removed" {
                    results.push(record);
                    continue;
                }
                let result = (|| -> Result<()> {
                    let path = paths::normalize(&record.entry.backup)?;
                    #[cfg(windows)]
                    let target_locks = record
                        .replacements
                        .iter()
                        .map(|f| {
                            crate::remote_transfer::LockedFile::open(Path::new(&f.path), false)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    #[cfg(windows)]
                    let backup_lock = crate::remote_transfer::LockedFile::open(&path, true)?;
                    validate(store, &record, true)?;
                    // Persist intent before deletion so a lost reply is retryable without deleting a replacement file.
                    record.state = "deleting".into();
                    record.error.clear();
                    store.put("backups", &id, &record)?;
                    #[cfg(windows)]
                    {
                        backup_lock.delete()?;
                        drop(target_locks);
                    }
                    #[cfg(not(windows))]
                    fs::remove_file(&path).map_err(|e| e.to_string())?;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        record.state = "removed".into();
                        record.error.clear();
                    }
                    Err(e) => {
                        // Only a previously persisted deletion intent may acknowledge an already absent file.
                        if deletion_finished(&record) {
                            record.state = "removed".into();
                            record.error.clear();
                        } else {
                            if record.state != "deleting" {
                                record.state = "error".into();
                            }
                            record.error = e;
                        }
                    }
                }
                store.put("backups", &id, &record)?;
                results.push(record);
            }
            Ok(results)
        },
    )
}
