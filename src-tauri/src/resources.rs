use crate::{copy, db::Store, model::*, paths, scan};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

pub fn list(store: &Store) -> Result<Vec<ResourceFile>> {
    let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
    let mut files = index.resources;
    // Older indexes remain useful until the first scan with this version.
    let mut seen: HashSet<_> = files
        .iter()
        .map(|r| paths::key(Path::new(&r.path)))
        .collect();
    for a in index.assets {
        let p = Path::new(&a.path);
        if seen.insert(paths::key(p)) {
            files.push(ResourceFile {
                unavailable: false,
                path: a.path.clone(),
                name: p.file_name().unwrap_or_default().to_string_lossy().into(),
                extension: paths::ext(p),
                kind: if paths::audio(p) { "music" } else { "tv" }.into(),
                signature: a.signature.clone(),
                asset: Some(a),
                probe: None,
                available: index.available,
            });
        }
    }
    for f in &mut files {
        let key = scan::probe_key(store, Path::new(&f.path), &f.signature)?;
        f.probe = store
            .get("probe_errors", &key)?
            .or(store.get("probes", &key)?)
            .or(f.probe.take().filter(|p| p.version == scan::PROBE_VERSION));
    }
    Ok(files)
}

pub fn probe_many(
    store: Arc<Store>,
    inputs: Vec<String>,
    force: bool,
    progress: impl Fn(usize, usize, &str, &Probe) + Sync,
) -> Result<()> {
    store.diagnostics.measure_with(
        "media.probe_batch",
        serde_json::json!({"files":inputs.len()}),
        |timing| {
            if store
                .probe_running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Err("已有媒体探测正在进行".into());
            }
            struct Reset<'a>(&'a Store);
            impl Drop for Reset<'_> {
                fn drop(&mut self) {
                    self.0.probe_running.store(false, Ordering::SeqCst);
                }
            }
            let _reset = Reset(&store);
            store.probe_cancel.store(false, Ordering::SeqCst);
            let mut seen = HashSet::new();
            let inputs: Vec<_> = inputs
                .into_iter()
                .filter(|p| seen.insert(paths::key(Path::new(p))))
                .collect();
            let total = inputs.len();
            let queue = Mutex::new(inputs.into_iter());
            let done = AtomicUsize::new(0);
            let errors = Mutex::new(Vec::new());
            std::thread::scope(|scope| {
                for _ in 0..2 {
                    let store = &store;
                    let queue = &queue;
                    let done = &done;
                    let progress = &progress;
                    let errors = &errors;
                    let context = crate::diagnostics::context();
                    scope.spawn(move || {
                        let _scope = crate::diagnostics::enter(context);
                        loop {
                            if store.probe_cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let Some(path) = queue.lock().unwrap().next() else {
                                break;
                            };
                            let before = paths::signature(Path::new(&path)).ok();
                            let p = scan::probe_controlled(
                                store,
                                &path,
                                force,
                                Some(&store.probe_cancel),
                            )
                            .unwrap_or_else(|e| Probe {
                                version: scan::PROBE_VERSION,
                                error: Some(e),
                                ..Default::default()
                            });
                            if !store.probe_cancel.load(Ordering::Relaxed) {
                                if let Err(e) = update_probed_resource(store, &path, before, &p) {
                                    errors.lock().unwrap().push(e);
                                }
                            }
                            progress(done.fetch_add(1, Ordering::SeqCst) + 1, total, &path, &p);
                        }
                    });
                }
            });
            if store.probe_cancel.load(Ordering::Relaxed) {
                timing.finish("cancelled");
            }
            let errors = errors.into_inner().map_err(|e| e.to_string())?;
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("；"))
            }
        },
    )
}

fn update_probed_resource(
    store: &Store,
    raw: &str,
    before: Option<Signature>,
    probe: &Probe,
) -> Result<()> {
    let _scan = store.library_scan.lock().map_err(|e| e.to_string())?;
    let mut index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
    let path = Path::new(raw);
    if let Some(file) = index
        .resources
        .iter_mut()
        .find(|f| paths::key(Path::new(&f.path)) == paths::key(path))
    {
        match paths::signature(path) {
            Ok(signature) if before.as_ref() == Some(&signature) => {
                // Refresh file facts but preserve the scanned association signature until the next scan.
                file.signature = signature;
                file.available = true;
                file.unavailable = false;
                file.probe = Some(probe.clone());
            }
            Err(_) => {
                file.available = path.parent().is_some_and(|p| fs::read_dir(p).is_ok());
                file.unavailable = file.available;
                file.probe = Some(probe.clone());
            }
            _ => return Ok(()),
        }
        store.put("index", "main", &index)?;
    }
    Ok(())
}

pub fn roots(s: &Settings) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for raw in [&s.movie_root, &s.tv_root, &s.music_root] {
        if let Ok(p) = paths::normalize(raw) {
            result.push(p);
        }
    }
    for kind in ["movie", "tv", "music"] {
        if let Ok(p) = crate::naming::root(s, kind) {
            result.push(p);
        }
    }
    result
}
pub fn root_for(s: &Settings, path: &Path) -> Result<PathBuf> {
    roots(s)
        .into_iter()
        .filter(|r| paths::contained(path, r))
        .max_by_key(|r| r.components().count())
        .ok_or("文件不在已注册资料库内".into())
}
pub fn is_remote_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        crate::remote_transfer::is_remote(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}
pub fn regular(path: &Path, root: Option<&Path>) -> Result<Signature> {
    if let Some(root) = root {
        paths::safe_target_entry(path, root)?;
    }
    for p in path.ancestors() {
        let m = fs::symlink_metadata(p).map_err(|e| format!("{}：{e}", paths::display(p)))?;
        if paths::is_link(&m) {
            return Err(format!("路径包含链接或重解析点：{}", paths::display(p)));
        }
    }
    paths::signature(path)
}
fn asset<'a>(index: &'a LibraryIndex, path: &Path) -> Result<&'a Asset> {
    index
        .assets
        .iter()
        .find(|a| paths::key(Path::new(&a.path)) == paths::key(path))
        .ok_or("资源尚未关联作品或集数，请先扫描并关联".into())
}
fn numbers(path: &Path) -> Option<(u32, u32, u32)> {
    let re =
        regex::Regex::new(r"(?i)(?:^|[^a-z0-9])S(\d{1,3})E(\d{1,4})(?:-E?(\d{1,4}))?(?:[^0-9]|$)")
            .ok()?;
    let name = path.file_stem()?.to_string_lossy();
    let c = re.captures(&name)?;
    let n = c[2].parse().ok()?;
    Some((
        c[1].parse().ok()?,
        n,
        c.get(3).and_then(|s| s.as_str().parse().ok()).unwrap_or(n),
    ))
}
pub fn candidates(store: &Store, inputs: Vec<String>) -> Result<CandidateScan> {
    store.diagnostics.measure(
        "replacement.scan",
        serde_json::json!({"files":inputs.len()}),
        || {
            let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
            let s = store.settings()?;
            let mut out = CandidateScan {
                candidates: vec![],
                errors: vec![],
            };
            let mut directories: HashMap<String, Vec<PathBuf>> = HashMap::new();
            let mut emitted = HashSet::new();
            for raw in inputs {
                let result = (|| -> Result<()> {
                    let old = paths::normalize(&raw)?;
                    let a = asset(&index, &old)?;
                    regular(&old, Some(&root_for(&s, &old)?))?;
                    let parent = old.parent().ok_or("没有父目录")?;
                    let files = if let Some(files) = directories.get(&paths::key(parent)) {
                        files.clone()
                    } else {
                        let files = fs::read_dir(parent)
                            .map_err(|e| e.to_string())?
                            .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
                            .collect::<Result<Vec<_>>>()?;
                        directories.insert(paths::key(parent), files.clone());
                        files
                    };
                    for new in files {
                        if paths::key(&new) == paths::key(&old)
                            || !paths::media(&new)
                            || paths::audio(&new) != paths::audio(&old)
                        {
                            continue;
                        }
                        let known = index
                            .assets
                            .iter()
                            .find(|b| paths::key(Path::new(&b.path)) == paths::key(&new));
                        if known.is_some_and(|b| {
                            b.work_id != a.work_id
                                || b.part != a.part
                                || (b.season, b.number, b.end) != (a.season, a.number, a.end)
                        }) {
                            continue;
                        }
                        let same_name = old
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(
                                &new.file_stem().unwrap_or_default().to_string_lossy(),
                            );
                        let exact_episode =
                            known.is_some() || (numbers(&new) == Some((a.season, a.number, a.end))
                                && crate::multipart::number(&new) == a.part);
                        if same_name || exact_episode {
                            if !emitted.insert((paths::key(&old), paths::key(&new))) {
                                continue;
                            }
                            if let Err(e) = regular(&new, Some(&root_for(&s, &new)?)) {
                                out.errors.push(e);
                                continue;
                            }
                            out.candidates.push(ReplacementCandidate {
                                old_path: paths::display(&old),
                                new_path: paths::display(&new),
                                reason: if same_name {
                                    "同目录主文件名一致"
                                } else {
                                    "同作品、同季集范围"
                                }
                                .into(),
                            });
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    out.errors.push(format!("{raw}：{e}"));
                }
            }
            out.candidates.sort_by(|a, b| {
                (&a.old_path, a.reason != "同目录主文件名一致", &a.new_path).cmp(&(
                    &b.old_path,
                    b.reason != "同目录主文件名一致",
                    &b.new_path,
                ))
            });
            Ok(out)
        },
    )
}

fn warnings(old: Option<&Probe>, new: Option<&Probe>) -> Vec<String> {
    let mut out = vec![];
    match (old, new) {
        (Some(a), Some(b)) if a.error.is_none() && b.error.is_none() => {
            if let (Some(x), Some(y)) = (a.duration, b.duration) {
                if (x - y).abs() > 5_f64.max(x * 0.01) {
                    out.push("新旧时长差异较大，请核对版本、完整性与字幕时间轴".into());
                }
            }
            if let (Some(aw), Some(ah), Some(bw), Some(bh)) = (a.width, a.height, b.width, b.height)
            {
                if bw < aw || bh < ah {
                    out.push("新文件分辨率下降，请核对画质".into());
                }
            }
            if b.audio.len() < a.audio.len() || b.subtitles.len() < a.subtitles.len() {
                out.push("新文件的音轨或字幕轨道减少".into());
            }
        }
        _ => out.push("媒体信息缺失或探测失败，请先刷新或人工核对文件".into()),
    }
    out
}
fn backup(store: &Store, path: &Path, preserve: bool) -> Result<BackupEntry> {
    Ok(BackupEntry {
        preserve_original: preserve,
        remote: None,
        original: paths::display(path),
        backup: String::new(),
        signature: regular(path, None)?,
        digest: String::new(),
        probe: if paths::media(path) {
            scan::cached_probe(store, &paths::display(path))?
        } else {
            None
        },
    })
}

pub fn preview(store: &Store, requests: Vec<ReplacementRequest>) -> Result<Plan> {
    store.diagnostics.measure(
        "replacement.preview",
        serde_json::json!({"files":requests.len()}),
        || {
            if requests.is_empty() {
                return Err("请选择替代文件".into());
            }
            let s = store.settings()?;
            paths::validate_settings(&s)?;
            if s.backup_root.is_empty() {
                return Err("请先设置旧版本备份目录".into());
            }
            let backup_root = paths::normalize(&s.backup_root)?;
            for root in roots(&s) {
                let overlap = |a: &Path, b: &Path| paths::contained(a, b) || paths::contained(b, a);
                if overlap(&root, &backup_root)
                    || paths::resolved(&root)
                        .ok()
                        .zip(paths::resolved(&backup_root).ok())
                        .is_some_and(|(a, b)| overlap(&a, &b))
                {
                    return Err("备份目录必须独立于所有已注册资料库（包括旧目录）".into());
                }
            }
            let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
            let mut snapshot = s.clone();
            snapshot.tmdb_token.clear();
            let mut p = Plan {
                kind: "replacement".into(),
                settings: Some(snapshot),
                id: id(),
                created_at: now(),
                state: "draft".into(),
                rows: vec![],
                works: vec![],
                tv_root: crate::naming::root(&s, "tv")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                movie_root: crate::naming::root(&s, "movie")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                music_root: crate::naming::root(&s, "music")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                extras_root: s.extras_root.clone(),
                backup_root: s.backup_root.clone(),
                verify_hash: s.verify_hash,
                metadata_errors: vec![],
                metadata_notices: vec![],
                cleanup_notices: vec![],
            };
            let mut used = HashSet::new();
            for request in requests {
                let old = paths::normalize(&request.old_path)?;
                let source = paths::normalize(&request.new_path)?;
                if paths::key(&old) == paths::key(&source) {
                    return Err("新旧文件不能是同一路径".into());
                }
                let a = asset(&index, &old)?.clone();
                let work = store
                    .get::<Work>("works", &a.work_id)?
                    .ok_or("作品不存在")?;
                if crate::tmdb_groups::requires_reorganization(store, &old, &work)? {
                    return Err(
                        "此文件仍使用切换剧集组前的季集路径，请先按新分组整理后再替换资源".into(),
                    );
                }

                let root = root_for(&s, &old)?;
                if regular(&old, Some(&root))? != a.signature {
                    return Err("旧文件已变化，请重新扫描资料库".into());
                }
                let signature = regular(&source, None)?;
                if signature.size == 0 {
                    return Err("替代文件为空".into());
                }
                if !paths::media(&source) || paths::audio(&source) != paths::audio(&old) {
                    return Err("新旧文件媒体类型不一致".into());
                }
                let known = index
                    .assets
                    .iter()
                    .find(|b| paths::key(Path::new(&b.path)) == paths::key(&source));
                if let Some(b) = known {
                    if b.work_id != a.work_id
                        || b.part != a.part
                        || (b.season, b.number, b.end) != (a.season, a.number, a.end)
                    {
                        return Err("替代资源关联的作品或集数范围不一致".into());
                    }
                }
                if numbers(&source).is_some_and(|n| n != (a.season, a.number, a.end)) {
                    return Err("替代文件季集范围不一致；合并集必须完整替换".into());
                }
                let source_part = known.map(|a| a.part).unwrap_or_else(|| crate::multipart::number(&source));
                let same_stem = old.file_stem().unwrap_or_default().to_string_lossy()
                    .eq_ignore_ascii_case(&source.file_stem().unwrap_or_default().to_string_lossy());
                if source_part > 0 && source_part != a.part && !(a.part == 0 && same_stem) {
                    return Err("替代文件的 Part 编号与原文件不一致".into());
                }
                if a.end > a.number && known.is_none() && numbers(&source).is_none() {
                    return Err(
                "合并集替代文件需具有明确且完整的季集范围，请先关联或在文件名中标明 SxxEyy-Ezz"
                    .into(),
            );
                }
                let work: Work = store.get("works", &a.work_id)?.ok_or("作品不存在")?;
                let mut episode_ids = Vec::new();
                for number in a.number..=a.end {
                    episode_ids.push(
                        work.episodes
                            .iter()
                            .find(|e| e.enabled && e.season == a.season && e.number == number)
                            .ok_or("请先启用并保存完整集数范围")?
                            .id
                            .clone(),
                    );
                }
                copy::episode_range(&work, &episode_ids)?;
                let target = old.with_extension(paths::ext(&source));
                paths::safe_target_entry(&target, &root)?;
                let adopt = paths::key(&source) == paths::key(&target);
                if !adopt
                    && paths::key(&target) != paths::key(&old)
                    && paths::optional_signature(&target)?.is_some()
                {
                    return Err(format!(
                        "目标被其他未选中文件占用：{}",
                        paths::display(&target)
                    ));
                }
                if !used.insert(paths::key(&old)) || !used.insert(paths::key(&source)) {
                    return Err("新旧文件在选择中重复或交叉，请分开处理".into());
                }
                let media_folder = old
                    .ancestors()
                    .skip(1)
                    .find(|folder| {
                        store
                            .get::<String>("folders", &paths::key(folder))
                            .ok()
                            .flatten()
                            .as_deref()
                            == Some(work.id.as_str())
                    })
                    .ok_or("无法确定作品实际目录，请重新扫描")?;
                let probe = scan::cached_probe(store, &paths::display(&source))?;
                let old_probe = scan::cached_probe(store, &paths::display(&old))?;
                let mut row = PlanRow {
                    part: a.part,
                    subtitle: None,
                    rename: None,
                    replacement: Some(ReplacementContext {
                        old_asset: a,
                        root: paths::display(&root),
                        adopt,
                    }),
                    remote: None,
                    media_folder: paths::display(media_folder),
                    id: id(),
                    source: paths::display(&source),
                    target: paths::display(&target),
                    work_id: work.id.clone(),
                    parent_id: None,
                    episode_ids,
                    extra: false,
                    attachment: false,
                    signature,
                    target_signature: if adopt {
                        None
                    } else {
                        paths::optional_signature(&target)?
                    },
                    warnings: warnings(old_probe.as_ref(), probe.as_ref()),
                    probe,
                    target_probe: old_probe,
                    conflicts: vec![],
                    reviewed: false,
                    action: "replace".into(),
                    operation: if adopt {
                        "adopt".into()
                    } else if root_for(&s, &source).is_ok() {
                        "move".into()
                    } else {
                        s.default_operation.clone()
                    },
                    source_cleanup_pending: false,
                    backups_prepared: false,
                    state: "pending".into(),
                    error: String::new(),
                    stage: String::new(),
                    digest: String::new(),
                    backups: vec![backup(store, &old, false)?],
                };
                // Metadata is backed up but retained at its shared path until the metadata writer commits.
                let mut nfos = vec![target.with_extension("nfo")];
                if work.kind != "movie" {
                    nfos.push(media_folder.join(if work.kind == "music" {
                        "album.nfo"
                    } else {
                        "tvshow.nfo"
                    }));
                }
                if work.kind == "tv" {
                    nfos.push(target.parent().unwrap().join("season.nfo"));
                }
                for nfo in nfos {
                    if paths::optional_signature(&nfo)?.is_some() {
                        row.backups.push(backup(store, &nfo, true)?);
                    }
                }
                let original_stem = source.file_stem().unwrap_or_default().to_string_lossy();
                let mut attachments = vec![];
                for path in copy::sibling_attachments(&source)?
                    .into_iter()
                    .filter(|p| paths::subtitle(p))
                {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let dest = target.with_file_name(format!(
                        "{}{}",
                        target.file_stem().unwrap_or_default().to_string_lossy(),
                        &name[original_stem.len()..]
                    ));
                    if paths::key(&path) == paths::key(&dest) {
                        continue;
                    }
                    if paths::optional_signature(&dest)?.is_some() && !request.replace_subtitles {
                        row.warnings.push(format!(
                            "保留原字幕，请核对时间轴：{}",
                            paths::display(&dest)
                        ));
                        continue;
                    }
                    let mapping = Mapping {
                        part: row.part,
                        source: paths::display(&path),
                        work_id: work.id.clone(),
                        episode_ids: row.episode_ids.clone(),
                        extra: false,
                    };
                    let mut sub = copy::new_row(
                        store,
                        &mapping,
                        &work,
                        dest,
                        true,
                        Some(row.id.clone()),
                        None,
                    )?;
                    sub.replacement = row.replacement.clone();
                    sub.replacement.as_mut().unwrap().adopt = false;
                    sub.action = "replace".into();
                    if sub.target_signature.is_some() {
                        sub.backups
                            .push(backup(store, Path::new(&sub.target), false)?);
                    }
                    sub.operation = if row.operation == "adopt" {
                        "copy".into()
                    } else {
                        row.operation.clone()
                    };
                    attachments.push(sub);
                }
                if copy::sibling_attachments(&old)?
                    .iter()
                    .any(|f| paths::subtitle(f))
                {
                    row.warnings
                        .push("原外置字幕将保留，请核对新版本时间轴".into());
                }
                p.rows.push(row);
                p.rows.extend(attachments);
                if !p.works.iter().any(|w| w.id == work.id) {
                    p.works.push(work);
                }
            }
            // Do not allow one row to move or back up any other row's input or output.
            for (i, row) in p.rows.iter().enumerate() {
                for (j, other) in p.rows.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    if paths::key(Path::new(&row.target)) == paths::key(Path::new(&other.source))
                        || paths::key(Path::new(&row.target))
                            == paths::key(Path::new(&other.target))
                        || row.backups.iter().any(|b| {
                            paths::key(Path::new(&b.original))
                                == paths::key(Path::new(&other.source))
                        })
                    {
                        return Err("替换操作的源、目标或附件路径交叉，请分开处理".into());
                    }
                }
            }
            for row in &mut p.rows {
                for (n, b) in row.backups.iter_mut().enumerate() {
                    b.backup = paths::display(
                        &paths::normalize(&p.backup_root)?
                            .join(&p.id)
                            .join(&row.id)
                            .join(format!(
                                "{n:03}-{}",
                                Path::new(&b.original)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )),
                    );
                }
            }
            store.put("replacement_previews", &p.id, &p)?;
            Ok(p)
        },
    )
}

pub fn create(store: &Store, id: &str, reviewed: Vec<String>) -> Result<Plan> {
    store.diagnostics.measure(
        "replacement.create",
        serde_json::json!({"task_id":id}),
        || {
            if let Some(p) = store.get("plans", id)? {
                return Ok(p);
            }
            let mut p: Plan = store
                .get("replacement_previews", id)?
                .ok_or("预览不存在，请重新生成")?;
            for row in &mut p.rows {
                row.reviewed = row.warnings.is_empty() || reviewed.contains(&row.id);
                if !row.reviewed {
                    return Err("请逐项核对警告".into());
                }
                if regular(Path::new(&row.source), None)? != row.signature {
                    return Err("新文件在预览后变化，请重新生成预览".into());
                }
                for b in &row.backups {
                    if regular(Path::new(&b.original), None)? != b.signature {
                        return Err("旧文件或附件在预览后变化".into());
                    }
                }
            }
            copy::preflight(store, &p)?;
            store.put("plans", id, &p)?;
            store.remove("replacement_previews", id)?;
            Ok(p)
        },
    )
}

pub fn validate_context(store: &Store, p: &Plan) -> Result<()> {
    if !p.rows.iter().any(|r| r.replacement.is_some()) {
        return Ok(());
    }
    let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
    let settings = store.settings()?;
    for row in p
        .rows
        .iter()
        .filter(|r| !r.attachment && r.action != "skip" && r.state != "complete")
    {
        let Some(context) = &row.replacement else {
            continue;
        };
        let old = &context.old_asset;
        if paths::key(&root_for(&settings, Path::new(&old.path))?)
            != paths::key(Path::new(&context.root))
        {
            return Err("原资源的资料库已变化，请重新预览".into());
        }
        if store
            .get::<String>("folders", &paths::key(Path::new(&row.media_folder)))?
            .as_deref()
            != Some(row.work_id.as_str())
        {
            return Err("原资源目录的作品关联已变化，请重新预览".into());
        }
        let matches = |a: &Asset| {
            a.work_id == old.work_id
                && a.part == old.part && row.part == old.part
                && (a.season, a.number, a.end) == (old.season, old.number, old.end)
        };
        let current = index
            .assets
            .iter()
            .find(|a| paths::key(Path::new(&a.path)) == paths::key(Path::new(&old.path)));
        if current.is_some_and(|a| !matches(a)) || (current.is_none() && !row.backups_prepared) {
            return Err("原资源关联已变化或尚未扫描，请重新预览".into());
        }
        if index.assets.iter().any(|a| {
            paths::key(Path::new(&a.path)) == paths::key(Path::new(&row.source)) && !matches(a)
        }) {
            return Err("替代文件的作品或集数关联已变化，请重新预览".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups;

    struct Fixture {
        _temp: tempfile::TempDir,
        store: Arc<Store>,
        folder: PathBuf,
        incoming: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let store = Store::open(temp.path().join("db")).unwrap();
            let s = Settings {
                tv_root: paths::display(&temp.path().join("tv")),
                movie_root: paths::display(&temp.path().join("movies")),
                music_root: paths::display(&temp.path().join("music")),
                extras_root: paths::display(&temp.path().join("extras")),
                backup_root: paths::display(&temp.path().join("backups")),
                ffprobe: "__not_installed__".into(),
                ..Default::default()
            };
            for root in [
                &s.tv_root,
                &s.movie_root,
                &s.music_root,
                &s.extras_root,
                &s.backup_root,
            ] {
                fs::create_dir_all(root).unwrap();
            }
            store.put("settings", "main", &s).unwrap();
            let work = Work {
                info: MediaInfo::default(),
                seasons: vec![],
                category: String::new(),
                metadata_errors: vec![],
                metadata_notices: vec![],
                id: "w".into(),
                title: "Show".into(),
                kind: "tv".into(),
                year: "2020".into(),
                folder_name: "Show".into(),
                tmdb_id: String::new(),
                parent_id: String::new(),
                overview: String::new(),
                poster: String::new(),
                bindings: vec![],
                episodes: (1..=3)
                    .map(|n| Episode {
                        info: MediaInfo::default(),
                        id: n.to_string(),
                        binding_id: String::new(),
                        remote_id: String::new(),
                        sort: n as f64,
                        title: format!("Episode {n}"),
                        kind: 0,
                        season: 1,
                        number: n,
                        enabled: true,
                    })
                    .collect(),
            };
            store.put("works", "w", &work).unwrap();
            let parent = Path::new(&s.tv_root).join("Show");
            let folder = parent.join("Season 01");
            fs::create_dir_all(&folder).unwrap();
            store.put("folders", &paths::key(&parent), &"w").unwrap();
            let incoming = temp.path().join("incoming");
            fs::create_dir_all(&incoming).unwrap();
            Self {
                _temp: temp,
                store,
                folder,
                incoming,
            }
        }
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let p = self.folder.join(name);
            fs::write(&p, bytes).unwrap();
            p
        }
        fn source(&self, name: &str) -> PathBuf {
            let p = self.incoming.join(name);
            fs::write(&p, b"new media content").unwrap();
            p
        }
        fn preview(&self, old: &Path, new: &Path, subs: bool) -> Plan {
            scan::index_library(&self.store).unwrap();
            preview(
                &self.store,
                vec![ReplacementRequest {
                    old_path: paths::display(old),
                    new_path: paths::display(new),
                    replace_subtitles: subs,
                }],
            )
            .unwrap()
        }
        fn create(&self, p: &Plan) -> Plan {
            create(
                &self.store,
                &p.id,
                p.rows.iter().map(|r| r.id.clone()).collect(),
            )
            .unwrap()
        }
        fn execute(&self, p: &Plan) -> Plan {
            copy::execute(self.store.clone(), p.id.clone(), |_| {}).unwrap()
        }
    }
    #[test]
    fn adopts_same_directory_file_preserves_other_versions_subtitles_and_shared_nfo() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new media");
        let other = f.file("alternate S01E01.mp4", b"keep version");
        let subtitle = f.file("S01E01.zh.ass", b"keep subtitle");
        let p = f.preview(&old, &new, false);
        assert_eq!(p.rows[0].operation, "adopt");
        assert!(p.rows[0]
            .backups
            .iter()
            .all(|b| b.original != paths::display(&new)));
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(
            p.state, "complete",
            "{:?} {:?}",
            p.metadata_errors, p.rows[0].error
        );
        assert!(!old.exists());
        assert_eq!(fs::read(new).unwrap(), b"new media");
        assert!(other.exists());
        assert_eq!(fs::read(subtitle).unwrap(), b"keep subtitle");
        assert!(f.folder.join("S01E01.nfo").exists());
        assert_eq!(fs::read(&p.rows[0].backups[0].backup).unwrap(), b"old");
    }
    #[test]
    fn existing_nfo_is_backed_up_and_shared_metadata_waits_for_all_files() {
        let f = Fixture::new();
        let first = f.file("S01E01.ts", b"old one");
        let second = f.file("S01E02.ts", b"old two");
        let new1 = f.file("S01E01.mkv", b"new one");
        let new2 = f.file("S01E02.mkv", b"new two");
        let nfo = f.folder.parent().unwrap().join("tvshow.nfo");
        let original = b"<tvshow><title>Show</title></tvshow>";
        fs::write(&nfo, original).unwrap();
        scan::index_library(&f.store).unwrap();
        let p = preview(
            &f.store,
            vec![(first, new1), (second, new2)]
                .into_iter()
                .map(|(a, b)| ReplacementRequest {
                    old_path: paths::display(&a),
                    new_path: paths::display(&b),
                    replace_subtitles: false,
                })
                .collect(),
        )
        .unwrap();
        let p = f.create(&p);
        let paused = copy::execute(f.store.clone(), p.id.clone(), |p| {
            if p.rows[0].state == "complete" {
                f.store.cancel.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(paused.state, "paused");
        assert_eq!(fs::read(&nfo).unwrap(), original);
        assert!(backups::list(&f.store).unwrap().iter().all(|b| !b.complete));
        f.store.cancel.store(false, Ordering::SeqCst);
        let p = f.execute(&paused);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert!(nfo.exists());
        for row in &p.rows {
            let backup = row
                .backups
                .iter()
                .find(|b| b.original == paths::display(&nfo))
                .unwrap();
            assert!(backup.preserve_original);
            assert_eq!(fs::read(&backup.backup).unwrap(), original);
        }
    }
    #[test]
    fn internal_candidate_moves_to_legacy_directory_after_naming_change() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("release S01E01.mkv", b"new");
        let mut settings = f.store.settings().unwrap();
        settings.output_root = paths::display(&f._temp.path().join("new-library"));
        f.store.put("settings", "main", &settings).unwrap();
        let p = f.preview(&old, &new, false);
        assert_eq!(p.rows[0].operation, "move");
        let p = f.create(&p);
        assert_eq!(f.execute(&p).state, "complete");
        assert!(!new.exists());
        assert_eq!(fs::read(old.with_extension("mkv")).unwrap(), b"new");
    }
    #[test]
    fn resumes_adoption_after_backup_commit_and_old_file_removal() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let mut p = f.create(&p);
        let b = &mut p.rows[0].backups[0];
        fs::create_dir_all(Path::new(&b.backup).parent().unwrap()).unwrap();
        fs::copy(&old, &b.backup).unwrap();
        b.digest = copy::digest(&old).unwrap();
        fs::remove_file(&old).unwrap();
        p.rows[0].backups_prepared = true;
        p.rows[0].state = "backing_up".into();
        p.state = "paused".into();
        f.store.put("plans", &p.id, &p).unwrap();
        assert_eq!(f.execute(&p).state, "complete");
        assert_eq!(fs::read(&new).unwrap(), b"new");
    }
    #[test]
    fn cleanup_recovers_deleted_file_but_retains_offline_deletion_intent() {
        for offline in [false, true] {
            let f = Fixture::new();
            let old = f.file("S01E01.ts", b"old");
            let new = f.file("S01E01.mkv", b"new");
            let p = f.preview(&old, &new, false);
            let p = f.create(&p);
            f.execute(&p);
            let mut b = backups::list(&f.store).unwrap().remove(0);
            b.state = "deleting".into();
            f.store.put("backups", &b.id, &b).unwrap();
            if offline {
                let dir = Path::new(&b.entry.backup).parent().unwrap();
                fs::rename(dir, dir.with_extension("offline")).unwrap();
                assert_eq!(backups::list(&f.store).unwrap()[0].state, "unavailable");
                assert_ne!(
                    backups::cleanup(&f.store, vec![b.id.clone()]).unwrap()[0].state,
                    "removed"
                );
                let persisted: BackupRecord = f.store.get("backups", &b.id).unwrap().unwrap();
                assert_eq!(persisted.state, "deleting");
            } else {
                fs::remove_file(&b.entry.backup).unwrap();
                assert_eq!(backups::list(&f.store).unwrap()[0].state, "removed");
            }
        }
    }
    #[test]
    fn cleanup_rejects_link_entries_and_directory_links() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        f.execute(&p);
        let mut b = backups::list(&f.store).unwrap().remove(0);
        let backup = PathBuf::from(&b.entry.backup);
        let saved = backup.with_extension("saved");
        fs::rename(&backup, &saved).unwrap();
        if paths::symlink_file(&saved, &backup).is_ok() {
            assert!(regular(&backup, Some(Path::new(&b.root))).is_err());
            assert_eq!(
                backups::cleanup(&f.store, vec![b.id.clone()]).unwrap()[0].state,
                "error"
            );
        }
        assert!(saved.exists());
        assert!(new.exists());
        let alias = Path::new(&b.root).join("alias");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // NTFS directory junctions do not need the symbolic-link privilege.
            let status = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&alias)
                .arg(&f.folder)
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(status.status.success(), "{:?}", status);
        }
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(&f.folder, &alias).unwrap();
        assert!(regular(&alias.join("S01E01.mkv"), None)
            .unwrap_err()
            .contains("链接"));
        b.entry.backup = paths::display(&alias.join("S01E01.mkv"));
        b.signature = paths::signature(&new).unwrap();
        f.store.put("backups", &b.id, &b).unwrap();
        assert_eq!(
            backups::cleanup(&f.store, vec![b.id]).unwrap()[0].state,
            "error"
        );
        fs::create_dir(f.folder.join("nested")).unwrap();
        fs::write(f.folder.join("nested/video.mkv"), b"media").unwrap();
        assert!(regular(&alias.join("nested/video.mkv"), Some(&alias.join("nested"))).is_err());
    }
    #[test]
    fn changed_association_prevents_execution_before_any_backup() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.source("new.mkv");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        let mut index: LibraryIndex = f.store.get("index", "main").unwrap().unwrap();
        index.assets[0].number = 2;
        f.store.put("index", "main", &index).unwrap();
        assert!(copy::execute(f.store.clone(), p.id, |_| {}).is_err());
        assert!(old.exists());
        assert!(new.exists());
    }
    #[test]
    fn indexes_root_files_and_audio_in_video_libraries_without_probing() {
        let f = Fixture::new();
        f.file("bonus.flac", b"audio");
        let root = PathBuf::from(f.store.settings().unwrap().tv_root);
        fs::write(root.join("unbound.ts"), b"video").unwrap();
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.resources.len(), 2);
        assert!(index
            .resources
            .iter()
            .all(|r| r.probe.is_none() && r.asset.is_none()));
        assert!(f.store.list::<Probe>("probes").unwrap().is_empty());
    }
    #[test]
    fn failed_probe_updates_file_facts_and_distinguishes_missing_files() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        scan::index_library(&f.store).unwrap();
        fs::write(&old, b"changed media").unwrap();
        probe_many(
            f.store.clone(),
            vec![paths::display(&old)],
            true,
            |_, _, _, _| {},
        )
        .unwrap();
        let file = list(&f.store).unwrap().remove(0);
        assert_eq!(file.signature.size, 13);
        assert_eq!(file.asset.unwrap().signature.size, 3);
        assert!(file.probe.unwrap().error.is_some());
        fs::remove_file(&old).unwrap();
        probe_many(
            f.store.clone(),
            vec![paths::display(&old)],
            true,
            |_, _, _, _| {},
        )
        .unwrap();
        let file = list(&f.store).unwrap().remove(0);
        assert!(file.available && file.unavailable);
        assert!(!f.store.probe_running.load(Ordering::SeqCst));
    }
    #[test]
    fn external_replacement_uses_old_name_and_only_selected_version() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let other = f.file("keep S01E01.mkv", b"keep");
        let new = f.source("release S01E01.mp4");
        let p = f.preview(&old, &new, false);
        assert_eq!(p.rows[0].target, paths::display(&old.with_extension("mp4")));
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(p.state, "complete");
        assert!(new.exists());
        assert!(other.exists());
        assert!(!old.exists());
    }
    #[test]
    fn same_extension_replacement_backs_up_video_and_preserves_old_subtitle_by_default() {
        let f = Fixture::new();
        let old = f.file("S01E01.mkv", b"old");
        let sub = f.file("S01E01.srt", b"old subtitle");
        let new = f.source("S01E01.mkv");
        fs::write(new.with_extension("srt"), b"new subtitle").unwrap();
        let p = f.preview(&old, &new, false);
        assert_eq!(p.rows.len(), 1);
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(p.state, "complete");
        assert_eq!(fs::read(sub).unwrap(), b"old subtitle");
        assert_eq!(fs::read(old).unwrap(), b"new media content");
    }
    #[test]
    fn explicitly_selected_subtitle_is_backed_up_and_replaced() {
        let f = Fixture::new();
        let old = f.file("S01E01.mkv", b"old");
        let sub = f.file("S01E01.srt", b"old subtitle");
        let new = f.source("S01E01.mkv");
        fs::write(new.with_extension("srt"), b"new subtitle").unwrap();
        let p = f.preview(&old, &new, true);
        assert_eq!(p.rows.len(), 2);
        assert_eq!(
            p.rows[1].backups.len(),
            1,
            "subtitle backup is visible before execution"
        );
        assert!(!p.rows[1].backups[0].backup.is_empty());
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(p.state, "complete");
        assert_eq!(fs::read(sub).unwrap(), b"new subtitle");
        assert_eq!(p.rows[1].backups.len(), 1);
    }
    #[test]
    fn adoption_with_new_subtitle_survives_task_decisions() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        let p = copy::decisions(
            &f.store,
            &p.id,
            p.rows
                .iter()
                .map(|r| Decision {
                    id: r.id.clone(),
                    action: r.action.clone(),
                    operation: r.operation.clone(),
                    reviewed: true,
                })
                .collect(),
        )
        .unwrap();
        assert_eq!(f.execute(&p).state, "complete");
    }
    #[test]
    fn movie_replacement_keeps_existing_folder_and_filename() {
        let f = Fixture::new();
        let mut w: Work = f.store.get("works", "w").unwrap().unwrap();
        w.kind = "movie".into();
        f.store.put("works", "w", &w).unwrap();
        let root = PathBuf::from(f.store.settings().unwrap().movie_root);
        let folder = root.join("Show");
        fs::create_dir_all(&folder).unwrap();
        f.store.put("folders", &paths::key(&folder), &"w").unwrap();
        let old = folder.join("Show.ts");
        fs::write(&old, b"old").unwrap();
        let new = f.source("new.mkv");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(p.state, "complete");
        assert!(folder.join("Show.mkv").exists());
    }
    #[test]
    fn replacement_candidates_and_execution_keep_parts_separate() {
        let f = Fixture::new();
        let first = f.file("S01E01 - part1.ts", b"first part");
        let second = f.file("S01E01 - part2.ts", b"second part");
        let candidate = f.file("S01E01 - part1.mkv", b"new first part");
        scan::index_library(&f.store).unwrap();
        let found = candidates(&f.store, vec![paths::display(&first)]).unwrap();
        assert_eq!(found.candidates.len(), 1);
        assert_eq!(found.candidates[0].new_path, paths::display(&candidate));
        assert!(preview(
            &f.store,
            vec![ReplacementRequest {
                old_path: paths::display(&first),
                new_path: paths::display(&second),
                replace_subtitles: false
            }]
        )
        .is_err());
        let p = f.preview(&first, &candidate, false);
        assert_eq!(p.rows[0].part, 1);
        let p = f.create(&p);
        assert_eq!(f.execute(&p).state, "complete");
        assert_eq!(fs::read(&second).unwrap(), b"second part");
        assert_eq!(fs::read(&candidate).unwrap(), b"new first part");
    }

    #[test]
    fn matches_exact_stems_or_episode_ranges_without_guessing_numbers() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        f.file("S01E01.mkv", b"new");
        f.file("other S01E01.mp4", b"other");
        f.file("01 1080p.mkv", b"no");
        f.file("S01E02.mkv", b"no");
        scan::index_library(&f.store).unwrap();
        let c = candidates(&f.store, vec![paths::display(&old)]).unwrap();
        assert_eq!(c.candidates.len(), 2);
    }
    #[test]
    fn merged_episode_partial_replacement_and_unrelated_target_are_rejected() {
        let f = Fixture::new();
        let old = f.file("S01E01-E02.ts", b"old");
        let new = f.source("S01E01.mkv");
        scan::index_library(&f.store).unwrap();
        let request = ReplacementRequest {
            old_path: paths::display(&old),
            new_path: paths::display(&new),
            replace_subtitles: false,
        };
        assert!(preview(&f.store, vec![request])
            .unwrap_err()
            .contains("范围"));
        let old = f.file("S01E03.ts", b"old");
        f.file("S01E03.mkv", b"keep");
        let new = f.source("S01E03.mkv");
        scan::index_library(&f.store).unwrap();
        assert!(preview(
            &f.store,
            vec![ReplacementRequest {
                old_path: paths::display(&old),
                new_path: paths::display(&new),
                replace_subtitles: false
            }]
        )
        .unwrap_err()
        .contains("占用"));
    }
    #[test]
    fn changed_source_after_preview_and_before_execution_is_rejected() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.source("new.mkv");
        let p = f.preview(&old, &new, false);
        fs::write(&new, b"changed").unwrap();
        assert!(create(
            &f.store,
            &p.id,
            p.rows.iter().map(|r| r.id.clone()).collect()
        )
        .is_err());
        assert!(old.exists());
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        fs::write(&new, b"changed again").unwrap();
        let p = f.execute(&p);
        assert_eq!(p.state, "failed");
        assert!(old.exists());
    }
    #[test]
    fn backup_failure_keeps_old_and_new_files_and_can_retry() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        let dest = PathBuf::from(&p.rows[0].backups[0].backup);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"unexpected").unwrap();
        let failed = f.execute(&p);
        assert_eq!(failed.state, "failed");
        assert!(old.exists());
        assert!(new.exists());
        fs::remove_file(dest).unwrap();
        assert_eq!(f.execute(&failed).state, "complete");
    }
    #[test]
    fn cancelled_adoption_resumes_from_verified_backups() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        f.store.cancel.store(true, Ordering::SeqCst);
        assert_eq!(f.execute(&p).state, "paused");
        assert!(old.exists());
        f.store.cancel.store(false, Ordering::SeqCst);
        assert_eq!(f.execute(&p).state, "complete");
    }
    #[test]
    fn backup_records_survive_task_deletion_and_cleanup_is_idempotent() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        let p = f.execute(&p);
        assert_eq!(p.state, "complete");
        copy::delete_plan(&f.store, &p.id).unwrap();
        let records = backups::list(&f.store).unwrap();
        assert!(!records.is_empty());
        assert!(records.iter().all(|r| r.state == "available"));
        let ids: Vec<_> = records.iter().map(|r| r.id.clone()).collect();
        assert!(backups::cleanup(&f.store, ids.clone())
            .unwrap()
            .iter()
            .all(|r| r.state == "removed"));
        assert!(backups::cleanup(&f.store, ids)
            .unwrap()
            .iter()
            .all(|r| r.state == "removed"));
        assert!(new.exists());
    }
    #[test]
    fn cleanup_refuses_changed_backup_changed_target_and_outside_path() {
        for mode in ["backup", "target", "outside"] {
            let f = Fixture::new();
            let old = f.file("S01E01.ts", b"old");
            let new = f.file("S01E01.mkv", b"new");
            let p = f.preview(&old, &new, false);
            let p = f.create(&p);
            f.execute(&p);
            let mut record = backups::list(&f.store).unwrap().remove(0);
            match mode {
                "backup" => fs::write(&record.entry.backup, b"modified backup").unwrap(),
                "target" => fs::write(&new, b"modified target").unwrap(),
                _ => {
                    record.entry.backup = paths::display(&new);
                    record.signature = paths::signature(&new).unwrap();
                    f.store.put("backups", &record.id, &record).unwrap();
                }
            }
            let result = backups::cleanup(&f.store, vec![record.id]).unwrap();
            assert_eq!(result[0].state, "error");
            assert!(Path::new(&record.entry.backup).exists());
            assert!(new.exists());
        }
    }
    #[test]
    fn failed_or_offline_replacements_keep_backups_protected() {
        let f = Fixture::new();
        let old = f.file("S01E01.ts", b"old");
        let new = f.file("S01E01.mkv", b"new");
        let p = f.preview(&old, &new, false);
        let p = f.create(&p);
        f.execute(&p);
        fs::rename(&new, new.with_extension("offline")).unwrap();
        let records = backups::list(&f.store).unwrap();
        assert!(records.iter().all(|r| r.state == "unavailable"));
    }
    #[test]
    fn unbound_resources_and_offline_index_are_retained() {
        let f = Fixture::new();
        f.file("unidentified.ts", b"file");
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.resources.len(), 1);
        assert!(index.resources[0].asset.is_none());
        let root = PathBuf::from(f.store.settings().unwrap().tv_root);
        fs::rename(&root, root.with_extension("offline")).unwrap();
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.resources.len(), 1);
        assert!(!index.resources[0].available);
    }
    #[test]
    fn probe_distinguishes_container_video_bitrate_and_tracks() {
        let p=scan::parse_probe(&serde_json::json!({"format":{"format_name":"mpegts","duration":"20"},"streams":[{"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,"profile":"Main 10","pix_fmt":"yuv420p10le","avg_frame_rate":"24000/1001","color_transfer":"smpte2084","bit_rate":"2000000"},{"codec_type":"audio","codec_name":"aac","channels":2,"tags":{"language":"jpn"}},{"codec_type":"subtitle","codec_name":"ass","tags":{"language":"zho"}}]}),10_000_000).unwrap();
        assert_eq!(p.bitrate, Some(4_000_000));
        assert!(p.bitrate_estimated);
        assert_eq!(p.video_bitrate, Some(2_000_000));
        assert_eq!(p.bit_depth, Some(10));
        assert_eq!(p.audio_tracks.len(), 1);
        assert_eq!(p.subtitles.len(), 1);
        assert!(p.hdr.is_some());
    }
    #[test]
    fn replacement_move_cannot_accept_old_target_when_source_disappears() {
        let f = Fixture::new();
        let old = f.file("S01E01.mkv", b"old");
        let new = f.source("new.mkv");
        fs::write(&new, b"new").unwrap();
        let mut settings = f.store.settings().unwrap();
        settings.default_operation = "move".into();
        f.store.put("settings", "main", &settings).unwrap();
        let p = f.preview(&old, &new, false);
        let mut p = f.create(&p);
        fs::remove_file(&new).unwrap();
        p.rows[0].state = "failed".into();
        f.store.put("plans", &p.id, &p).unwrap();
        assert_eq!(f.execute(&p).state, "failed");
        assert_eq!(fs::read(&old).unwrap(), b"old");
    }
    #[test]
    #[ignore = "Requires BANGUMI_TEST_FFPROBE pointing to a real ffprobe executable"]
    fn real_probe_supports_cache_batch_cancellation_and_retry() {
        let f = Fixture::new();
        let mut settings = f.store.settings().unwrap();
        settings.ffprobe = std::env::var("BANGUMI_TEST_FFPROBE").unwrap();
        f.store.put("settings", "main", &settings).unwrap();
        // One second of mono, 8 kHz, 16-bit PCM; no encoder dependency.
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(16036_u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(8000_u32.to_le_bytes());
        wav.extend(16000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(16000_u32.to_le_bytes());
        wav.resize(16044, 0);
        let files: Vec<_> = (0..10)
            .map(|n| paths::display(&f.file(&format!("audio{n}.wav"), &wav)))
            .collect();
        scan::index_library(&f.store).unwrap();
        let completed = AtomicUsize::new(0);
        probe_many(f.store.clone(), files.clone(), true, |_, _, _, p| {
            if p.error.is_none() {
                assert_eq!(p.audio_tracks[0].codec, "pcm_s16le");
                assert_eq!(p.duration, Some(1.0));
            }
            completed.fetch_add(1, Ordering::SeqCst);
            f.store.probe_cancel.store(true, Ordering::SeqCst);
        })
        .unwrap();
        assert!((1..=2).contains(&completed.load(Ordering::SeqCst)));
        assert!(!f.store.probe_running.load(Ordering::SeqCst));
        probe_many(f.store.clone(), files.clone(), false, |_, _, _, p| {
            assert!(p.error.is_none(), "{:?}", p.error)
        })
        .unwrap();
        assert_eq!(
            list(&f.store)
                .unwrap()
                .iter()
                .filter(|r| r.probe.as_ref().is_some_and(|p| p.error.is_none()))
                .count(),
            10
        );
        let cached = scan::cached_probe(&f.store, &files[0]).unwrap().unwrap();
        assert_eq!(cached.version, scan::PROBE_VERSION);
        assert_eq!(cached.container.as_deref(), Some("wav"));
    }
    #[test]
    fn warnings_require_review_but_allow_intentional_quality_tradeoffs() {
        let old = Probe {
            duration: Some(100.0),
            width: Some(1920),
            height: Some(1080),
            audio: vec!["aac".into()],
            ..Default::default()
        };
        let new = Probe {
            duration: Some(110.0),
            width: Some(1280),
            height: Some(720),
            ..Default::default()
        };
        assert_eq!(warnings(Some(&old), Some(&new)).len(), 3);
        let f = Fixture::new();
        let a = f.file("S01E01.ts", b"old");
        let b = f.source("new.mkv");
        let p = f.preview(&a, &b, false);
        assert!(create(&f.store, &p.id, vec![]).is_err());
        assert!(f.create(&p).rows[0].reviewed);
    }
}
