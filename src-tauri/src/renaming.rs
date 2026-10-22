use crate::{copy, db::Store, model::*, naming, paths, resources, scan, tmdb_groups};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

pub fn in_progress(p: &Plan) -> bool {
    p.kind == "rename"
        && p.state != "complete"
        && p.rows
            .iter()
            .any(|r| r.rename.as_ref().is_some_and(|c| c.phase != "pending"))
}

fn folder(store: &Store, source: &Path, root: &Path, work: &Work) -> Result<PathBuf> {
    for parent in source
        .ancestors()
        .skip(1)
        .take_while(|p| paths::contained(p, root))
    {
        if store
            .get::<String>("folders", &paths::key(parent))?
            .as_deref()
            == Some(&work.id)
        {
            return Ok(parent.to_path_buf());
        }
    }
    Err("资源目录尚未明确关联作品，请先扫描并关联".into())
}

fn destination(
    s: &Settings,
    work: &Work,
    m: &Mapping,
    source: &Path,
    folder: &Path,
    probe: Option<&Probe>,
) -> Result<PathBuf> {
    if !s.output_root.is_empty() || work.kind == "music" {
        let template = match work.kind.as_str() {
            "movie" => &s.naming.movie,
            "music" => &s.naming.music,
            _ => &s.naming.tv,
        };
        if probe.is_none()
            && ["videoFormat", "videoCodec", "audioCodec"]
                .iter()
                .any(|v| template.contains(v))
        {
            return Err(format!("命名规则需要媒体信息，请先探测：{}", m.source));
        }
        let relative = naming::render(
            s,
            work,
            &copy::episode_range(work, &m.episode_ids)?,
            source,
            probe,
        )?;
        Ok(crate::multipart::target(folder.join(relative.components().skip(1).collect::<PathBuf>()), m.part))
    } else {
        let target = copy::destination(s, work, m)?;
        let base = naming::root(s, &work.kind)?.join(&work.folder_name);
        Ok(folder.join(target.strip_prefix(base).map_err(|e| e.to_string())?))
    }
}

pub fn preview(store: &Store, inputs: Vec<String>) -> Result<Plan> {
    store.diagnostics.measure(
        "rename.preview",
        serde_json::json!({"files":inputs.len()}),
        || {
            if inputs.is_empty() {
                return Err("请选择已入库资源".into());
            }
            let _scan = store.library_scan.lock().map_err(|e| e.to_string())?;
            let s = store.settings()?;
            paths::validate_settings(&s)?;
            let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
            let mut settings = s.clone();
            settings.tmdb_token.clear();
            let mut p = Plan {
                kind: "rename".into(),
                settings: Some(settings),
                id: id(),
                created_at: now(),
                state: "draft".into(),
                rows: vec![],
                works: vec![],
                tv_root: naming::root(&s, "tv")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                movie_root: naming::root(&s, "movie")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                music_root: naming::root(&s, "music")
                    .map(|r| paths::display(&r))
                    .unwrap_or_default(),
                extras_root: s.extras_root.clone(),
                backup_root: s.backup_root.clone(),
                verify_hash: s.verify_hash,
                metadata_errors: vec![],
                metadata_notices: vec![],
                cleanup_notices: vec![],
            };
            let selected: HashSet<_> = inputs
                .iter()
                .map(|v| paths::normalize(v).map(|p| paths::key(&p)))
                .collect::<Result<_>>()?;
            let mut seen = HashSet::new();
            let mut backup_paths = HashSet::new();
            for raw in inputs {
                let source = paths::normalize(&raw)?;
                if !seen.insert(paths::key(&source)) {
                    continue;
                }
                let asset = index
                    .assets
                    .iter()
                    .find(|a| paths::key(Path::new(&a.path)) == paths::key(&source))
                    .ok_or("资源尚未关联作品和集数，请先扫描资料库")?
                    .clone();
                let work = store
                    .get::<Work>("works", &asset.work_id)?
                    .ok_or("作品不存在")?;
                let root = resources::root_for(&s, &source)?;
                if resources::regular(&source, Some(&root))? != asset.signature {
                    return Err(format!("资源已变化或不可用，请重新扫描：{raw}"));
                }
                let owner = folder(store, &source, &root, &work)?;
                let m = Mapping {
                    part: asset.part,
                    source: paths::display(&source),
                    work_id: work.id.clone(),
                    episode_ids: tmdb_groups::episode_ids(store, &asset, &work)?,
                    extra: false,
                };
                let probe = scan::cached_probe(store, &raw)?;
                let target = destination(&s, &work, &m, &source, &owner, probe.as_ref())?;
                paths::safe_target_entry(&target, &root)?;
                if paths::key(&source) == paths::key(&target) {
                    p.metadata_notices.push(format!("已符合当前命名：{raw}"));
                    continue;
                }
                let mut row = copy::new_row(store, &m, &work, target.clone(), false, None, probe)?;
                row.media_folder = paths::display(&owner);
                row.rename = Some(RenameContext {
                    root: paths::display(&root),
                    asset: Some(asset),
                    phase: "pending".into(),
                });
                row.operation = "rename".into();
                row.action = "copy".into();
                row.reviewed = true;
                row.target_signature = None;
                row.conflicts.clear();
                row.stage = paths::display(
                    &source.with_file_name(format!(".bm-rename-{}-{}.partial", p.id, row.id)),
                );
                let parent_id = row.id.clone();
                let mut attachments = vec![];
                for sidecar in copy::sibling_attachments(&source)? {
                    if ["tvshow.nfo", "season.nfo", "album.nfo"]
                        .iter()
                        .any(|name| {
                            sidecar
                                .file_name()
                                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                        })
                    {
                        continue;
                    }
                    if index.assets.iter().any(|a| {
                        let other = Path::new(&a.path);
                        paths::key(&other.with_extension(""))
                            == paths::key(&source.with_extension(""))
                            && !selected.contains(&paths::key(other))
                    }) {
                        return Err(format!(
                            "同名字幕或 NFO 被其他版本共用，请一并选择这些版本：{}",
                            paths::display(&sidecar)
                        ));
                    }
                    let original_stem = source.file_stem().unwrap().to_string_lossy();
                    let name = sidecar.file_name().unwrap().to_string_lossy();
                    let suffix = name
                        .get(original_stem.len()..)
                        .ok_or("附件文件名无法匹配")?;
                    let target_sub = target.with_file_name(format!(
                        "{}{suffix}",
                        target.file_stem().unwrap().to_string_lossy()
                    ));
                    if paths::key(&sidecar) == paths::key(&target_sub) {
                        continue;
                    }
                    resources::regular(&sidecar, Some(&root))?;
                    let mut mapping = m.clone();
                    mapping.source = paths::display(&sidecar);
                    let mut sub = copy::new_row(
                        store,
                        &mapping,
                        &work,
                        target_sub,
                        true,
                        Some(parent_id.clone()),
                        None,
                    )?;
                    sub.media_folder = row.media_folder.clone();
                    sub.operation = "rename".into();
                    sub.action = "copy".into();
                    sub.reviewed = true;
                    sub.target_signature = None;
                    sub.conflicts.clear();
                    sub.rename = Some(RenameContext {
                        root: paths::display(&root),
                        asset: None,
                        phase: "pending".into(),
                    });
                    sub.stage = paths::display(
                        &sidecar.with_file_name(format!(".bm-rename-{}-{}.partial", p.id, sub.id)),
                    );
                    if let Some(previous) = p
                        .rows
                        .iter()
                        .find(|r| paths::key(Path::new(&r.source)) == paths::key(&sidecar))
                    {
                        if paths::key(Path::new(&previous.target))
                            != paths::key(Path::new(&sub.target))
                        {
                            return Err("同一附件对应多个不同目标，请分别确认资源关联".into());
                        }
                    } else {
                        attachments.push(sub);
                    }
                }
                let mut nfos = vec![source.with_extension("nfo"), target.with_extension("nfo")];
                if work.kind != "movie" {
                    nfos.push(owner.join(if work.kind == "music" {
                        "album.nfo"
                    } else {
                        "tvshow.nfo"
                    }));
                }
                if work.kind == "tv" {
                    nfos.push(target.parent().unwrap().join("season.nfo"));
                }
                for nfo in nfos {
                    if paths::optional_signature(&nfo)?.is_some()
                        && backup_paths.insert(paths::key(&nfo))
                    {
                        row.backups.push(BackupEntry {
                            original: paths::display(&nfo),
                            backup: String::new(),
                            signature: resources::regular(&nfo, Some(&root))?,
                            digest: String::new(),
                            remote: None,
                            probe: None,
                            preserve_original: true,
                        });
                    }
                }
                p.rows.push(row);
                p.rows.extend(attachments);
                if !p.works.iter().any(|w| w.id == work.id) {
                    p.works.push(work);
                }
            }
            let sources: HashSet<_> = p
                .rows
                .iter()
                .map(|r| paths::key(Path::new(&r.source)))
                .collect();
            let mut targets = HashSet::new();
            for row in &mut p.rows {
                let target = Path::new(&row.target);
                if !targets.insert(paths::key(target)) {
                    row.conflicts
                        .push("多个文件生成相同名称，请调整命名规则或选择范围".into());
                }
                if paths::optional_signature(target)?.is_some()
                    && !sources.contains(&paths::key(target))
                {
                    row.conflicts
                        .push("目标已被未选中的文件占用，不能覆盖".into());
                }
                if !row.attachment {
                    let nfo = target.with_extension("nfo");
                    if paths::optional_signature(&nfo)?.is_some()
                        && !sources.contains(&paths::key(&nfo))
                    {
                        row.conflicts.push(format!(
                            "目标 NFO 已存在且未参与重命名：{}",
                            paths::display(&nfo)
                        ));
                    }
                }
            }
            if p.rows.iter().any(|r| !r.backups.is_empty()) {
                let root = paths::normalize(&p.backup_root)
                    .map_err(|_| "请先设置备份目录，用于保存重命名前的 NFO".to_string())?;
                for library in resources::roots(&s) {
                    let overlap =
                        |a: &Path, b: &Path| paths::contained(a, b) || paths::contained(b, a);
                    if overlap(&root, &library)
                        || paths::resolved(&root)
                            .ok()
                            .zip(paths::resolved(&library).ok())
                            .is_some_and(|(a, b)| overlap(&a, &b))
                    {
                        return Err("备份目录必须独立于所有资料库".into());
                    }
                }
                for row in &mut p.rows {
                    for (i, backup) in row.backups.iter_mut().enumerate() {
                        backup.backup =
                            paths::display(&root.join(&p.id).join(&row.id).join(format!(
                                "{i:03}-{}",
                                Path::new(&backup.original)
                                    .file_name()
                                    .unwrap()
                                    .to_string_lossy()
                            )));
                    }
                }
            }
            store.put("rename_previews", &p.id, &p)?;
            Ok(p)
        },
    )
}

pub fn create(store: &Store, id: &str) -> Result<Plan> {
    store
        .diagnostics
        .measure("rename.create", serde_json::json!({"task_id":id}), || {
            if let Some(p) = store.get::<Plan>("plans", id)? {
                return Ok(p);
            }
            let p: Plan = store
                .get("rename_previews", id)?
                .ok_or("预览不存在，请重新生成")?;
            if p.rows.is_empty() {
                return Err("所选资源已符合当前命名，无需重命名".into());
            }
            copy::preflight(store, &p)?;
            store.put("plans", id, &p)?;
            store.remove("rename_previews", id)?;
            Ok(p)
        })
}

pub fn validate_plan(p: &Plan) -> Result<()> {
    let mut sources = HashSet::new();
    let mut targets = HashSet::new();
    for row in &p.rows {
        if row.rename.is_none()
            || row.operation != "rename"
            || row.action != "copy"
            || !row.conflicts.is_empty()
        {
            return Err("重命名预览存在冲突或操作已更改，请重新预览".into());
        }
        if !sources.insert(paths::key(Path::new(&row.source)))
            || !targets.insert(paths::key(Path::new(&row.target)))
        {
            return Err("重命名路径重复".into());
        }
        let work = p
            .works
            .iter()
            .find(|w| w.id == row.work_id)
            .ok_or("缺少作品快照")?;
        copy::episode_range(work, &row.episode_ids)?;
    }
    Ok(())
}

pub fn validate_context(store: &Store, p: &Plan) -> Result<()> {
    if p.kind != "rename" {
        return Ok(());
    }
    let index: LibraryIndex = store.get("index", "main")?.unwrap_or_default();
    let settings = store.settings()?;
    let pending_sources: HashSet<_> = p
        .rows
        .iter()
        .filter(|r| r.rename.as_ref().is_some_and(|c| c.phase == "pending"))
        .map(|r| paths::key(Path::new(&r.source)))
        .collect();
    for row in &p.rows {
        let context = row.rename.as_ref().ok_or("缺少重命名上下文")?;
        let root = paths::normalize(&context.root)?;
        let source = paths::normalize(&row.source)?;
        let target = paths::normalize(&row.target)?;
        if paths::key(&resources::root_for(&settings, &source)?) != paths::key(&root) {
            return Err("资源所在资料库已改变，请重新预览".into());
        }
        paths::safe_target_entry(&source, &root)?;
        paths::safe_target_entry(&target, &root)?;
        paths::safe_target_entry(Path::new(&row.stage), &root)?;
        if store
            .get::<String>("folders", &paths::key(Path::new(&row.media_folder)))?
            .as_deref()
            != Some(&row.work_id)
        {
            return Err("资源目录的作品关联已变化".into());
        }
        if context.phase == "pending" {
            if resources::regular(&source, Some(&root))? != row.signature {
                return Err(format!("文件在预览后发生变化：{}", row.source));
            }
            if paths::optional_signature(Path::new(&row.stage))?.is_some() {
                return Err("重命名临时位置已被其他文件占用".into());
            }
            if let Some(old) = &context.asset {
                let current = index
                    .assets
                    .iter()
                    .find(|a| paths::key(Path::new(&a.path)) == paths::key(&source))
                    .ok_or("资源关联已失效，请重新扫描")?;
                if serde_json::to_value(current).unwrap() != serde_json::to_value(old).unwrap() {
                    return Err("资源关联在预览后发生变化".into());
                }
            }
            if paths::optional_signature(&target)?.is_some()
                && !pending_sources.contains(&paths::key(&target))
            {
                return Err(format!("目标已被其他文件占用：{}", row.target));
            }
            if !row.attachment {
                let nfo = target.with_extension("nfo");
                if paths::optional_signature(&nfo)?.is_some()
                    && !pending_sources.contains(&paths::key(&nfo))
                {
                    return Err(format!(
                        "目标 NFO 已被其他文件占用：{}",
                        paths::display(&nfo)
                    ));
                }
            }
        }
        for b in &row.backups {
            if b.digest.is_empty()
                && b.remote.is_none()
                && resources::regular(Path::new(&b.original), None)? != b.signature
            {
                return Err("NFO 在预览后发生变化，请重新预览".into());
            }
            if !b.digest.is_empty() {
                resources::regular(Path::new(&b.backup), Some(Path::new(&p.backup_root)))?;
                if copy::measured_digest(store, Path::new(&b.backup))? != b.digest {
                    return Err("NFO 备份发生变化，停止重命名".into());
                }
            }
            #[cfg(windows)]
            if let Some(proof) = b.remote.as_ref().filter(|r| r.ready) {
                let file = crate::remote_transfer::LockedFile::open(Path::new(&b.backup), false)?;
                if file.stamp()? != proof.target {
                    return Err("NFO 备份标识发生变化".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn same_file(a: &FileStamp, b: &FileStamp) -> bool {
    // Reusing a recently vacated Windows filename can change creation time.
    // File identity, content size and last-write time must still agree.
    a.identity == b.identity && a.size == b.size && a.modified == b.modified
}

fn verify(store: &Store, row: &PlanRow, path: &Path) -> Result<()> {
    resources::regular(path, Some(Path::new(&row.rename.as_ref().unwrap().root)))?;
    #[cfg(windows)]
    {
        let proof = row.remote.as_ref().ok_or("重命名检查点缺少文件标识")?;
        let file = crate::remote_transfer::LockedFile::open(path, false)?;
        if !same_file(&file.stamp()?, &proof.source) {
            return Err("文件标识或内容属性与重命名检查点不一致".into());
        }
    }
    if !row.digest.is_empty() && copy::measured_digest(store, path)? != row.digest {
        return Err("文件内容与重命名检查点不一致".into());
    }
    #[cfg(not(windows))]
    if row.digest.is_empty() {
        return Err("重命名检查点缺少内容校验".into());
    }
    Ok(())
}

fn move_file(store: &Store, row: &PlanRow, source: &Path, target: &Path) -> Result<()> {
    let root = Path::new(&row.rename.as_ref().unwrap().root);
    paths::safe_target_entry(target, root)?;
    verify(store, row, source)?;
    if store.cancel.load(Ordering::Relaxed) {
        return Err("重命名已暂停，文件和恢复记录已保留".into());
    }
    #[cfg(windows)]
    {
        let file = crate::remote_transfer::LockedFile::open(source, true)?;
        if !same_file(&file.stamp()?, &row.remote.as_ref().unwrap().source) {
            return Err("文件在加锁前发生变化".into());
        }
        file.rename(target)
            .map_err(|e| format!("无法重命名，文件和恢复记录已保留：{e}"))?;
    }
    #[cfg(not(windows))]
    paths::commit_no_replace(source, target)?;
    verify(store, row, target)
}

fn stage(store: &Store, p: &mut Plan, i: usize) -> Result<()> {
    let row = p.rows[i].clone();
    let phase = &row.rename.as_ref().unwrap().phase;
    if ["staged", "committing", "committed"].contains(&phase.as_str()) {
        return Ok(());
    }
    let source = Path::new(&row.source);
    let stage = Path::new(&row.stage);
    if phase == "pending" {
        if resources::regular(source, None)? != row.signature {
            return Err("源文件在执行前发生变化".into());
        }
        #[cfg(windows)]
        {
            let file = crate::remote_transfer::LockedFile::open(source, false)?;
            if paths::signature(source)? != row.signature {
                return Err("源文件在加锁前发生变化".into());
            }
            let stamp = file.stamp()?;
            p.rows[i].remote = Some(RemoteTransfer {
                method: "rename".into(),
                source: stamp.clone(),
                target: stamp,
                ready: true,
                sha256: String::new(),
            });
        }
        if p.verify_hash || !cfg!(windows) {
            p.rows[i].digest = copy::measured_digest(store, source)?;
        }
        if paths::signature(source)? != row.signature {
            return Err("源文件在校验期间发生变化".into());
        }
        p.rows[i].rename.as_mut().unwrap().phase = "staging".into();
        copy::save(store, p)?;
    }
    if paths::optional_signature(stage)?.is_some() {
        verify(store, &p.rows[i], stage)?;
        if paths::optional_signature(source)?.is_some() {
            return Err("源路径与临时路径同时存在，请保留文件并检查".into());
        }
    } else {
        move_file(store, &p.rows[i], source, stage)?;
    }
    p.rows[i].rename.as_mut().unwrap().phase = "staged".into();
    p.rows[i].state = "staged".into();
    p.rows[i].error.clear();
    copy::save(store, p)
}

fn commit(store: &Store, p: &mut Plan, i: usize) -> Result<()> {
    let row = p.rows[i].clone();
    let phase = &row.rename.as_ref().unwrap().phase;
    let target = Path::new(&row.target);
    if phase == "committed" {
        // Generated NFO is rewritten after the complete file batch has committed.
        if !row.attachment || paths::ext(target) != "nfo" {
            verify(store, &row, target)?;
        }
        p.rows[i].state = "complete".into();
        p.rows[i].error.clear();
        return copy::save(store, p);
    }
    if phase != "staged" && phase != "committing" {
        return Err("文件尚未安全暂存".into());
    }
    let stage = Path::new(&row.stage);
    paths::safe_target_entry(target, Path::new(&row.rename.as_ref().unwrap().root))?;
    fs::create_dir_all(target.parent().ok_or("无效目标路径")?).map_err(|e| e.to_string())?;
    if phase == "committing" && paths::optional_signature(stage)?.is_none() {
        verify(store, &row, target)?;
    } else {
        p.rows[i].rename.as_mut().unwrap().phase = "committing".into();
        copy::save(store, p)?;
        move_file(store, &p.rows[i], stage, target)?;
    }
    #[cfg(windows)]
    {
        let stamp = crate::remote_transfer::LockedFile::open(target, false)?.stamp()?;
        p.rows[i].remote.as_mut().unwrap().target = stamp;
    }
    p.rows[i].rename.as_mut().unwrap().phase = "committed".into();
    p.rows[i].state = "complete".into();
    p.rows[i].error.clear();
    copy::save(store, p)
}

pub fn transfer(store: &Store, p: &mut Plan, progress: &impl Fn(&Plan)) -> Result<()> {
    let _scan = store.library_scan.lock().map_err(|e| e.to_string())?;
    // Preserve NFO before vacating any paths. All sources are staged before committing
    // destinations, so swapped episode numbers never overwrite another episode.
    for pass in 0..3 {
        for i in 0..p.rows.len() {
            if store.cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let result = store.diagnostics.measure(
                "rename.file",
                serde_json::json!({
                    "task_id":p.id,"row_id":p.rows[i].id,"phase":pass
                }),
                || match pass {
                    0 if p.rows[i].backups.iter().any(|b| {
                        b.digest.is_empty() && !b.remote.as_ref().is_some_and(|r| r.ready)
                    }) =>
                    {
                        copy::backup_existing(store, p, i)
                    }
                    0 => Ok(()),
                    1 => stage(store, p, i),
                    _ => commit(store, p, i),
                },
            );
            if let Err(error) = result {
                p.rows[i].state = "failed".into();
                p.rows[i].error = error;
                copy::save(store, p)?;
                progress(p);
                return Ok(());
            }
            progress(p);
        }
    }
    // Remove old keys first: in a swap they may be another row's destination.
    for row in p.rows.iter().filter(|r| !r.attachment) {
        store.remove("group_asset_identity", &paths::key(Path::new(&row.source)))?;
        store.remove("asset_identity", &paths::key(Path::new(&row.source)))?;
        store.remove("sources", &paths::key(Path::new(&row.source)))?;
    }
    for row in p.rows.iter().filter(|r| !r.attachment) {
        tmdb_groups::register_completed(store, row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fixture {
        _temp: tempfile::TempDir,
        store: Arc<Store>,
        folder: PathBuf,
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
                id: "fictional".into(),
                title: "虚构重命名样例".into(),
                folder_name: "Archive".into(),
                kind: "tv".into(),
                episodes: (1..=3)
                    .map(|n| Episode {
                        id: n.to_string(),
                        binding_id: "binding".into(),
                        remote_id: n.to_string(),
                        season: 1,
                        number: n,
                        sort: n as f64,
                        enabled: true,
                        title: format!("章节 {n}"),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            };
            store.put("works", &work.id, &work).unwrap();
            let folder = Path::new(&s.tv_root).join("Archive");
            fs::create_dir_all(folder.join("Season 01")).unwrap();
            store
                .put("folders", &paths::key(&folder), &work.id)
                .unwrap();
            Self {
                _temp: temp,
                store,
                folder,
            }
        }
        fn file(&self, name: &str, number: u32, end: u32, data: &[u8]) -> PathBuf {
            let path = self.folder.join("Season 01").join(name);
            fs::write(&path, data).unwrap();
            let mut index: LibraryIndex =
                self.store.get("index", "main").unwrap().unwrap_or_default();
            index.available = true;
            index.assets.push(Asset {
                part: crate::multipart::number(&path),
                path: paths::display(&path),
                work_id: "fictional".into(),
                season: 1,
                number,
                end,
                signature: paths::signature(&path).unwrap(),
            });
            self.store.put("index", "main", &index).unwrap();
            path
        }
        fn reorder(&self, numbers: &[(u32, u32)]) {
            let mut work: Work = self.store.get("works", "fictional").unwrap().unwrap();
            for (ep, (season, number)) in work.episodes.iter_mut().zip(numbers) {
                ep.season = *season;
                ep.number = *number;
            }
            tmdb_groups::save_reordered_work(&self.store, &work, "").unwrap();
        }
        fn preview(&self, paths: &[&Path]) -> Plan {
            preview(
                &self.store,
                paths.iter().map(|p| paths::display(p)).collect(),
            )
            .unwrap()
        }
        fn run(&self, plan: &Plan) -> Plan {
            copy::execute(self.store.clone(), plan.id.clone(), |_| {}).unwrap()
        }
    }

    #[test]
    fn renumbering_keeps_distinct_parts_and_their_language_subtitles() {
        let f = Fixture::new();
        let first = f.file("Old.S01E01 - part1.mkv", 1, 1, b"first part");
        let second = f.file("Old.S01E01 - part2.mkv", 1, 1, b"second part");
        fs::write(second.with_extension("chs.ass"), b"second captions").unwrap();
        f.reorder(&[(2, 4), (1, 2), (1, 3)]);
        let p = f.preview(&[&first, &second]);
        assert_eq!(p.rows.iter().filter(|r| !r.attachment).count(), 2);
        let p = create(&f.store, &p.id).unwrap();
        let done = f.run(&p);
        assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
        for part in 1..=2 {
            let target = f.folder.join(format!("Season 02/S02E04 - part{part}.mkv"));
            assert!(target.exists());
        }
        assert_eq!(
            fs::read(f.folder.join("Season 02/S02E04 - part2.chs.ass")).unwrap(),
            b"second captions"
        );
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.assets.len(), 2);
        assert!(index
            .assets
            .iter()
            .all(|a| a.season == 2 && a.number == 4 && a.part > 0));
    }

    #[test]
    fn renamed_group_moves_subtitles_updates_nfo_and_keeps_backups() {
        let f = Fixture::new();
        let old = f.file("S01E01.mkv", 1, 1, b"original media");
        let subtitle = old.with_file_name("S01E01.zh.srt");
        fs::write(&subtitle, b"original subtitle").unwrap();
        let nfo = old.with_extension("nfo");
        let old_nfo = b"<episodedetails><season>1</season><episode>1</episode></episodedetails>";
        fs::write(&nfo, old_nfo).unwrap();
        fs::write(
            f.folder.join("tvshow.nfo"),
            b"<tvshow><title>Archive</title></tvshow>",
        )
        .unwrap();
        f.reorder(&[(3, 5), (1, 2), (1, 3)]);
        let p = f.preview(&[&old]);
        assert_eq!(p.rows.len(), 3);
        assert_eq!(p.rows[0].episode_ids, vec!["1"]);
        assert!(old.exists() && subtitle.exists() && nfo.exists());
        let p = create(&f.store, &p.id).unwrap();
        assert!(old.exists());
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.metadata_errors);
        let target = f.folder.join("Season 03/S03E05.mkv");
        assert_eq!(fs::read(&target).unwrap(), b"original media");
        assert_eq!(
            fs::read(target.with_file_name("S03E05.zh.srt")).unwrap(),
            b"original subtitle"
        );
        assert!(!old.exists() && !subtitle.exists() && !nfo.exists());
        assert!(f.folder.join("tvshow.nfo").exists());
        let generated = fs::read_to_string(target.with_extension("nfo")).unwrap();
        assert!(
            generated.contains("<season>3</season>") && generated.contains("<episode>5</episode>")
        );
        let b = p.rows[0]
            .backups
            .iter()
            .find(|b| b.original == paths::display(&nfo))
            .unwrap();
        assert_eq!(fs::read(&b.backup).unwrap(), old_nfo);
        let work: Work = f.store.get("works", "fictional").unwrap().unwrap();
        assert!(!tmdb_groups::requires_reorganization(&f.store, &target, &work).unwrap());
        let index = scan::index_library(&f.store).unwrap();
        assert!(
            index
                .assets
                .iter()
                .any(|a| paths::key(Path::new(&a.path)) == paths::key(&target)
                    && (a.season, a.number) == (3, 5)),
            "{index:?}"
        );
        assert!(crate::backups::list(&f.store)
            .unwrap()
            .iter()
            .all(|b| b.complete));
    }

    #[test]
    fn swaps_episode_paths_without_overwriting_either_file() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"first");
        let b = f.file("S01E02.mkv", 2, 2, b"other");
        for path in [&a, &b] {
            fs::write(path.with_extension("nfo"), b"<episodedetails/>").unwrap();
        }
        f.reorder(&[(1, 2), (1, 1), (1, 3)]);
        let p = f.preview(&[&a, &b]);
        assert!(p.rows.iter().all(|r| r.conflicts.is_empty()));
        let p = create(&f.store, &p.id).unwrap();
        let p = f.run(&p);
        assert_eq!(
            p.state,
            "complete",
            "{:?} {:?}",
            p.metadata_errors,
            p.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
        );
        assert_eq!(fs::read(&a).unwrap(), b"other");
        assert_eq!(fs::read(&b).unwrap(), b"first");
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.assets.len(), 2);
        assert!(
            index.assets.iter().any(|a| a.number == 1)
                && index.assets.iter().any(|a| a.number == 2)
        );
    }

    #[test]
    fn no_op_and_conflicting_unselected_version_never_modify_files() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"first");
        let b = f.file("S01E02.mkv", 2, 2, b"other");
        let no_op = f.preview(&[&a]);
        assert!(no_op.rows.is_empty());
        assert!(create(&f.store, &no_op.id).is_err());
        f.reorder(&[(1, 2), (1, 1), (1, 3)]);
        let conflict = f.preview(&[&a]);
        assert!(!conflict.rows[0].conflicts.is_empty());
        assert!(create(&f.store, &conflict.id).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"first");
        assert_eq!(fs::read(&b).unwrap(), b"other");
    }

    #[test]
    fn preserves_other_versions_and_merged_ranges() {
        let f = Fixture::new();
        let a = f.file("S01E01-E02.mkv", 1, 2, b"merged");
        let other = f.file("S01E01-E02.mp4", 1, 2, b"other version");
        f.reorder(&[(2, 3), (2, 4), (1, 3)]);
        let p = f.preview(&[&a]);
        assert_eq!(p.rows[0].episode_ids, vec!["1", "2"]);
        let p = create(&f.store, &p.id).unwrap();
        assert_eq!(f.run(&p).state, "complete");
        assert_eq!(
            fs::read(f.folder.join("Season 02/S02E03-E04.mkv")).unwrap(),
            b"merged"
        );
        assert_eq!(fs::read(other).unwrap(), b"other version");
    }

    #[test]
    fn non_contiguous_merged_episodes_and_shared_sidecars_require_resolution() {
        let f = Fixture::new();
        let a = f.file("S01E01-E02.mkv", 1, 2, b"merged");
        let mut w: Work = f.store.get("works", "fictional").unwrap().unwrap();
        w.episodes[1].number = 7;
        assert!(tmdb_groups::save_reordered_work(&f.store, &w, "").is_err());
        let _ = f.file("S01E01-E02.mp4", 1, 2, b"other");
        fs::write(a.with_extension("srt"), b"shared subtitles").unwrap();
        f.reorder(&[(2, 1), (2, 2), (1, 3)]);
        assert!(preview(&f.store, vec![paths::display(&a)]).is_err());
        assert!(a.exists());
    }

    #[test]
    fn changed_media_metadata_and_links_are_rejected_before_execution() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"first");
        f.reorder(&[(2, 1), (1, 2), (1, 3)]);
        let p = f.preview(&[&a]);
        fs::write(&a, b"changed media file").unwrap();
        assert!(create(&f.store, &p.id).is_err());
        assert!(!Path::new(&p.rows[0].target).exists());
        assert!(resources::regular(&a, Some(&f.folder.join("unrelated"))).is_err());
        let symlink = f.folder.join("linked.mkv");
        if paths::symlink_file(&a, &symlink).is_ok() {
            assert!(resources::regular(&symlink, None).is_err());
        }
    }

    #[test]
    fn pause_during_staging_preserves_recovery_and_resumes_after_restart() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"first");
        let b = f.file("S01E02.mkv", 2, 2, b"other");
        f.reorder(&[(1, 2), (1, 1), (1, 3)]);
        let p = f.preview(&[&a, &b]);
        let p = create(&f.store, &p.id).unwrap();
        let paused = copy::execute(f.store.clone(), p.id.clone(), |p| {
            if p.rows
                .iter()
                .any(|r| r.rename.as_ref().unwrap().phase == "staged")
            {
                f.store.cancel.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(paused.state, "paused");
        assert!(copy::cancel_plan(&f.store, &p.id).is_err());
        assert!(copy::delete_plan(&f.store, &p.id).is_err());
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        let done = copy::execute(reopened, p.id, |_| {}).unwrap();
        assert_eq!(
            done.state,
            "complete",
            "{:?} {:?}",
            done.metadata_errors,
            done.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
        );
        assert_eq!(fs::read(&a).unwrap(), b"other");
        assert_eq!(fs::read(&b).unwrap(), b"first");
    }

    #[test]
    fn interrupted_commit_is_verified_and_recovered_without_size_only_acceptance() {
        for corrupt in [false, true] {
            let f = Fixture::new();
            let a = f.file("S01E01.mkv", 1, 1, b"first");
            f.reorder(&[(2, 1), (1, 2), (1, 3)]);
            let p = f.preview(&[&a]);
            let mut p = create(&f.store, &p.id).unwrap();
            stage(&f.store, &mut p, 0).unwrap();
            fs::create_dir_all(Path::new(&p.rows[0].target).parent().unwrap()).unwrap();
            p.rows[0].rename.as_mut().unwrap().phase = "committing".into();
            copy::save(&f.store, &mut p).unwrap();
            move_file(
                &f.store,
                &p.rows[0],
                Path::new(&p.rows[0].stage),
                Path::new(&p.rows[0].target),
            )
            .unwrap();
            if corrupt {
                fs::remove_file(&p.rows[0].target).unwrap();
                fs::write(&p.rows[0].target, b"other").unwrap();
            }
            let done = f.run(&p);
            assert_eq!(done.state, if corrupt { "failed" } else { "complete" });
        }
    }

    #[test]
    fn failed_backup_keeps_sources_and_retry_finishes_backup_before_rename() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"first");
        let nfo = a.with_extension("nfo");
        fs::write(&nfo, b"<episodedetails/>").unwrap();
        f.reorder(&[(2, 1), (1, 2), (1, 3)]);
        let p = f.preview(&[&a]);
        let p = create(&f.store, &p.id).unwrap();
        let backup = Path::new(&p.rows[0].backups[0].backup);
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::write(backup, b"occupied").unwrap();
        assert_eq!(f.run(&p).state, "failed");
        assert!(a.exists() && nfo.exists());
        fs::remove_file(backup).unwrap();
        assert_eq!(f.run(&p).state, "complete");
        assert_eq!(fs::read(backup).unwrap(), b"<episodedetails/>");
    }

    #[test]
    fn custom_naming_and_hash_verification_keep_the_registered_work_folder() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"media");
        let mut settings = f.store.settings().unwrap();
        settings.output_root = paths::display(&f._temp.path().join("new-library"));
        settings.naming.tv =
            "{{title}}/Season {{season}}/{{season_episode}} - {{episode_title}}{{fileExt}}".into();
        settings.verify_hash = true;
        for kind in ["tv", "movie", "music"] {
            fs::create_dir_all(naming::root(&settings, kind).unwrap()).unwrap();
        }
        f.store.put("settings", "main", &settings).unwrap();
        f.reorder(&[(4, 9), (1, 2), (1, 3)]);
        let p = f.preview(&[&a]);
        let target = f.folder.join("Season 04/S04E09 - 章节 1.mkv");
        assert_eq!(
            paths::key(Path::new(&p.rows[0].target)),
            paths::key(&target)
        );
        let p = create(&f.store, &p.id).unwrap();
        let done = f.run(&p);
        assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
        assert!(!done.rows[0].digest.is_empty());
        assert_eq!(fs::read(target).unwrap(), b"media");
        assert!(!a.exists());
    }

    #[test]
    fn late_target_collision_preserves_staged_source_and_reserves_recovery_queue() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"media");
        f.reorder(&[(1, 2), (1, 1), (1, 3)]);
        let p = f.preview(&[&a]);
        let mut p = create(&f.store, &p.id).unwrap();
        stage(&f.store, &mut p, 0).unwrap();
        fs::write(&p.rows[0].target, b"unrelated file").unwrap();
        let failed = f.run(&p);
        assert_eq!(failed.state, "failed");
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"unrelated file");
        assert_eq!(fs::read(&p.rows[0].stage).unwrap(), b"media");
        assert!(scan::index_library(&f.store).is_err());
        assert!(crate::task_queue::set_paused(&f.store, false).is_err());
        fs::remove_file(&p.rows[0].target).unwrap();
        let queue = QueueState {
            ids: vec!["unrelated-task".into()],
            paused: true,
            ..Default::default()
        };
        f.store.put("queue", "main", &queue).unwrap();
        crate::task_queue::enqueue(&f.store, &p.id).unwrap();
        assert_eq!(crate::task_queue::state(&f.store).unwrap().ids[0], p.id);
        crate::task_queue::set_paused(&f.store, false).unwrap();
        assert_eq!(f.run(&p).state, "complete");
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"media");
    }

    #[test]
    fn offline_source_and_new_target_nfo_require_a_fresh_preview() {
        let f = Fixture::new();
        let a = f.file("S01E01.mkv", 1, 1, b"media");
        f.reorder(&[(1, 2), (1, 1), (1, 3)]);
        let p = f.preview(&[&a]);
        let nfo = Path::new(&p.rows[0].target).with_extension("nfo");
        fs::write(&nfo, b"unrelated metadata").unwrap();
        assert!(create(&f.store, &p.id).is_err());
        assert!(a.exists());
        assert_eq!(fs::read(&nfo).unwrap(), b"unrelated metadata");
        fs::remove_file(nfo).unwrap();
        let offline = f.folder.with_file_name("offline");
        fs::rename(&f.folder, &offline).unwrap();
        assert!(create(&f.store, &p.id).is_err());
        assert!(offline.join("Season 01/S01E01.mkv").exists());
    }
}
