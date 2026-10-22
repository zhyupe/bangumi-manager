use crate::{db::Store, model::*, paths, scan};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};

pub fn episode_range<'a>(work: &'a Work, ids: &[String]) -> Result<Vec<&'a Episode>> {
    let mut result = ids
        .iter()
        .map(|id| {
            work.episodes
                .iter()
                .find(|e| &e.id == id)
                .ok_or("集数映射不存在".to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    result.sort_by_key(|e| (e.season, e.number));
    if result.is_empty() || result.iter().any(|e| !e.enabled) {
        return Err("请先确认并启用对应集数".into());
    }
    for pair in result.windows(2) {
        if pair[0].season != pair[1].season || pair[1].number != pair[0].number + 1 {
            return Err("多集文件只能映射到同一季的连续集数".into());
        }
    }
    Ok(result)
}
pub(crate) fn destination(s: &Settings, w: &Work, m: &Mapping) -> Result<PathBuf> {
    let source = paths::normalize(&m.source)?;
    if m.extra {
        let relative = crate::extras::relative_path(&source).unwrap_or_else(|| {
            PathBuf::from(paths::clean_name(
                &source.file_name().unwrap_or_default().to_string_lossy(),
            ))
        });
        return Ok(paths::normalize(&s.extras_root)?
            .join(&w.folder_name)
            .join(relative));
    }
    if !s.output_root.is_empty() || w.kind == "music" {
        return crate::naming::target(s, w, &episode_range(w, &m.episode_ids)?, &source, None)
            .map(|v| crate::multipart::target(v.0, m.part));
    }
    if w.kind == "movie" {
        return Ok(crate::multipart::target(
            paths::normalize(&s.movie_root)?
                .join(&w.folder_name)
                .join(format!("{}.{}", w.folder_name, paths::ext(&source))),
            m.part,
        ));
    }
    let eps = episode_range(w, &m.episode_ids)?;
    let first = eps[0];
    let last = eps.last().unwrap();
    let suffix = if eps.len() > 1 {
        format!("-E{:02}", last.number)
    } else {
        String::new()
    };
    Ok(crate::multipart::target(
        paths::normalize(&s.tv_root)?
            .join(&w.folder_name)
            .join(format!("Season {:02}", first.season))
            .join(format!(
                "S{:02}E{:02}{}.{}",
                first.season,
                first.number,
                suffix,
                paths::ext(&source)
            )),
        m.part,
    ))
}
pub(crate) fn sibling_attachments(source: &Path) -> Result<Vec<PathBuf>> {
    let base = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let mut files = vec![];
    for e in fs::read_dir(source.parent().ok_or("无效源路径")?).map_err(|e| e.to_string())? {
        let p = e.map_err(|e| e.to_string())?.path();
        let stem = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if (paths::subtitle(&p) || paths::ext(&p) == "nfo")
            && (stem == base || stem.starts_with(&(base.clone() + ".")))
        {
            files.push(p);
        }
    }
    Ok(files)
}
pub(crate) fn new_row(
    store: &Store,
    m: &Mapping,
    w: &Work,
    target: PathBuf,
    attachment: bool,
    parent_id: Option<String>,
    probe: Option<Probe>,
) -> Result<PlanRow> {
    let source = paths::normalize(&m.source)?;
    let signature = paths::signature(&source)?;
    let target_signature = paths::optional_signature(&target)?;
    let mut conflicts = vec![];
    if target_signature.is_some() {
        conflicts.push("目标文件已存在".into());
    }
    if paths::key(&source) == paths::key(&target) {
        return Err("源文件已经位于目标位置，请跳过此文件".into());
    }
    let target_probe = if target_signature.is_some() && paths::media(&target) {
        scan::cached_probe(store, &paths::display(&target))?
    } else {
        None
    };
    Ok(PlanRow {
        part: m.part,
        subtitle: None,
        rename: None,
        replacement: None,
        remote: None,
        media_folder: String::new(),
        id: id(),
        source: paths::display(&source),
        target: paths::display(&target),
        work_id: w.id.clone(),
        parent_id,
        episode_ids: m.episode_ids.clone(),
        extra: m.extra,
        attachment,
        signature,
        target_signature,
        probe,
        target_probe,
        conflicts: conflicts.clone(),
        warnings: vec![],
        reviewed: false,
        action: if conflicts.is_empty() {
            "copy"
        } else {
            "undecided"
        }
        .into(),
        state: "pending".into(),
        error: String::new(),
        stage: String::new(),
        digest: String::new(),
        backups: vec![],
        backups_prepared: false,
        source_cleanup_pending: false,
        operation: store.settings()?.default_operation,
    })
}
fn mapping_destination(
    s: &Settings,
    w: &Work,
    m: &Mapping,
    source: &Path,
    probe: Option<&Probe>,
) -> Result<(PathBuf, PathBuf)> {
    Ok(
        if !m.extra && (!s.output_root.is_empty() || w.kind == "music") {
            let (path, folder) =
                crate::naming::target(&s, &w, &episode_range(&w, &m.episode_ids)?, &source, probe)?;
            (crate::multipart::target(path, m.part), folder)
        } else {
            (
                destination(&s, &w, m)?,
                if m.extra {
                    paths::normalize(&s.extras_root)?
                } else {
                    crate::naming::root(&s, &w.kind)?
                }
                .join(&w.folder_name),
            )
        },
    )
}

pub fn create_plan(
    store: &Store,
    mappings: Vec<Mapping>,
    progress: impl Fn(usize, usize),
) -> Result<Plan> {
    let plan = build_plan(store, mappings, progress)?;
    store.put("plans", &plan.id, &plan)?;
    Ok(plan)
}
pub(crate) fn build_plan(
    store: &Store,
    mappings: Vec<Mapping>,
    progress: impl Fn(usize, usize),
) -> Result<Plan> {
    store.diagnostics.measure("task.preview", serde_json::json!({"files":mappings.len()}), || {

    if mappings.is_empty() {
        return Err("请选择文件并匹配集数".into());
    }
    let s = store.settings()?;
    paths::validate_settings(&s)?;
    let mut snapshot = s.clone();
    snapshot.tmdb_token.clear();
    let mut plan = Plan {
        kind: "organize".into(),
        settings: Some(snapshot),
        music_root: crate::naming::root(&s, "music")
            .map(|p| paths::display(&p))
            .unwrap_or_default(),
        id: id(),
        created_at: now(),
        state: "draft".into(),
        rows: vec![],
        tv_root: crate::naming::root(&s, "tv")
            .map(|p| paths::display(&p))
            .unwrap_or_default(),
        movie_root: crate::naming::root(&s, "movie")
            .map(|p| paths::display(&p))
            .unwrap_or_default(),
        extras_root: s.extras_root.clone(),
        backup_root: s.backup_root.clone(),
        verify_hash: s.verify_hash,
        works: vec![],
        metadata_errors: vec![],
        metadata_notices: vec![],
        cleanup_notices: vec![],
    };
    let index = if mappings.iter().any(|m| !m.extra) {
        if store.running.load(Ordering::SeqCst) {
            store.get("index", "main")?.unwrap_or_default()
        } else {
            let mut related: Vec<(Work, Vec<PathBuf>)> = Vec::new();
            for m in mappings.iter().filter(|m| !m.extra) {
                let position = if let Some(i) = related.iter().position(|(w, _)| w.id == m.work_id) {
                    i
                } else {
                    let work = store.get::<Work>("works", &m.work_id)?.ok_or("本地剧集不存在")?;
                    related.push((work, Vec::new()));
                    related.len() - 1
                };
                let source = paths::normalize(&m.source)?;
                if !paths::subtitle(&source) {
                    let probe = scan::cached_probe(store, &m.source)?;
                    let (_, folder) = mapping_destination(&s, &related[position].0, m, &source, probe.as_ref())?;
                    related[position].1.push(folder);
                }
            }
            scan::index_works(store, &related)?
        }
    } else {
        LibraryIndex::default()
    };
    let explicit_sources = mappings.iter()
        .map(|m| paths::normalize(&m.source).map(|p| paths::key(&p)))
        .collect::<Result<HashSet<_>>>()?;
    let mut sources = HashSet::new();
    for (i, m) in mappings.iter().enumerate() {
        let source = paths::normalize(&m.source)?;
        if !sources.insert(paths::key(&source)) {
            return Err("同一个源文件不能重复加入计划".into());
        }
        let w = store
            .get::<Work>("works", &m.work_id)?
            .ok_or("本地剧集不存在")?;
        crate::multipart::validate(m.part, &w.kind, m.extra, m.episode_ids.len())?;
        if !m.extra && paths::subtitle(&source) {
            plan.rows.push(crate::subtitles::new_row(store, &s, &index, m, &w)?);
            if !plan.works.iter().any(|x| x.id == w.id) {
                plan.works.push(w);
            }
            progress(i + 1, mappings.len());
            continue;
        }
        let probe = if !m.extra {
            scan::cached_probe(store, &m.source)?
        } else {
            None
        };
        if !m.extra && probe.is_none() {
            let template = match w.kind.as_str() {
                "movie" => &s.naming.movie,
                "music" => &s.naming.music,
                _ => &s.naming.tv,
            };
            if ["videoFormat", "videoCodec", "audioCodec"]
                .iter()
                .any(|name| template.contains(name))
            {
                return Err(format!(
                    "命名规则需要媒体信息，请先为此文件刷新媒体信息：{}",
                    m.source
                ));
            }
        }
        let (target, media_folder) = mapping_destination(&s, &w, m, &source, probe.as_ref())?;
        let root = if m.extra {
            paths::normalize(&s.extras_root)?
        } else {
            crate::naming::root(&s, &w.kind)?
        };
        paths::safe_target(&target, &root)?;
        if !m.extra {
            let folder = media_folder.clone();
            if folder.exists()
                && fs::read_dir(&folder)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_some()
            {
                let owner = store.get::<String>("folders", &paths::key(&folder))?;
                if owner.as_deref() != Some(&w.id) {
                    return Err(format!(
                        "目标目录已有内容，请在剧集目录中扫描并确认关联：{}",
                        paths::display(&folder)
                    ));
                }
            }
        }
        let mut row = new_row(store, m, &w, target.clone(), false, None, probe)?;
        row.media_folder = paths::display(&media_folder);
        if !m.extra {
            if !(if w.kind == "music" {
                paths::audio(&source)
            } else {
                paths::video(&source)
            }) {
                return Err(format!("请选择与资料库类型相符的媒体文件：{}", m.source));
            }
            let eps = episode_range(&w, &m.episode_ids)?;
            let existing: Vec<_> = index
                .assets
                .iter()
                .filter(|a| {
                    a.work_id == w.id
                        && eps.iter().any(|e| {
                            e.season == a.season && e.number >= a.number && e.number <= a.end
                        })
                })
                .collect();
            for asset in existing {
                if (asset.part == 0) != (m.part == 0) {
                    return Err("同一集已有完整文件或分段文件，不能在整理时混用；请先核对现有资源".into());
                }
                if asset.part != m.part { continue; }
                if (asset.number..=asset.end).any(|number| {
                    !eps.iter()
                        .any(|e| e.season == asset.season && e.number == number)
                }) {
                    return Err(format!("已有合并多集文件覆盖 S{:02}E{:02}-E{:02}，不能仅替换其中部分集数。请为完整集数生成合并文件映射，或先人工整理该文件：{}", asset.season, asset.number, asset.end, asset.path));
                }
                if paths::key(Path::new(&asset.path)) == paths::key(&source) {
                    return Err("所选源文件已在本地库中，请从库外选择待整理文件".into());
                }
                if paths::optional_signature(Path::new(&asset.path))?.is_some() {
                    row.conflicts
                        .push(format!("同一集已有版本：{}", asset.path));
                } else {
                    return Err("本地索引中的版本已变化，请重新扫描资料库后生成计划".into());
                }
                if paths::key(Path::new(&asset.path)) != paths::key(&target) {
                    row.backups.push(BackupEntry {
                        preserve_original: false,
                        remote: None,
                        original: asset.path.clone(),
                        backup: String::new(),
                        signature: asset.signature.clone(),
                        digest: String::new(),
                        probe: scan::cached_probe(store, &asset.path)?,
                    });
                }
            }
            if !row.conflicts.is_empty() {
                row.action = "undecided".into();
            }
        }
        let parent_id = row.id.clone();
        plan.rows.push(row);
        if !m.extra {
            for subtitle in sibling_attachments(&source)?
                .into_iter()
                .filter(|p| paths::subtitle(p))
                .filter(|p| !explicit_sources.contains(&paths::key(p)))
            {
                let original_stem = source.file_stem().unwrap_or_default().to_string_lossy();
                let filename = subtitle.file_name().unwrap_or_default().to_string_lossy();
                let raw_suffix = &filename[original_stem.len()..];
                let subtitle_stem = subtitle.file_stem().unwrap_or_default().to_string_lossy();
                let suffix = if raw_suffix.matches('.').count() > 1 {
                    if let Some(language) = crate::subtitles::language(&subtitle_stem, &s.subtitle_language_pattern)? {
                        let dot = raw_suffix[..raw_suffix.rfind('.').unwrap()].rfind('.').unwrap();
                        format!("{}.{}.{}", &raw_suffix[..dot], language, paths::ext(&subtitle))
                    } else { raw_suffix.to_string() }
                } else { raw_suffix.to_string() };
                let target_sub = target.with_file_name(format!(
                    "{}{}",
                    target.file_stem().unwrap_or_default().to_string_lossy(),
                    suffix
                ));
                let mut sub = m.clone();
                sub.source = paths::display(&subtitle);
                plan.rows.push(new_row(
                    store,
                    &sub,
                    &w,
                    target_sub,
                    true,
                    Some(parent_id.clone()),
                    None,
                )?);
            }
        }
        if !plan.works.iter().any(|x| x.id == w.id) {
            plan.works.push(w);
        }
        progress(i + 1, mappings.len());
    }
    // Record all collisions, including overlapping multi-episode files, before any write.
    let mut occupied: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in plan.rows.iter().enumerate() {
        occupied
            .entry(format!("path:{}", paths::key(Path::new(&row.target))))
            .or_default()
            .push(i);
        if !row.extra && !row.attachment {
            let w = plan.works.iter().find(|w| w.id == row.work_id).unwrap();
            for ep in episode_range(w, &row.episode_ids)? {
                for part in crate::multipart::slots(row.part) {
                    occupied
                        .entry(format!("episode:{}:{}:{}:{}", w.id, ep.season, ep.number, part))
                        .or_default()
                        .push(i);
                }
            }
        }
    }
    for indices in occupied.values().filter(|v| v.len() > 1) {
        for i in indices {
            let row = &mut plan.rows[*i];
            if !row
                .conflicts
                .iter()
                .any(|c| c == "本计划中多个文件映射到同一位置或集数")
            {
                row.conflicts
                    .push("本计划中多个文件映射到同一位置或集数".into());
                row.action = "undecided".into();
            }
        }
    }
    let mut durations: HashMap<String, Vec<f64>> = HashMap::new();
    for row in &plan.rows {
        if !row.extra && !row.attachment {
            if let Some(d) = row.probe.as_ref().and_then(|p| p.duration) {
                durations.entry(row.work_id.clone()).or_default().push(d);
            }
        }
    }
    for row in &mut plan.rows {
        if let Some(p) = &row.probe {
            if let Some(e) = &p.error {
                row.warnings.push(e.clone());
            }
            if let (Some(d), Some(group)) = (p.duration, durations.get(&row.work_id)) {
                let mut sorted = group.clone();
                sorted.sort_by(f64::total_cmp);
                if row.part == 0 && sorted.len() >= 3 && d < sorted[sorted.len() / 2] * 0.45 {
                    row.warnings
                        .push("时长明显短于同批文件，请核对是否为 OVA、特典或错误集数".into());
                }
            }
        }
    }
    plan.rows.sort_by(|a, b| a.source.cmp(&b.source));
    crate::subtitles::validate_context(store, &plan)?;
    Ok(plan)

    })
}
pub fn decisions(store: &Store, plan_id: &str, choices: Vec<Decision>) -> Result<Plan> {
    let active = store.active_plan.lock().map_err(|e| e.to_string())?;
    if active.as_deref() == Some(plan_id) {
        return Err("此整理任务执行中，请先暂停任务".into());
    }
    let mut p = store.get::<Plan>("plans", plan_id)?.ok_or("计划不存在")?;
    if p.kind == "cue_split" {
        return Err("CUE tasks use confirmed split decisions; preview again to change them".into());
    }
    if p.kind == "rename" {
        return Err("重命名任务按已确认的预览执行，请重新预览以调整范围".into());
    }
    if ["queued", "running", "cancelling", "cancelled", "complete"].contains(&p.state.as_str())
        || p.rows.iter().any(|r| {
            r.state == "complete"
                || !r.digest.is_empty()
                || r.backups_prepared
                || r.remote.is_some()
        })
    {
        return Err("已执行的计划不能更改映射，请建立新计划".into());
    }
    apply_decisions(&mut p, choices)?;
    store.put("plans", &p.id, &p)?;
    Ok(p)
}
pub(crate) fn apply_decisions(p: &mut Plan, choices: Vec<Decision>) -> Result<()> {
    for choice in choices {
        if !["copy", "replace", "skip", "undecided"].contains(&choice.action.as_str()) {
            return Err("无效冲突决策".into());
        }
        if !["copy", "move", "hardlink", "symlink", "adopt"].contains(&choice.operation.as_str()) {
            return Err("无效文件操作方式".into());
        }
        if let Some(row) = p.rows.iter_mut().find(|r| r.id == choice.id) {
            if row.replacement.is_some()
                && (choice.operation != row.operation
                    || !["replace", "skip"].contains(&choice.action.as_str()))
            {
                return Err(
                    "替换任务只能跳过或执行已预览的操作；调整文件操作请重新生成预览".into(),
                );
            }
            if row.replacement.is_none() && choice.operation == "adopt" {
                return Err("接纳操作仅用于替换预览".into());
            }
            row.operation = choice.operation;
            row.action = choice.action;
            row.reviewed = choice.reviewed;
        }
    }
    let decisions: HashMap<_, _> = p
        .rows
        .iter()
        .filter(|r| !r.attachment)
        .map(|r| {
            (
                r.id.clone(),
                (r.action.clone(), r.reviewed, r.operation.clone()),
            )
        })
        .collect();
    for row in &mut p.rows {
        if let Some(parent) = row.parent_id.as_ref().and_then(|id| decisions.get(id)) {
            row.action = parent.0.clone();
            row.reviewed = parent.1;
            if row.replacement.is_none() {
                row.operation = parent.2.clone();
            }
        }
    }
    Ok(())
}
pub fn preflight(store: &Store, p: &Plan) -> Result<()> {
    if p.kind == "cue_split" { return crate::cue::preflight(store,p,false); }
    store.diagnostics.measure(
        "task.preflight",
        serde_json::json!({"task_id":p.id}),
        || {
            validate_plan(p)?;
            crate::resources::validate_context(store, p)?;
            crate::renaming::validate_context(store, p)?;
            crate::subtitles::validate_context(store, p)?;
            let s = store.settings()?;
            if crate::naming::root(&s, "tv")
                .map(|p| paths::display(&p))
                .unwrap_or_default()
                != p.tv_root
                || crate::naming::root(&s, "movie")
                    .map(|p| paths::display(&p))
                    .unwrap_or_default()
                    != p.movie_root
                || s.extras_root != p.extras_root
                || s.backup_root != p.backup_root
            {
                return Err(
                    "资料库路径已在预览后修改，请重新生成计划；恢复旧任务时需使用原路径设置".into(),
                );
            }
            if let Some(old) = &p.settings {
                if old.output_root != s.output_root
                    || old.music_root != s.music_root
                    || serde_json::to_value(&old.naming).unwrap()
                        != serde_json::to_value(&s.naming).unwrap()
                {
                    return Err("输出目录或命名规则已修改，请重新生成计划".into());
                }
            }
            for work in &p.works {
                let current = store
                    .get::<Work>("works", &work.id)?
                    .ok_or("计划关联的本地条目已不存在")?;
                if serde_json::to_value(&current).map_err(|e| e.to_string())?
                    != serde_json::to_value(work).map_err(|e| e.to_string())?
                {
                    return Err(format!(
                        "{} 的资料或分季映射已在预览后修改，请重新生成计划",
                        work.title
                    ));
                }
            }
            Ok(())
        },
    )
}
fn validate_plan(p: &Plan) -> Result<()> {
    if p.state == "cancelled" {
        return Err("任务已取消，请重新创建整理计划".into());
    }
    if p.kind == "rename" {
        return crate::renaming::validate_plan(p);
    }
    let sources: HashSet<_> = p
        .rows
        .iter()
        .map(|r| paths::key(Path::new(&r.source)))
        .collect();
    let mut targets = HashSet::new();
    let mut eps = HashSet::new();
    for row in p.rows.iter().filter(|r| r.action != "skip") {
        if !["copy", "move", "hardlink", "symlink", "adopt"].contains(&row.operation.as_str()) {
            return Err("无效文件操作方式".into());
        }
        let adopt = row.operation == "adopt"
            && row.replacement.as_ref().is_some_and(|r| r.adopt)
            && paths::key(Path::new(&row.source)) == paths::key(Path::new(&row.target));
        if row.operation == "adopt" && !adopt {
            return Err("无效的接纳操作".into());
        }
        if sources.contains(&paths::key(Path::new(&row.target))) && !adopt {
            return Err("目标与计划中的源文件重叠，请重新生成计划".into());
        }
        if !["copy", "replace"].contains(&row.action.as_str()) {
            return Err("还有未决冲突，请选择保留、替换或跳过".into());
        }
        if !row.warnings.is_empty() && !row.reviewed {
            return Err("请确认带警告的文件已人工核对".into());
        }
        if !targets.insert(paths::key(Path::new(&row.target))) {
            return Err("仍有多个文件写入同一个目标，请只保留其中一份".into());
        }
        if !row.extra && !row.attachment {
            let w = p
                .works
                .iter()
                .find(|w| w.id == row.work_id)
                .ok_or("计划缺少条目快照")?;
            crate::multipart::validate(row.part, &w.kind, row.extra, row.episode_ids.len())?;
            for e in episode_range(w, &row.episode_ids)? {
                for part in crate::multipart::slots(row.part) {
                    if !eps.insert((w.id.clone(), e.season, e.number, part)) {
                        return Err("同一集仍选择了多个版本，请跳过重复项".into());
                    }
                }
            }
        }
        if (row.target_signature.is_some() || !row.backups.is_empty()) && row.action != "replace" {
            return Err("目标已有资源，需要明确选择替换或跳过".into());
        }
        if row.action == "replace" && p.backup_root.is_empty() {
            return Err("替换前必须设置位于媒体库外的备份目录".into());
        }
    }
    Ok(())
}
pub fn digest(path: &Path) -> Result<String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
pub fn measured_digest(store: &Store, path: &Path) -> Result<String> {
    store
        .diagnostics
        .measure("file.sha256", serde_json::json!({}), || digest(path))
}
pub(crate) fn stream_copy(store: &Store, source: &Path, target: &Path) -> Result<(u64, String)> {
    store
        .diagnostics
        .measure("file.stream_copy", serde_json::json!({}), || {
            let mut src = File::open(source).map_err(|e| e.to_string())?;
            let mut dst = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            let mut bytes = 0;
            let mut hash = Sha256::new();
            loop {
                if store.cancel.load(Ordering::Relaxed) {
                    return Err("任务已取消；可从当前文件重新复制".into());
                }
                let n = src.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                dst.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                hash.update(&buf[..n]);
                bytes += n as u64;
            }
            dst.sync_all().map_err(|e| e.to_string())?;
            Ok((bytes, hex::encode(hash.finalize())))
        })
}
pub(crate) fn save(store: &Store, p: &mut Plan) -> Result<()> {
    let _active = store.active_plan.lock().map_err(|e| e.to_string())?;
    // A worker may hold an older snapshot when cancellation is requested.
    // Never overwrite the durable stop request with that snapshot.
    if let Some(saved) = store.get::<Plan>("plans", &p.id)? {
        if saved.state == "cancelling" || saved.state == "cancelled" {
            if p.state == "running" || p.state == "cancelling" {
                p.state = "cancelling".into();
            } else {
                p.mark_cancelled();
            }
        }
    }
    store.put("plans", &p.id, p)
}
pub fn start_plan(store: &Store, id: &str) -> Result<()> {
    let mut active = store.active_plan.lock().map_err(|e| e.to_string())?;
    if active.is_some() {
        return Err("已有整理任务正在执行".into());
    }
    let mut p = store.get::<Plan>("plans", id)?.ok_or("计划不存在")?;
    if ["complete", "cancelled", "cancelling"].contains(&p.state.as_str()) {
        return Err("任务已完成或已取消，请重新创建整理计划".into());
    }
    preflight(store, &p)?;
    p.state = "running".into();
    store.put("plans", id, &p)?;
    store.cancel.store(false, Ordering::SeqCst);
    store.running.store(true, Ordering::SeqCst);
    *active = Some(id.into());
    Ok(())
}
pub fn pause_plan(store: &Store, id: &str) -> Result<()> {
    let active = store.active_plan.lock().map_err(|e| e.to_string())?;
    if active.as_deref() != Some(id) {
        return Err("此任务当前未在执行".into());
    }
    store.cancel.store(true, Ordering::SeqCst);
    Ok(())
}
pub fn cancel_plan(store: &Store, id: &str) -> Result<Plan> {
    let active = store.active_plan.lock().map_err(|e| e.to_string())?;
    let mut p = store.get::<Plan>("plans", id)?.ok_or("计划不存在")?;
    if crate::cue::cleanup_in_progress(store, &p)? {
        return Err("errors.cue.originals.recoveryRequired".into());
    }
    if p.kind == "rename" && (p.state == "running" || crate::renaming::in_progress(&p)) {
        return Err("重命名已开始，请暂停或继续完成任务，以保留文件恢复记录".into());
    }
    if p.state == "complete" || p.state == "cancelled" {
        return Ok(p);
    }
    if active.as_deref() == Some(id) {
        p.state = "cancelling".into();
        store.put("plans", id, &p)?;
        store.cancel.store(true, Ordering::SeqCst);
    } else {
        p.mark_cancelled();
        store.put("plans", id, &p)?;
    }
    Ok(p)
}
pub fn delete_plan(store: &Store, id: &str) -> Result<()> {
    delete_plans(store, &[id.to_owned()])
}
pub fn delete_plans(store: &Store, ids: &[String]) -> Result<()> {
    let active = store.active_plan.lock().map_err(|e| e.to_string())?;
    // Validate the entire batch under the same execution lock, then delete atomically.
    for id in ids {
        if active.as_deref() == Some(id.as_str()) {
            return Err("任务尚未停止，请等待执行结束后删除".into());
        }
        let p = store.get::<Plan>("plans", id)?.ok_or("计划不存在")?;
        if crate::cue::cleanup_in_progress(store, &p)? {
            return Err("errors.cue.originals.recoveryRequired".into());
        }
        if crate::renaming::in_progress(&p) {
            return Err("重命名尚未完成，不能删除文件恢复记录".into());
        }
        crate::backups::register_plan(store, &p)?;
        if !["complete", "cancelled"].contains(&p.state.as_str()) {
            return Err("只能删除已完成或已取消的任务".into());
        }
    }
    let mut conn = store.connection()?;
    let transaction = conn.transaction().map_err(|e| e.to_string())?;
    for id in ids {
        // Delete records only: media, backups and partial files remain on disk.
        transaction
            .execute(
                "DELETE FROM documents WHERE collection='plans' AND id=?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())
}
pub fn finish_plan(store: &Store, id: &str, error: Option<&str>) -> Result<Plan> {
    let mut active = store.active_plan.lock().map_err(|e| e.to_string())?;
    let result = (|| {
        let mut p = store.get::<Plan>("plans", id)?.ok_or("计划不存在")?;
        if p.state == "cancelling" {
            p.mark_cancelled();
        } else if let Some(error) = error {
            p.state = "failed".into();
            p.metadata_errors.push(error.into());
        }
        store.put("plans", id, &p)?;
        Ok(p)
    })();
    // Release the execution slot even if saving the final status failed.
    if active.as_deref() == Some(id) {
        *active = None;
        store.running.store(false, Ordering::SeqCst);
    }
    result
}
pub(crate) fn backup_candidates(p: &Plan, index: usize) -> Result<Vec<BackupEntry>> {
    let row = &p.rows[index];
    let mut candidates = row.backups.clone();
    if let Some(sig) = &row.target_signature {
        if !candidates.iter().any(|e| e.original == row.target) {
            candidates.push(BackupEntry {
                preserve_original: false, remote: None, original: row.target.clone(),
                backup: String::new(), signature: sig.clone(), digest: String::new(), probe: row.target_probe.clone(),
            });
        }
    }
    if !row.backups_prepared && row.replacement.is_none() && row.rename.is_none() && !row.attachment {
        let originals: Vec<_> = candidates.iter().map(|e| e.original.clone()).collect();
        for original in originals {
            for sidecar in sibling_attachments(Path::new(&original))? {
                let original = paths::display(&sidecar);
                if !candidates.iter().any(|e| e.original == original)
                    && !p.rows.iter().any(|r| r.target == original)
                {
                    candidates.push(BackupEntry {
                        preserve_original: false, remote: None, original, backup: String::new(),
                        signature: paths::signature(&sidecar)?, digest: String::new(), probe: None,
                    });
                }
            }
        }
    }
    Ok(candidates)
}
pub(crate) fn backup_existing(store: &Store, p: &mut Plan, index: usize) -> Result<()> {
    store.diagnostics.measure(
        "file.backup",
        serde_json::json!({"task_id":p.id,"row_id":p.rows[index].id}),
        || {
            let row = &p.rows[index];
            let target = PathBuf::from(&row.target);
            let mut candidates = backup_candidates(p, index)?;
            let root = paths::normalize(&p.backup_root)?;
            for (n, c) in candidates.iter_mut().enumerate() {
                if c.backup.is_empty() {
                    c.backup =
                        paths::display(&root.join(&p.id).join(&p.rows[index].id).join(format!(
                    "{n:03}-{}",
                    Path::new(&c.original)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )));
                }
            }
            p.rows[index].backups = candidates;
            p.rows[index].backups_prepared = true;
            p.rows[index].state = "backing_up".into();
            save(store, p)?;
            for n in 0..p.rows[index].backups.len() {
                let b = p.rows[index].backups[n].clone();
                let source = PathBuf::from(&b.original);
                let backup = PathBuf::from(&b.backup);
                paths::safe_target(&backup, &root)?;
                if p.rows[index].replacement.is_some()
                    && b.digest.is_empty()
                    && b.remote.is_none()
                    && crate::resources::regular(&source, None)? != b.signature
                {
                    return Err("旧资源或附件在预览后变化，请重新生成替换计划".into());
                }
                #[cfg(windows)]
                if b.remote.is_some()
                    || (b.digest.is_empty() && crate::remote_transfer::pair(&source, &backup))
                {
                    remote_backup(store, p, index, n)?;
                    continue;
                }
                if !b.digest.is_empty() {
                    if paths::signature(&backup)?.size != b.signature.size
                        || measured_digest(store, &backup)? != b.digest
                    {
                        return Err("备份校验失败，停止替换".into());
                    }
                    continue;
                }
                if paths::signature(&source)? != b.signature {
                    return Err("已有资源在预览后发生变化，请重新生成计划".into());
                }
                fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
                let hash = if backup.exists() {
                    let source_hash = measured_digest(store, &source)?;
                    if measured_digest(store, &backup)? != source_hash {
                        return Err("备份路径存在不一致文件，保留原资源并停止".into());
                    }
                    source_hash
                } else {
                    let stage = backup.with_extension("bm-backup-partial");
                    if stage.exists() {
                        fs::remove_file(&stage).map_err(|e| e.to_string())?;
                    }
                    let (count, hash) = stream_copy(store, &source, &stage)?;
                    if count != b.signature.size
                        || paths::signature(&source)? != b.signature
                        || measured_digest(store, &stage)? != hash
                    {
                        return Err("备份复制校验失败".into());
                    }
                    paths::commit_no_replace(&stage, &backup)?;
                    hash
                };
                p.rows[index].backups[n].digest = hash;
                save(store, p)?;
            }
            // Remove copy-backed originals only after all backups are durable and verified.
            // Move-backed originals already live at their checked backup paths.
            if store.cancel.load(Ordering::Relaxed) {
                return Err("任务已停止，旧版本和备份已保留".into());
            }
            for b in &p.rows[index].backups {
                if b.preserve_original {
                    continue;
                }
                let original = Path::new(&b.original);
                if b.remote.as_ref().is_some_and(|proof| proof.method == "smb_move") {
                    // The original was already renamed. Never delete a newly-created file here.
                    if original.try_exists().map_err(|e| e.to_string())? {
                        return Err("旧版本路径重新出现文件，停止替换".into());
                    }
                    continue;
                }
                if let Some(sig) = paths::optional_signature(original)? {
                    if sig.size != b.signature.size
                        || ((b.remote.is_none() || p.rows[index].replacement.is_some())
                            && sig != b.signature)
                    {
                        return Err("旧版本已变化，停止替换".into());
                    }
                    #[cfg(windows)]
                    if let Some(proof) = &b.remote {
                        crate::remote_transfer::open_verified(
                            original,
                            &proof.source,
                            &proof.sha256,
                            true,
                        )?
                        .delete()?;
                        continue;
                    }
                    fs::remove_file(original)
                        .map_err(|e| format!("旧版本已备份，但无法替换 {}：{}", b.original, e))?;
                }
            }
            if p.rows[index].rename.is_none() && p.rows[index].operation != "adopt" && paths::optional_signature(&target)?.is_some() {
                return Err("目标位置被其他文件占用，已保留备份和临时文件".into());
            }
            Ok(())
        },
    )
}
#[cfg(windows)]
fn same_moved_backup(actual: &FileStamp, expected: &FileStamp) -> bool {
    // Windows may retain a deleted destination's creation time through file tunneling.
    actual.identity == expected.identity && actual.size == expected.size && actual.modified == expected.modified
}
#[cfg(windows)]
fn remote_backup(store: &Store, p: &mut Plan, index: usize, n: usize) -> Result<()> {
    use crate::remote_transfer as remote;
    let b = p.rows[index].backups[n].clone();
    let source = Path::new(&b.original);
    let backup = Path::new(&b.backup);
    let stage = backup.with_extension("bm-backup-partial");
    if backup.try_exists().map_err(|e| e.to_string())? {
        let proof = b
            .remote
            .as_ref()
            .ok_or("备份路径存在无检查点文件，停止替换")?;
        if proof.method == "smb_move" {
            let file = remote::open_verified(backup, &proof.target, &proof.sha256, false)?;
            let stamp = file.stamp()?;
            if !same_moved_backup(&stamp, &proof.target) || source.try_exists().map_err(|e| e.to_string())? {
                return Err("远端移动备份与检查点不一致，停止替换".into());
            }
            p.rows[index].backups[n].remote.as_mut().unwrap().target = stamp;
            p.rows[index].backups[n].remote.as_mut().unwrap().ready = true;
            save(store, p)?;
        } else {
            if !proof.ready {
                return Err("备份路径存在无检查点文件，停止替换".into());
            }
            remote::open_verified(backup, &proof.target, &proof.sha256, false)?;
        }
        return Ok(());
    }
    fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
    if !b.preserve_original && b.remote.as_ref().is_none_or(|r| r.method == "smb_move") {
        let moved = store.diagnostics.measure_with(
            "file.backup.remote_move",
            serde_json::json!({"task_id":p.id,"row_id":p.rows[index].id,"bytes":b.signature.size}),
            |span| {
                let file = remote::LockedFile::open(source, true)?;
                let stamp = file.stamp()?;
                if paths::signature(source)? != b.signature
                    || b.remote.as_ref().is_some_and(|r| r.source != stamp)
                {
                    return Err("旧资源或附件在预览后变化，请重新生成替换计划".into());
                }
                let hash = if p.verify_hash {
                    measured_digest(store, source)?
                } else {
                    String::new()
                };
                // Persist rename intent before changing the filesystem; recovery verifies the target identity.
                p.rows[index].backups[n].remote = Some(RemoteTransfer {
                    method: "smb_move".into(),
                    source: stamp.clone(),
                    target: stamp,
                    ready: true,
                    sha256: hash,
                });
                store.diagnostics.measure(
                    "file.backup.checkpoint",
                    serde_json::json!({"stage":"before_move"}),
                    || save(store, p),
                )?;
                remote::check_cancel(&store.cancel)?;
                match file.rename(backup) {
                    Ok(()) => {
                        let proof = p.rows[index].backups[n].remote.as_mut().unwrap();
                        let stamp = file.stamp()?;
                        if !same_moved_backup(&stamp, &proof.target) {
                            return Err("远端移动备份与检查点不一致，停止替换".into());
                        }
                        proof.target = stamp;
                        proof.ready = true;
                        store.diagnostics.measure(
                            "file.backup.checkpoint",
                            serde_json::json!({"stage":"after_move"}),
                            || save(store, p),
                        )?;
                        span.field("method", "smb_move");
                        Ok(true)
                    }
                    Err(e) if matches!(e.raw_os_error(), Some(1 | 17 | 50 | 87)) => {
                        span.field("fallback_error", e.to_string());
                        p.rows[index].backups[n].remote = None;
                        save(store, p)?;
                        Ok(false)
                    }
                    Err(e) => Err(format!("远端移动失败，源文件已保留：{e}")),
                }
            },
        )?;
        if moved {
            return Ok(());
        }
    }
    let previous = p.rows[index].backups[n].remote.clone();
    let session = store.diagnostics.measure(
        "file.backup.remote_copy",
        serde_json::json!({"task_id":p.id,"row_id":p.rows[index].id,"bytes":b.signature.size,"verify_hash":p.verify_hash,"preserve_original":b.preserve_original}),
        || remote::prepare_copy(
        source,
        &stage,
        false,
        &b.signature,
        previous.as_ref(),
        p.verify_hash,
        &store.cancel,
        |proof| {
            p.rows[index].backups[n].remote = Some(proof.clone());
            store.diagnostics.measure("file.backup.checkpoint", serde_json::json!({"stage":"copy"}), || save(store, p))
        },
    ))?;
    remote::check_cancel(&store.cancel)?;
    store.diagnostics.measure(
        "file.backup.commit",
        serde_json::json!({"method":"smb_copy"}),
        || {
            session
                .stage
                .rename(backup)
                .map_err(|e| format!("无法提交远端备份：{e}"))
        },
    )?;
    Ok(())
}
pub(crate) fn check_destination(p: &Plan, index: usize, target: &Path, size_only: bool) -> Result<()> {
    let row = &p.rows[index];
    let current = paths::optional_signature(target)?;
    let matches = if size_only {
        current.as_ref().map(|s| s.size) == row.target_signature.as_ref().map(|s| s.size)
    } else {
        current == row.target_signature
    };
    if !matches {
        let removed_by_job = current.is_none()
            && row.backups.iter().any(|b| {
                b.original == row.target
                    && (!b.digest.is_empty() || b.remote.as_ref().is_some_and(|r| r.ready))
            });
        if !removed_by_job {
            return Err("目标在预览后发生变化，请重新生成计划".into());
        }
    }
    Ok(())
}
pub(crate) fn complete_row(store: &Store, p: &mut Plan, index: usize) -> Result<()> {
    crate::tmdb_groups::register_completed(store, &p.rows[index])?;
    if p.rows[index].operation == "move" {
        store.remove("sources", &paths::key(Path::new(&p.rows[index].source)))?;
    }
    p.rows[index].source_cleanup_pending = false;
    p.rows[index].state = "complete".into();
    p.rows[index].error.clear();
    save(store, p)
}
// A move is already complete when its source is gone and the planned target has
// the recorded size. Accept this even with stale/missing checkpoints or hashes.
fn recover_moved_file(
    store: &Store,
    p: &mut Plan,
    index: usize,
    source: &Path,
    target: &Path,
) -> Result<bool> {
    let row = &p.rows[index];
    if row.operation != "move" || source.try_exists().map_err(|e| e.to_string())? {
        return Ok(false);
    }
    let Some(current) = paths::optional_signature(target)? else {
        return Ok(false);
    };
    if current.size != row.signature.size {
        return Err(format!(
            "已移动目标文件大小与记录不一致：{}（记录 {} 字节，当前 {} 字节）",
            paths::display(target),
            row.signature.size,
            current.size
        ));
    }
    if row.replacement.is_some() {
        if !row.backups_prepared
            || row
                .backups
                .iter()
                .any(|b| b.digest.is_empty() && !b.remote.as_ref().is_some_and(|proof| proof.ready))
        {
            return Err("无法确认替换备份已完成，不能仅凭文件大小接纳目标".into());
        }
        crate::resources::regular(target, None)?;
        if !row.digest.is_empty() {
            if measured_digest(store, target)? != row.digest {
                return Err("替换目标内容与检查点不一致".into());
            }
        } else {
            #[cfg(windows)]
            {
                let proof = row
                    .remote
                    .as_ref()
                    .filter(|r| r.ready)
                    .ok_or("替换任务缺少有效移动检查点")?;
                let file = crate::remote_transfer::open_verified(
                    target,
                    &proof.target,
                    &proof.sha256,
                    false,
                )?;
                if proof.sha256.is_empty() && file.stamp()? != proof.target {
                    return Err("无法确认替换目标与移动检查点一致".into());
                }
            }
            #[cfg(not(windows))]
            return Err("替换任务缺少内容校验记录".into());
        }
    }
    complete_row(store, p, index)?;
    Ok(true)
}
#[cfg(windows)]
fn remote_row(store: &Store, p: &mut Plan, index: usize, root: &Path) -> Result<()> {
    use crate::remote_transfer as remote;
    let row = p.rows[index].clone();
    let source = paths::normalize(&row.source)?;
    let target = paths::normalize(&row.target)?;
    let stage = target
        .parent()
        .unwrap()
        .join(format!(".bm-{}-{}.partial", p.id, row.id));
    remote::check_cancel(&store.cancel)?;
    paths::safe_target_entry(&target, root)?;
    if recover_moved_file(store, p, index, &source, &target)? {
        return Ok(());
    }
    // Resume a committed copy using its recorded size (and optional content hash).
    if let Some(proof) = &row.remote {
        let was_committed = proof.ready
            && target.try_exists().map_err(|e| e.to_string())?
            && if proof.method == "smb_move" {
                !source.try_exists().map_err(|e| e.to_string())?
            } else {
                !stage.try_exists().map_err(|e| e.to_string())?
            };
        if was_committed {
            let _target = remote::open_verified(&target, &proof.target, &proof.sha256, false)?;
            if row.operation == "move" && proof.method != "smb_move" {
                if source.try_exists().map_err(|e| e.to_string())? {
                    let file = remote::open_verified(&source, &proof.source, &proof.sha256, true)?;
                    remote::check_cancel(&store.cancel)?;
                    p.rows[index].source_cleanup_pending = true;
                    save(store, p)?;
                    file.delete()?;
                } else if !row.source_cleanup_pending {
                    return Err("无法确认远端移动源文件状态".into());
                }
            }
            return complete_row(store, p, index);
        }
    }
    if paths::signature(&source)?.size != row.signature.size {
        return Err("源文件大小在预览后发生变化，请重新生成计划".into());
    }
    if row.replacement.is_some() && crate::resources::regular(&source, None)? != row.signature {
        return Err("替代文件已变化，请重新生成替换计划".into());
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    paths::safe_target_entry(&target, root)?;
    check_destination(p, index, &target, true)?;
    if row.operation == "move" && row.remote.as_ref().is_none_or(|r| r.method == "smb_move") {
        let file = remote::LockedFile::open(&source, true)?;
        if row.replacement.is_some()
            && row.remote.is_none()
            && paths::signature(&source)? != row.signature
        {
            return Err("替代文件在加锁前发生变化，请重新生成预览".into());
        }
        let stamp = file.stamp()?;
        if row
            .remote
            .as_ref()
            .is_some_and(|r| r.source.size != stamp.size)
        {
            return Err("远端源文件大小与移动检查点不一致".into());
        }
        if paths::signature(&source)?.size != row.signature.size {
            return Err("源文件大小在加锁前发生变化".into());
        }
        let hash = if p.verify_hash {
            measured_digest(store, &source)?
        } else {
            String::new()
        };
        p.rows[index].remote = Some(RemoteTransfer {
            method: "smb_move".into(),
            source: stamp.clone(),
            target: stamp,
            ready: true,
            sha256: hash,
        });
        p.rows[index].state = "committing".into();
        save(store, p)?;
        if row.action == "replace" {
            backup_existing(store, p, index)?;
        }
        remote::check_cancel(&store.cancel)?;
        paths::safe_target_entry(&target, root)?;
        match file.rename(&target) {
            Ok(()) => {
                let proof = p.rows[index].remote.as_ref().unwrap();
                if file.stamp()?.size != row.signature.size {
                    return Err("远端移动后文件大小与记录不一致".into());
                }
                if !proof.sha256.is_empty() && measured_digest(store, &target)? != proof.sha256 {
                    return Err("移动后 SHA-256 校验失败".into());
                }
                return complete_row(store, p, index);
            }
            Err(e) if matches!(e.raw_os_error(), Some(1 | 17 | 50 | 87)) => {
                // Cross-volume/share renames may be unsupported. Try SMB COPYCHUNK, never a client relay.
                p.rows[index].remote = None;
                save(store, p)?;
            }
            Err(e) => return Err(format!("远端移动失败，源文件已保留：{e}")),
        }
    }
    let previous = p.rows[index].remote.clone();
    p.rows[index].stage = paths::display(&stage);
    p.rows[index].state = "staging".into();
    save(store, p)?;
    let session = remote::prepare_copy(
        &source,
        &stage,
        row.operation == "move",
        &row.signature,
        previous.as_ref(),
        p.verify_hash,
        &store.cancel,
        |proof| {
            p.rows[index].remote = Some(proof.clone());
            if proof.ready {
                p.rows[index].state = "staged".into();
            }
            save(store, p)
        },
    )?;
    remote::check_cancel(&store.cancel)?;
    if row.action == "replace" {
        backup_existing(store, p, index)?;
    }
    remote::check_cancel(&store.cancel)?;
    paths::safe_target_entry(&target, root)?;
    p.rows[index].state = "committing".into();
    save(store, p)?;
    session
        .stage
        .rename(&target)
        .map_err(|e| format!("无法提交远端文件：{e}"))?;
    if session.stage.stamp()?.size != session.proof.target.size {
        return Err("远端提交后文件大小与记录不一致".into());
    }
    if row.operation == "move" {
        remote::check_cancel(&store.cancel)?;
        p.rows[index].source_cleanup_pending = true;
        p.rows[index].state = "removing_source".into();
        save(store, p)?;
        session.source.delete()?;
    }
    complete_row(store, p, index)
}
fn verify_link(row: &PlanRow, target: &Path, source: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(target).map_err(|e| e.to_string())?;
    if row.operation == "symlink" {
        let link = fs::read_link(target).map_err(|e| e.to_string())?;
        if paths::key(&link) != paths::key(source) {
            return Err("软链接指向已变化，停止操作".into());
        }
    } else if paths::is_link(&metadata) {
        return Err("目标被替换为链接，停止操作".into());
    }
    Ok(())
}
fn finish_transfer(store: &Store, p: &mut Plan, index: usize) -> Result<()> {
    let row = p.rows[index].clone();
    if row.operation == "move" {
        let target = Path::new(&row.target);
        if measured_digest(store, target)? != row.digest
            || paths::signature(target)?.size != row.signature.size
        {
            return Err("目标校验失败，已保留源文件".into());
        }
        let source = Path::new(&row.source);
        match fs::symlink_metadata(source) {
            Ok(m) => {
                if paths::is_link(&m)
                    || paths::signature(source)? != row.signature
                    || measured_digest(store, source)? != row.digest
                {
                    return Err("源文件在移动期间发生变化，已保留源文件与目标文件".into());
                }
                if store.cancel.load(Ordering::Relaxed) {
                    return Err("任务已暂停，源文件尚未删除".into());
                }
                p.rows[index].source_cleanup_pending = true;
                p.rows[index].state = "removing_source".into();
                save(store, p)?;
                fs::remove_file(source).map_err(|e| {
                    format!("目标已校验，但无法删除源文件；重试将继续完成移动：{e}")
                })?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && row.source_cleanup_pending => {}
            Err(e) => return Err(format!("无法确认移动源文件状态：{e}")),
        }
    }
    complete_row(store, p, index)
}
fn copy_row(store: &Store, p: &mut Plan, index: usize) -> Result<()> {
    store.diagnostics.measure("file.transfer", serde_json::json!({"task_id":p.id,"row_id":p.rows[index].id,"operation":p.rows[index].operation,"bytes":p.rows[index].signature.size}), || {

    let row = p.rows[index].clone();
    // Hold the existing video while importing its subtitle, including SMB transfers.
    #[cfg(windows)]
    let _subtitle_video = row.subtitle.as_ref()
        .map(|c| crate::remote_transfer::LockedFile::open(Path::new(&c.asset.path), false))
        .transpose()?;
    crate::subtitles::validate_row(store, p, &row)?;
    let source = paths::normalize(&row.source)?;
    let target = paths::normalize(&row.target)?;
    let work = p
        .works
        .iter()
        .find(|w| w.id == row.work_id)
        .ok_or("条目快照不存在")?;
    let root = paths::normalize(if let Some(replacement) = &row.replacement {
        &replacement.root
    } else if let Some(subtitle) = &row.subtitle {
        &subtitle.root
    } else if row.extra {
        &p.extras_root
    } else if work.kind == "movie" {
        &p.movie_root
    } else if work.kind == "music" {
        &p.music_root
    } else {
        &p.tv_root
    })?;
    paths::safe_target_entry(&target, &root)?;
    if row.operation == "adopt" {
        #[cfg(windows)]
        let _lock = crate::remote_transfer::LockedFile::open(&source, false)?;
        if crate::resources::regular(&source, Some(&root))? != row.signature {
            return Err("已在目标位置的新文件发生变化，请重新预览".into());
        }
        if p.verify_hash || !cfg!(windows) || !crate::resources::is_remote_path(&source) {
            p.rows[index].digest = measured_digest(store, &source)?;
        }
        save(store, p)?;
        backup_existing(store, p, index)?;
        if store.cancel.load(Ordering::Relaxed) {
            return Err("任务已暂停，备份和新文件均保留".into());
        }
        if paths::signature(&source)? != row.signature {
            return Err("接纳期间新文件发生变化".into());
        }
        return complete_row(store, p, index);
    }
    if row.replacement.is_some()
        && row.state == "pending"
        && paths::signature(&source)? != row.signature
    {
        return Err("替代文件在预览后发生变化，请重新生成预览".into());
    }
    #[cfg(windows)]
    if row.remote.is_some()
        || (["copy", "move"].contains(&row.operation.as_str())
            && row.digest.is_empty()
            && crate::remote_transfer::pair(&source, &target))
    {
        return remote_row(store, p, index, &root);
    }
    if recover_moved_file(store, p, index, &source, &target)? {
        return Ok(());
    }
    // Copies and links still use the saved content digest for interrupted commits.
    if !row.digest.is_empty()
        && !row.stage.is_empty()
        && fs::symlink_metadata(&row.stage).is_err()
        && target.try_exists().map_err(|e| e.to_string())?
        && measured_digest(store, &target)? == row.digest
    {
        verify_link(&row, &target, &source)?;
        return finish_transfer(store, p, index);
    }
    if paths::is_link(&fs::symlink_metadata(&source).map_err(|e| e.to_string())?) {
        return Err("源文件是链接或重解析点，请选择原始文件".into());
    }
    if paths::signature(&source)? != row.signature {
        return Err("源文件在预览后发生变化，请重新扫描并生成计划".into());
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| format!("无法创建目标目录：{e}"))?;
    paths::safe_target_entry(&target, &root)?;
    let stage = target
        .parent()
        .unwrap()
        .join(format!(".bm-{}-{}.partial", p.id, row.id));
    let reuse_stage = !row.digest.is_empty()
        && stage.exists()
        && paths::signature(&stage)?.size == row.signature.size
        && measured_digest(store, &stage)? == row.digest;
    if !reuse_stage {
        if fs::symlink_metadata(&stage).is_ok() {
            fs::remove_file(&stage).map_err(|e| e.to_string())?;
        }
        p.rows[index].stage = paths::display(&stage);
        p.rows[index].state = "staging".into();
        p.rows[index].digest.clear();
        save(store, p)?;
        let (bytes, hash) = match row.operation.as_str() {
            "hardlink" => {
                fs::hard_link(&source, &stage).map_err(|e| {
                    format!("无法创建硬链接（源和目标需位于同一文件系统，Samba 共享需支持）：{e}")
                })?;
                (row.signature.size, measured_digest(store, &source)?)
            }
            "symlink" => {
                paths::symlink_file(&source, &stage)?;
                (row.signature.size, measured_digest(store, &source)?)
            }
            _ => stream_copy(store, &source, &stage)?,
        };
        if bytes != row.signature.size
            || paths::signature(&stage)?.size != bytes
            || paths::signature(&source)? != row.signature
        {
            return Err("复制大小或源文件状态发生变化，未提交目标文件".into());
        }
        if (p.verify_hash || row.operation == "move") && measured_digest(store, &stage)? != hash {
            return Err("SHA-256 校验失败，未提交目标文件".into());
        }
        p.rows[index].digest = hash;
        p.rows[index].state = "staged".into();
        save(store, p)?;
    }
    verify_link(&p.rows[index], &stage, &source)?;
    crate::subtitles::validate_row(store, p, &row)?;
    check_destination(p, index, &target, false)?;
    if store.cancel.load(Ordering::Relaxed) {
        return Err("任务已停止，临时文件已保留".into());
    }
    if row.action == "replace" {
        backup_existing(store, p, index)?;
    }
    if store.cancel.load(Ordering::Relaxed) {
        return Err("任务已取消；备份与临时文件已保留".into());
    }
    p.rows[index].state = "committing".into();
    save(store, p)?;
    paths::safe_target_entry(&target, &root)?;
    paths::commit_no_replace(&stage, &target)?;
    finish_transfer(store, p, index)

    })
}
fn cleanup_source_directories(store: &Store, p: &Plan) -> Vec<String> {
    if store.cancel.load(Ordering::Relaxed) {
        return vec![];
    }
    let mut notices = vec![];
    let mut parents = std::collections::BTreeMap::new();
    for row in p.rows.iter().filter(|r| {
        r.state == "complete"
            && r.operation == "move"
            && r.action != "skip"
            && r.replacement.is_none()
    }) {
        match paths::normalize(&row.source) {
            Ok(source) => {
                if let Some(parent) = source.parent().filter(|p| p.file_name().is_some()) {
                    parents.insert(paths::key(parent), parent.to_path_buf());
                }
            }
            Err(error) => notices.push(format!("无法检查源目录：{}：{error}", row.source)),
        }
    }
    let mut protected = vec![store.data_dir.clone()];
    for raw in [
        &p.tv_root,
        &p.movie_root,
        &p.music_root,
        &p.extras_root,
        &p.backup_root,
    ] {
        if let Ok(root) = paths::normalize(raw) {
            protected.push(root);
        }
    }
    if let Some(settings) = &p.settings {
        if let Ok(root) = paths::normalize(&settings.output_root) {
            protected.push(root);
        }
    }
    // Only immediate parents recorded in this task are candidates; never walk up to a share/root.
    let mut parents: Vec<_> = parents.into_values().collect();
    parents.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for parent in parents {
        if store.cancel.load(Ordering::Relaxed) {
            break;
        }
        if protected
            .iter()
            .any(|root| paths::key(root) == paths::key(&parent))
        {
            continue;
        }
        if let Err(error) = remove_empty_source_directory(&parent) {
            notices.push(format!(
                "无法清理空源目录，已保留：{}：{error}",
                paths::display(&parent)
            ));
        }
    }
    notices
}
fn remove_empty_source_directory(path: &Path) -> std::io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || paths::is_link(&metadata) {
        return Ok(());
    }
    // Includes hidden files and directories. remove_dir itself also refuses a racing new file.
    if fs::read_dir(path)?.next().transpose()?.is_some() {
        return Ok(());
    }
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}
pub fn execute(store: Arc<Store>, plan_id: String, progress: impl Fn(&Plan)) -> Result<Plan> {
    store.diagnostics.measure_with(
        "task.execute",
        serde_json::json!({"task_id":plan_id}),
        |timing| {
            let mut p = store.get::<Plan>("plans", &plan_id)?.ok_or("计划不存在")?;
            preflight(&store, &p)?;
            p.state = "running".into();
            save(&store, &mut p)?;
            progress(&p);
            if p.kind == "cue_split" {
                crate::cue::transfer(&store, &mut p, &progress)?;
            } else if p.kind == "rename" {
                crate::renaming::transfer(&store, &mut p, &progress)?;
            } else {
            for i in 0..p.rows.len() {
                if store.cancel.load(Ordering::Relaxed) {
                    break;
                }
                if p.rows[i].state == "complete" || p.rows[i].state == "skipped" {
                    continue;
                }
                if p.rows[i].action == "skip" {
                    p.rows[i].state = "skipped".into();
                    save(&store, &mut p)?;
                    continue;
                }
                if let Some(parent) = &p.rows[i].parent_id {
                    if p.rows
                        .iter()
                        .any(|r| &r.id == parent && r.state != "complete")
                    {
                        continue;
                    }
                }
                if let Err(error) = copy_row(&store, &mut p, i) {
                    p.rows[i].error = error;
                    p.rows[i].state = "failed".into();
                    save(&store, &mut p)?;
                    progress(&p);
                    break;
                }
                progress(&p);
            }
            // Filename ordering may place a subtitle before its video. A second pass handles those attachments.
            for i in 0..p.rows.len() {
                if store.cancel.load(Ordering::Relaxed)
                    || p.rows.iter().any(|r| r.state == "failed")
                {
                    break;
                }
                if p.rows[i].attachment
                    && p.rows[i].state == "pending"
                    && p.rows[i].action != "skip"
                {
                    if let Some(parent) = &p.rows[i].parent_id {
                        if p.rows
                            .iter()
                            .any(|r| &r.id == parent && r.state == "complete")
                        {
                            if let Err(error) = copy_row(&store, &mut p, i) {
                                p.rows[i].error = error;
                                p.rows[i].state = "failed".into();
                                save(&store, &mut p)?;
                            }
                            progress(&p);
                        }
                    }
                }
            }
            }
            p.metadata_errors.clear();
            let mut metadata_notices = vec![];
            let mut seasons = crate::metadata::SeasonMetadataBatch::default();
            let mut metadata_works = HashSet::new();
            for row in p
                .rows
                .iter()
                .filter(|r| r.state == "complete" && !r.extra && !r.attachment
                    && (p.kind != "rename" || p.rows.iter().all(|r| r.state == "complete")))
            {
                // Keep shared NFO snapshots intact until every replacement for this work has a durable backup.
                if row.replacement.is_some()
                    && p.rows.iter().any(|r| {
                        r.work_id == row.work_id && r.action != "skip" && r.state != "complete"
                    })
                {
                    continue;
                }
                if let Some(work) = p.works.iter().find(|w| w.id == row.work_id) {
                    if let Err(e) = crate::metadata::write_metadata(
                        &store,
                        &p,
                        work,
                        row,
                        metadata_works.insert(work.id.clone()),
                        &mut metadata_notices,
                        &mut seasons,
                    ) {
                        p.metadata_errors.push(format!("{}：{e}", row.target));
                    }
                }
            }
            p.metadata_errors
                .extend(seasons.flush(&store, &mut metadata_notices));
            p.cleanup_notices = cleanup_source_directories(&store, &p);
            metadata_notices.sort();
            metadata_notices.dedup();
            p.metadata_notices = metadata_notices;
            if p.kind != "rename" || p.rows.iter().all(|r| r.state == "complete") {
                let indexed = {
                    let related = p.works.iter().map(|work| {
                        let current = store.get::<Work>("works", &work.id)?.ok_or("关联条目不存在")?;
                        let folders = p.rows.iter().filter(|r| r.work_id == work.id && r.state == "complete" && !r.extra && !r.media_folder.is_empty())
                            .map(|r| PathBuf::from(&r.media_folder)).collect();
                        Ok((current, folders))
                    }).collect::<Result<Vec<_>>>();
                    related.and_then(|works| scan::index_works(&store, &works))
                };
                if let Err(e) = indexed {
                    p.metadata_errors.push(e);
                }
            }
            p.state = if store.cancel.load(Ordering::Relaxed) {
                "paused"
            } else if p
                .rows
                .iter()
                .any(|r| r.state != "complete" && r.state != "skipped")
            {
                "failed"
            } else if !p.metadata_errors.is_empty() {
                "metadata_failed"
            } else {
                "complete"
            }
            .into();
            crate::cue::finish_cleanup(&store, &mut p)?;
            save(&store, &mut p)?;
            for row in p
                .rows
                .iter()
                .filter(|r| r.state == "complete" && !r.attachment)
            {
                if let (Some(probe), Ok(signature)) =
                    (&row.probe, paths::signature(Path::new(&row.target)))
                {
                    if probe.error.is_none() && probe.version == scan::PROBE_VERSION {
                        store.put(
                            "probes",
                            &scan::probe_key(&store, Path::new(&row.target), &signature)?,
                            probe,
                        )?;
                    }
                }
            }
            crate::backups::register_plan(&store, &p)?;
            crate::cue::clear_completed_sources(&store, &p)?;
            progress(&p);
            timing.field("state", &p.state);
            timing.finish(match p.state.as_str() {
                "complete" => "ok",
                "paused" | "cancelled" => "cancelled",
                _ => "error",
            });
            Ok(p)
        },
    )
}
pub fn export_plan(store: &Store, id: &str, raw: &str) -> Result<()> {
    let p = store.get::<Plan>("plans", id)?.ok_or("计划不存在")?;
    let target = paths::normalize(raw)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| format!("请选一个尚不存在的 JSON 文件：{e}"))?;
    f.write_all(&serde_json::to_vec_pretty(&p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    mod inline_import {
        use super::*;
        use crate::organize_preview as preview;

        fn choices(plan: &Plan, action: &str) -> Vec<Decision> {
            plan.rows.iter().filter(|r| r.parent_id.is_none()).map(|r| Decision {
                id: r.id.clone(), action: action.into(), operation: "copy".into(), reviewed: true,
            }).collect()
        }

        #[test]
        fn preview_does_not_create_task_and_submit_is_idempotent() {
            let f = Fixture::new();
            let m = f.media("Fictional.E01.mkv", b"media", "1");
            let p = preview::preview(&f.store, vec![m], |_, _| {}).unwrap();
            assert!(f.store.list::<Plan>("plans").unwrap().is_empty());
            assert!(!Path::new(&p.rows[0].target).exists());
            let created = preview::submit(&f.store, &p.id, choices(&p, "copy")).unwrap();
            assert_eq!(created.id, p.id);
            assert_eq!(created.state, "draft");
            assert!(!Path::new(&p.rows[0].target).exists());
            assert!(f.store.get::<Plan>("organize_previews", &p.id).unwrap().is_none());
            assert_eq!(preview::submit(&f.store, &p.id, vec![]).unwrap().id, p.id);
            assert_eq!(f.store.list::<Plan>("plans").unwrap().len(), 1);
            assert_eq!(f.run(&created).state, "complete");
        }

        #[test]
        fn stale_preview_refuses_source_sidecar_target_and_work_changes() {
            for scenario in 0..4 {
                let f = Fixture::new();
                let m = f.media("Fictional.E01.mkv", b"media", "1");
                let p = preview::preview(&f.store, vec![m.clone()], |_, _| {}).unwrap();
                match scenario {
                    0 => fs::write(&m.source, b"changed source").unwrap(),
                    1 => fs::write(Path::new(&m.source).with_extension("zh.ass"), b"new subtitle").unwrap(),
                    2 => {
                        fs::create_dir_all(Path::new(&p.rows[0].target).parent().unwrap()).unwrap();
                        fs::write(&p.rows[0].target, b"another file").unwrap();
                    }
                    _ => {
                        let mut w = work(); w.episodes[0].number = 5;
                        f.store.put("works", &w.id, &w).unwrap();
                    }
                }
                assert!(preview::submit(&f.store, &p.id, choices(&p, "copy")).is_err());
                assert!(f.store.list::<Plan>("plans").unwrap().is_empty());
                assert!(f.store.get::<Plan>("organize_previews", &p.id).unwrap().is_some());
                assert!(Path::new(&m.source).exists());
            }
        }

        #[test]
        fn preview_lists_old_sidecars_and_requires_explicit_replacement() {
            let f = Fixture::new();
            let old = f.media("Old.E01.mkv", b"old video", "1");
            let old_plan = f.plan(vec![old], "copy");
            f.run(&old_plan);
            let old_sub = Path::new(&old_plan.rows[0].target).with_extension("en.srt");
            fs::write(&old_sub, b"old subtitles").unwrap();
            let m = f.media("New.E01.mkv", b"new video", "1");
            let p = preview::preview(&f.store, vec![m.clone()], |_, _| {}).unwrap();
            assert!(p.rows[0].backups.iter().any(|b| b.original == paths::display(&old_sub)));
            assert!(p.rows[0].backups.iter().any(|b| b.original.ends_with(".nfo")));
            assert!(preview::submit(&f.store, &p.id, choices(&p, "copy")).is_err());
            fs::write(&old_sub, b"changed subtitles").unwrap();
            assert!(preview::submit(&f.store, &p.id, choices(&p, "replace")).is_err());
            let fresh = preview::preview(&f.store, vec![m], |_, _| {}).unwrap();
            let ready = preview::submit(&f.store, &fresh.id, choices(&fresh, "replace")).unwrap();
            let done = f.run(&ready);
            assert_eq!(done.state, "complete");
            assert!(done.rows[0].backups.iter().any(|b| fs::read(&b.backup).unwrap() == b"changed subtitles"));
        }

        #[test]
        fn extras_preview_preserves_recognized_directory_structure() {
            let f = Fixture::new();
            let folder = f.source.join("Bonus").join("Book");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("page.jpg"), b"image").unwrap();
            fs::write(folder.join("clip.srt"), b"subtitle extra").unwrap();
            let files = scan::scan_sources(&f.store, vec![paths::display(&f.source)], |_| {}).unwrap().files;
            let mappings = files.into_iter().map(|file| Mapping {
                part: 0,
                source: file.path, work_id: "show".into(), episode_ids: vec![], extra: true,
            }).collect();
            let p = preview::preview(&f.store, mappings, |_, _| {}).unwrap();
            assert_eq!(p.rows.len(), 2);
            assert!(p.rows.iter().all(|r| r.extra && r.episode_ids.is_empty() && r.target.contains("Book")));
            let ready = preview::submit(&f.store, &p.id, choices(&p, "copy")).unwrap();
            assert_eq!(f.run(&ready).state, "complete");
            assert!(scan::index_library(&f.store).unwrap().assets.is_empty());
        }
    }
    fn work() -> Work {
        Work {
            info: MediaInfo::default(),
            seasons: vec![],
            category: String::new(),
            metadata_errors: vec![],
            metadata_notices: vec![],
            id: "show".into(),
            title: "FICTIONAL VOYAGE".into(),
            kind: "tv".into(),
            year: "2011".into(),
            folder_name: "FICTIONAL VOYAGE".into(),
            tmdb_id: "".into(),
            parent_id: "".into(),
            overview: "".into(),
            poster: "".into(),
            bindings: vec![],
            episodes: (1..=3)
                .map(|n| Episode {
                    info: MediaInfo::default(),
                    id: n.to_string(),
                    binding_id: "b".into(),
                    remote_id: n.to_string(),
                    sort: n as f64,
                    title: "".into(),
                    kind: 0,
                    season: 2,
                    number: n,
                    enabled: true,
                })
                .collect(),
        }
    }
    #[test]
    fn ranges_require_contiguous_episodes() {
        let w = work();
        assert!(episode_range(&w, &["1".into(), "3".into()]).is_err());
        assert_eq!(
            episode_range(&w, &["2".into(), "1".into()]).unwrap()[0].number,
            1
        );
    }
    #[test]
    fn season_and_media_filename() {
        let d = tempfile::tempdir().unwrap();
        let s = Settings {
            tv_root: paths::display(d.path()),
            ..Default::default()
        };
        let m = Mapping {
            part: 0,
            source: paths::display(&d.path().join("[FICTIONAL_VOYAGE_2][01].mkv")),
            work_id: "show".into(),
            episode_ids: vec!["1".into(), "2".into()],
            extra: false,
        };
        assert!(paths::display(&destination(&s, &work(), &m).unwrap())
            .replace('\\', "/")
            .ends_with("FICTIONAL VOYAGE/Season 02/S02E01-E02.mkv"));
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        source: PathBuf,
        tv: PathBuf,
        backup: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let d = tempfile::tempdir().unwrap();
            let store = Store::open(d.path().join("state")).unwrap();
            let source = d.path().join("incoming");
            let tv = d.path().join("TV Shows");
            let backup = d.path().join("Backups");
            let movies = d.path().join("Movies");
            let extras = d.path().join("Extras");
            for p in [&source, &tv, &backup, &movies, &extras] {
                fs::create_dir_all(p).unwrap();
            }
            let s = Settings {
                tv_root: paths::display(&tv),
                movie_root: paths::display(&movies),
                extras_root: paths::display(&extras),
                backup_root: paths::display(&backup),
                ffprobe: "__ffprobe_not_installed_for_test__".into(),
                verify_hash: true,
                ..Default::default()
            };
            store.put("settings", "main", &s).unwrap();
            store.put("works", "show", &work()).unwrap();
            Self {
                _dir: d,
                store,
                source,
                tv,
                backup,
            }
        }
        fn media(&self, name: &str, content: &[u8], episode: &str) -> Mapping {
            let path = self.source.join(name);
            fs::write(&path, content).unwrap();
            Mapping {
                part: 0,
                source: paths::display(&path),
                work_id: "show".into(),
                episode_ids: vec![episode.into()],
                extra: false,
            }
        }
        fn plan(&self, m: Vec<Mapping>, action: &str) -> Plan {
            let p = create_plan(&self.store, m, |_, _| {}).unwrap();
            let choices = p
                .rows
                .iter()
                .map(|r| Decision {
                    id: r.id.clone(),
                    action: action.into(),
                    operation: "copy".into(),
                    reviewed: true,
                })
                .collect();
            decisions(&self.store, &p.id, choices).unwrap()
        }
        fn run(&self, p: &Plan) -> Plan {
            execute(self.store.clone(), p.id.clone(), |_| {}).unwrap()
        }
    }
    #[test]
    fn preview_scans_actual_and_target_folders_without_reading_unrelated_works() {
        let f = Fixture::new();
        let old = f.tv.join("Previous folder");
        let unrelated = f.tv.join("Unrelated folder");
        for dir in [&old, &unrelated] {
            fs::create_dir_all(dir).unwrap();
        }
        let mut other = work();
        other.id = "other".into();
        f.store.put("works", &other.id, &other).unwrap();
        f.store.put("folders", &paths::key(&old), &"show").unwrap();
        f.store
            .put("folders", &paths::key(&unrelated), &"other")
            .unwrap();
        fs::write(old.join("S02E01.mkv"), b"old").unwrap();
        fs::write(unrelated.join("S02E01.mkv"), b"unrelated").unwrap();
        let mut before = scan::index_library(&f.store).unwrap();
        before.available = false;
        before.errors.push("Unrelated root is offline".into());
        f.store.put("index", "main", &before).unwrap();
        // A full scan would parse this invalid NFO and invalidate the other folder.
        fs::write(unrelated.join("tvshow.nfo"), b"<broken").unwrap();
        fs::write(old.join("S02E01.ts"), b"unindexed old version").unwrap();
        let target_folder = f.tv.join(work().folder_name);
        fs::create_dir_all(&target_folder).unwrap();
        f.store
            .put("folders", &paths::key(&target_folder), &"show")
            .unwrap();
        fs::write(target_folder.join("S02E01.mp4"), b"target version").unwrap();
        // Unrelated malformed database documents must not be decoded either.
        f.store
            .put("works", "unused", &serde_json::json!({"invalid": true}))
            .unwrap();
        let before_stats = f.store.diagnostics.report().statistics;
        let plan = create_plan(&f.store, vec![f.media("New.mkv", b"new", "1")], |_, _| {}).unwrap();
        assert_eq!(plan.rows[0].backups.len(), 3);
        let after: LibraryIndex = f.store.get("index", "main").unwrap().unwrap();
        assert_eq!(after.errors, before.errors);
        assert_eq!(after.scanned_at, before.scanned_at);
        assert!(!after.available);
        assert_eq!(
            serde_json::to_value(after.assets.iter().find(|a| a.work_id == "other")).unwrap(),
            serde_json::to_value(before.assets.iter().find(|a| a.work_id == "other")).unwrap()
        );
        for operation in ["library.scan", "db.list.works"] {
            let count = |stats: &[crate::diagnostics::Statistics]| {
                stats
                    .iter()
                    .find(|s| s.operation == operation)
                    .map_or(0, |s| s.count)
            };
            assert_eq!(
                count(&f.store.diagnostics.report().statistics),
                count(&before_stats)
            );
        }
        let related = f
            .store
            .diagnostics
            .report()
            .recent
            .into_iter()
            .find(|e| e.operation == "library.scan_related")
            .unwrap();
        assert_eq!(related.fields["folders"], 2);
        assert_eq!(related.fields["resources"], 3);
    }

    #[test]
    fn completed_task_refreshes_only_related_works() {
        let f = Fixture::new();
        let unrelated = f.tv.join("Unrelated folder");
        fs::create_dir_all(&unrelated).unwrap();
        let mut other = work();
        other.id = "other".into();
        f.store.put("works", &other.id, &other).unwrap();
        f.store
            .put("folders", &paths::key(&unrelated), &other.id)
            .unwrap();
        fs::write(unrelated.join("S02E01.mkv"), b"unrelated").unwrap();
        let before = scan::index_library(&f.store).unwrap();
        let plan = f.plan(vec![f.media("01.mkv", b"new", "1")], "copy");
        fs::write(unrelated.join("tvshow.nfo"), b"<broken").unwrap();
        let count = |stats: &[crate::diagnostics::Statistics]| {
            stats
                .iter()
                .find(|s| s.operation == "library.scan")
                .map_or(0, |s| s.count)
        };
        let scans = count(&f.store.diagnostics.report().statistics);
        let done = f.run(&plan);
        assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
        assert_eq!(count(&f.store.diagnostics.report().statistics), scans);
        let after: LibraryIndex = f.store.get("index", "main").unwrap().unwrap();
        assert!(after.assets.iter().any(|a| a.work_id == "show"));
        assert_eq!(
            serde_json::to_value(after.assets.iter().find(|a| a.work_id == "other")).unwrap(),
            serde_json::to_value(before.assets.iter().find(|a| a.work_id == "other")).unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn remote_backup_move_recovers_and_rejects_recreated_originals() {
        let f = Fixture::new();
        let mut plan = f.plan(vec![f.media("01.mkv", b"new", "1")], "copy");
        let original = f.tv.join("old.mkv");
        let backup = f.backup.join("moved.mkv");
        fs::write(&original, b"old content").unwrap();
        plan.rows[0].backups = vec![BackupEntry {
            original: paths::display(&original),
            backup: paths::display(&backup),
            signature: paths::signature(&original).unwrap(),
            preserve_original: false,
            remote: None,
            digest: String::new(),
            probe: None,
        }];
        fs::write(&backup, b"occupied").unwrap();
        assert!(remote_backup(&f.store, &mut plan, 0, 0).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"old content");
        assert_eq!(fs::read(&backup).unwrap(), b"occupied");
        fs::remove_file(&backup).unwrap();
        // Use the same Windows handle operations on an isolated local volume.
        remote_backup(&f.store, &mut plan, 0, 0).unwrap();
        assert!(!original.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"old content");
        assert_eq!(
            plan.rows[0].backups[0].remote.as_ref().unwrap().method,
            "smb_move"
        );
        remote_backup(&f.store, &mut plan, 0, 0).unwrap();
        fs::write(&original, b"replacement").unwrap();
        assert!(remote_backup(&f.store, &mut plan, 0, 0).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"replacement");
        fs::remove_file(&original).unwrap();
        fs::rename(&backup, backup.with_extension("saved")).unwrap();
        fs::write(&backup, b"old content").unwrap();
        assert!(remote_backup(&f.store, &mut plan, 0, 0).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn remote_backup_move_resumes_saved_intent_and_cancelled_operation() {
        let f = Fixture::new();
        let mut plan = f.plan(vec![f.media("01.mkv", b"new", "1")], "copy");
        let original = f.tv.join("old.mkv");
        let backup = f.backup.join("moved.mkv");
        fs::write(&original, b"old content").unwrap();
        plan.rows[0].backups = vec![BackupEntry {
            original: paths::display(&original),
            backup: paths::display(&backup),
            signature: paths::signature(&original).unwrap(),
            preserve_original: false,
            remote: None,
            digest: String::new(),
            probe: None,
        }];
        f.store.cancel.store(true, Ordering::Relaxed);
        assert!(remote_backup(&f.store, &mut plan, 0, 0).is_err());
        assert!(original.exists());
        assert!(!backup.exists());
        f.store.cancel.store(false, Ordering::Relaxed);
        let mut saved: Plan = f.store.get("plans", &plan.id).unwrap().unwrap();
        // Simulate termination after the rename but before the final checkpoint.
        crate::remote_transfer::LockedFile::open(&original, true)
            .unwrap()
            .rename(&backup)
            .unwrap();
        remote_backup(&f.store, &mut saved, 0, 0).unwrap();
        assert!(saved.rows[0].backups[0].remote.as_ref().unwrap().ready);
        assert_eq!(fs::read(&backup).unwrap(), b"old content");
    }

    #[test]
    fn preview_preserves_index_and_refuses_an_unavailable_related_folder() {
        let f = Fixture::new();
        let old = f.tv.join("Previous folder");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("S02E01.mkv"), b"old").unwrap();
        f.store.put("folders", &paths::key(&old), &"show").unwrap();
        let before = scan::index_library(&f.store).unwrap();
        fs::rename(&old, f.tv.join("Disconnected folder")).unwrap();
        let error =
            create_plan(&f.store, vec![f.media("New.mkv", b"new", "1")], |_, _| {}).unwrap_err();
        assert!(error.contains("相关作品目录不可用"), "{error}");
        let after: LibraryIndex = f.store.get("index", "main").unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
        assert!(f.store.list::<Plan>("plans").unwrap().is_empty());
    }

    #[test]
    fn preview_refuses_a_target_owned_by_another_work() {
        let f = Fixture::new();
        let target = f.tv.join(work().folder_name);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("S02E01.mkv"), b"other").unwrap();
        f.store
            .put("folders", &paths::key(&target), &"other")
            .unwrap();
        let error =
            create_plan(&f.store, vec![f.media("New.mkv", b"new", "1")], |_, _| {}).unwrap_err();
        assert!(error.contains("目录关联与所选作品不一致"), "{error}");
        assert_eq!(
            f.store
                .get::<String>("folders", &paths::key(&target))
                .unwrap()
                .as_deref(),
            Some("other")
        );
        assert!(f.store.list::<Plan>("plans").unwrap().is_empty());
    }

    #[test]
    fn multipart_manual_identity_survives_rescan_when_movie_title_contains_part() {
        for part in [0, 2] {
            let f = Fixture::new();
            let mut movie = work();
            movie.kind = "movie".into();
            movie.title = "Fictional Part1".into();
            movie.folder_name = movie.title.clone();
            movie.episodes.truncate(1);
            f.store.put("works", "show", &movie).unwrap();
            let mut mapping = f.media("Fictional Part1.mkv", b"movie", "1");
            mapping.part = part;
            assert_eq!(f.run(&f.plan(vec![mapping], "copy")).state, "complete");
            let reopened = Store::open(f.store.data_dir.clone()).unwrap();
            let index = scan::index_library(&reopened).unwrap();
            assert_eq!(index.assets.len(), 1);
            assert_eq!(index.assets[0].part, part);
            if part == 0 {
                let replacement = f.media("Fictional Part1.mp4", b"replacement", "1");
                let preview = crate::resources::preview(
                    &reopened,
                    vec![ReplacementRequest {
                        old_path: index.assets[0].path.clone(),
                        new_path: replacement.source,
                        replace_subtitles: false,
                    }],
                )
                .unwrap();
                assert_eq!(preview.rows[0].part, 0);
            }
        }
    }

    #[test]
    fn multipart_queue_accepts_other_parts_and_reserves_duplicate_or_whole_files() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let mut first = f.media("First.mkv", b"first", "1");
        first.part = 1;
        let mut second = f.media("Second.mkv", b"second", "1");
        second.part = 2;
        let mut duplicate = f.media("Duplicate.mp4", b"duplicate", "1");
        duplicate.part = 1;
        let a = f.plan(vec![first], "copy");
        let b = f.plan(vec![second], "copy");
        let duplicate = f.plan(vec![duplicate], "copy");
        let whole = f.plan(vec![f.media("Whole.mkv", b"whole", "1")], "copy");
        queue::enqueue(&f.store, &a.id).unwrap();
        queue::enqueue(&f.store, &b.id).unwrap();
        assert!(queue::enqueue(&f.store, &duplicate.id)
            .unwrap_err()
            .contains("重复"));
        assert!(queue::enqueue(&f.store, &whole.id)
            .unwrap_err()
            .contains("重复"));
        queue::set_paused(&f.store, false).unwrap();
        queue::drain(f.store.clone(), |_| {});
        for plan in [&a, &b] {
            assert_eq!(
                f.store
                    .get::<Plan>("plans", &plan.id)
                    .unwrap()
                    .unwrap()
                    .state,
                "complete"
            );
            assert!(Path::new(&plan.rows[0].target).exists());
        }
    }

    #[test]
    fn multipart_tv_import_and_replacement_preserve_other_parts_and_subtitles() {
        let f = Fixture::new();
        let mut first = f.media("Chapter.Part1.mkv", b"first part", "1");
        first.part = 1;
        let mut second = f.media("Chapter.Part2.mkv", b"second part", "1");
        second.part = 2;
        fs::write(f.source.join("Chapter.Part1.chs.ass"), b"first captions").unwrap();
        fs::write(f.source.join("Chapter.Part2.chs.ass"), b"second captions").unwrap();
        let p = f.plan(vec![first, second], "copy");
        assert!(p.rows.iter().all(|r| r.conflicts.is_empty()));
        let done = f.run(&p);
        assert_eq!(
            done.state,
            "complete",
            "{:?}",
            done.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
        );
        assert_eq!(done.rows.len(), 4);
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.assets.len(), 2);
        assert!(index.assets.iter().all(|a| a.number == 1 && a.end == 1));
        let first_path = index
            .assets
            .iter()
            .find(|a| a.part == 1)
            .unwrap()
            .path
            .clone();
        let second_path = index
            .assets
            .iter()
            .find(|a| a.part == 2)
            .unwrap()
            .path
            .clone();
        let mut newer = f.media("New.Part2.mp4", b"new second part", "1");
        newer.part = 2;
        let replacement = f.plan(vec![newer], "replace");
        assert!(replacement.rows[0]
            .backups
            .iter()
            .any(|b| b.original == second_path));
        assert!(replacement
            .rows
            .iter()
            .flat_map(|r| &r.backups)
            .all(|b| b.original != first_path));
        assert_eq!(f.run(&replacement).state, "complete");
        assert_eq!(fs::read(&first_path).unwrap(), b"first part");
        let first_sub = Path::new(&first_path).with_extension("chs.ass");
        assert_eq!(fs::read(first_sub).unwrap(), b"first captions");
        assert!(!Path::new(&second_path).exists());
        let unsplit = f.media("Whole.mkv", b"whole file", "1");
        assert!(create_plan(&f.store, vec![unsplit], |_, _| {})
            .unwrap_err()
            .contains("混用"));
    }

    #[test]
    fn multipart_movie_files_share_one_movie_without_colliding() {
        for templates in [false, true] {
            let f = Fixture::new();
            if templates {
                let mut settings = f.store.settings().unwrap();
                settings.output_root = paths::display(&f._dir.path().join("Library"));
                fs::create_dir_all(&settings.output_root).unwrap();
                f.store.put("settings", "main", &settings).unwrap();
            }
            let mut movie = work();
            movie.kind = "movie".into();
            movie.episodes.truncate(1);
            f.store.put("works", "show", &movie).unwrap();
            let mappings = (1..=3)
                .map(|part| {
                    let mut m = f.media(
                        &format!("Fictional Feature - {part:02}.mkv"),
                        &[part as u8],
                        "1",
                    );
                    m.part = part;
                    m
                })
                .collect();
            let p = f.plan(mappings, "copy");
            assert_eq!(p.rows.len(), 3);
            for row in &p.rows {
                assert!(row.target.ends_with(&format!(" - part{}.mkv", row.part)));
                assert!(row.conflicts.is_empty());
            }
            assert_eq!(f.run(&p).state, "complete");
            assert_eq!(scan::index_library(&f.store).unwrap().assets.len(), 3);
        }
    }

    #[test]
    fn multipart_rejects_duplicate_parts_ranges_and_whole_file_overlap() {
        let f = Fixture::new();
        let mut a = f.media("A.mkv", b"a", "1");
        let mut b = f.media("B.mkv", b"b", "1");
        a.part = 1;
        b.part = 1;
        let p = create_plan(&f.store, vec![a.clone(), b.clone()], |_, _| {}).unwrap();
        assert!(p.rows.iter().all(|r| !r.conflicts.is_empty()));
        let mut p = p;
        for row in &mut p.rows {
            row.action = "copy".into();
        }
        assert!(validate_plan(&p).is_err());
        b.part = 0;
        let mut p = create_plan(&f.store, vec![a.clone(), b], |_, _| {}).unwrap();
        assert!(p.rows.iter().all(|r| !r.conflicts.is_empty()));
        for row in &mut p.rows {
            row.action = "copy".into();
        }
        assert!(validate_plan(&p).is_err());
        a.episode_ids.push("2".into());
        assert!(create_plan(&f.store, vec![a], |_, _| {})
            .unwrap_err()
            .contains("Part 编号"));
    }

    #[test]
    fn multipart_preview_submission_keeps_part_identity() {
        let f = Fixture::new();
        let mut mapping = f.media("Chapter.mkv", b"part", "1");
        mapping.part = 2;
        let preview = crate::organize_preview::preview(&f.store, vec![mapping], |_, _| {}).unwrap();
        let choices = preview
            .rows
            .iter()
            .map(|r| Decision {
                id: r.id.clone(),
                action: "copy".into(),
                operation: "copy".into(),
                reviewed: true,
            })
            .collect();
        let plan = crate::organize_preview::submit(&f.store, &preview.id, choices).unwrap();
        assert_eq!(plan.rows[0].part, 2);
        assert_eq!(f.run(&plan).state, "complete");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Uses an isolated UUID directory under BANGUMI_SMB_TEST_ROOT"]
    fn live_smb_tasks_copy_move_backup_and_resume() {
        let base = fs::canonicalize(std::env::var("BANGUMI_SMB_TEST_ROOT").unwrap()).unwrap();
        let root = base.join(format!(".bangumi-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let mut f = Fixture::new();
        f.source = root.join("incoming");
        f.tv = root.join("tv");
        f.backup = root.join("backup");
        for dir in [&f.source, &f.tv, &f.backup] {
            fs::create_dir(dir).unwrap();
        }
        let mut settings = f.store.settings().unwrap();
        settings.tv_root = paths::display(&f.tv);
        settings.backup_root = paths::display(&f.backup);
        settings.verify_hash = false;
        f.store.put("settings", "main", &settings).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let first = f.media("01.mkv", &vec![0x41; 1024 * 1024 + 13], "1");
            fs::write(f.source.join("01.zh.ass"), b"subtitle").unwrap();
            let p = f.run(&f.plan(vec![first], "copy"));
            assert_eq!(
                p.state,
                "complete",
                "{:?}",
                p.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
            );
            assert_eq!(p.rows.len(), 2);
            assert!(p.rows.iter().all(|r| r
                .remote
                .as_ref()
                .is_some_and(|v| v.method == "smb_copy" && v.sha256.is_empty())));
            let replacement = f.plan(vec![f.media("01.webm", b"replacement", "1")], "replace");
            let p = f.run(&replacement);
            assert_eq!(
                p.state,
                "complete",
                "{:?}",
                p.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
            );
            assert!(p.rows[0]
                .backups
                .iter()
                .all(|b| b.remote.as_ref().is_some_and(|v| v.ready)));
            assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"replacement");
            let mapping = f.media("02.mkv", b"move", "2");
            fs::write(f.source.join("02.ja.srt"), b"subtitle").unwrap();
            let p = f.run(&operation_plan(&f, vec![mapping], "move", "copy"));
            assert_eq!(
                p.state,
                "complete",
                "{:?}",
                p.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
            );
            assert!(p
                .rows
                .iter()
                .all(|r| r.remote.as_ref().is_some_and(|v| v.method == "smb_move")));
            assert!(p.rows.iter().all(|r| !Path::new(&r.source).exists()));
            let mut p = f.plan(vec![f.media("03.mkv", b"interrupted", "3")], "copy");
            let source = PathBuf::from(&p.rows[0].source);
            let target = PathBuf::from(&p.rows[0].target);
            let stage = target
                .parent()
                .unwrap()
                .join(format!(".bm-{}-{}.partial", p.id, p.rows[0].id));
            let signature = p.rows[0].signature.clone();
            p.rows[0].stage = paths::display(&stage);
            let session = crate::remote_transfer::prepare_copy(
                &source,
                &stage,
                false,
                &signature,
                None,
                false,
                &f.store.cancel,
                |proof| {
                    p.rows[0].remote = Some(proof.clone());
                    p.rows[0].state = "committing".into();
                    save(&f.store, &mut p)
                },
            )?;
            session.stage.rename(&target).map_err(|e| e.to_string())?;
            drop(session);
            let reopened = Store::open(f.store.data_dir.clone())?;
            let resumed = execute(reopened, p.id, |_| {})?;
            assert_eq!(
                resumed.state,
                "complete",
                "{:?}",
                resumed.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
            );
            assert_eq!(fs::read(target).unwrap(), b"interrupted");
            let empty_after_move = f.source.join("empty-after-move");
            fs::create_dir(&empty_after_move).unwrap();
            let mapping = f.media("empty-after-move/03.webm", b"move and clean", "3");
            let p = f.run(&operation_plan(&f, vec![mapping], "move", "replace"));
            assert_eq!(p.state, "complete", "{:?} {:?}", p.rows, p.metadata_errors);
            assert!(p.cleanup_notices.is_empty(), "{:?}", p.cleanup_notices);
            assert!(!empty_after_move.exists());
            assert!(f.source.is_dir());
            Ok(())
        }));
        // Verify the absolute, newly created directory before enumerating its own test files for cleanup.
        let resolved = fs::canonicalize(&root).unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        assert!(root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".bangumi-test-"));
        let mut entries: Vec<_> = walkdir::WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .map(|e| e.unwrap())
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.depth()));
        for entry in entries {
            assert!(entry.path().starts_with(&root));
            if entry.file_type().is_dir() {
                fs::remove_dir(entry.path()).unwrap();
            } else {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        result.unwrap().unwrap();
    }
    #[test]
    fn attached_subtitles_normalize_language_without_changing_other_suffixes() {
        let f = Fixture::new();
        let mapping = f.media("01.mkv", b"movie", "1");
        fs::write(f.source.join("01.forced.sc_xxxxxx.ass"), b"subtitle").unwrap();
        let plan = create_plan(&f.store, vec![mapping], |_, _| {}).unwrap();
        let subtitle = plan.rows.iter().find(|r| r.attachment).unwrap();
        assert!(subtitle.target.ends_with(".forced.sc.ass"));
        assert!(subtitle.source.ends_with(".forced.sc_xxxxxx.ass"));
    }

    #[test]
    fn default_operation_applies_to_new_tasks_and_subtitles_only() {
        let f = Fixture::new();
        let mapping = f.media("01.mkv", b"movie", "1");
        fs::write(f.source.join("01.zh.ass"), b"subtitle").unwrap();
        let old = create_plan(&f.store, vec![mapping.clone()], |_, _| {}).unwrap();
        for operation in ["copy", "move", "hardlink", "symlink"] {
            let mut settings = f.store.settings().unwrap();
            settings.default_operation = operation.into();
            paths::validate_settings(&settings).unwrap();
            f.store.put("settings", "main", &settings).unwrap();
            let plan = create_plan(&f.store, vec![mapping.clone()], |_, _| {}).unwrap();
            assert_eq!(plan.rows.len(), 2);
            assert!(plan.rows.iter().all(|r| r.operation == operation));
            assert_eq!(
                f.store.get::<Plan>("plans", &old.id).unwrap().unwrap().rows[0].operation,
                "copy"
            );
        }
        let legacy: Settings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(legacy.default_operation, "copy");
        let mut bad = f.store.settings().unwrap();
        bad.default_operation = "delete".into();
        assert!(paths::validate_settings(&bad).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn remote_move_recovery_accepts_matching_size_and_ignores_stale_metadata() {
        use crate::remote_transfer::{open_verified, LockedFile};
        let f = Fixture::new();
        let mut plan = operation_plan(
            &f,
            vec![f.media("01.mkv", b"original", "1")],
            "move",
            "copy",
        );
        plan.verify_hash = false;
        plan.rows[0].signature.modified += 1000;
        remote_row(&f.store, &mut plan, 0, &f.tv).unwrap();
        assert_eq!(plan.rows[0].state, "complete");
        let proof = plan.rows[0].remote.clone().unwrap();
        assert_eq!(proof.method, "smb_move");
        assert!(proof.sha256.is_empty());
        assert!(!Path::new(&plan.rows[0].source).exists());
        open_verified(Path::new(&plan.rows[0].target), &proof.target, "", false).unwrap();
        plan.rows[0].state = "committing".into();
        remote_row(&f.store, &mut plan, 0, &f.tv).unwrap();
        assert_eq!(plan.rows[0].state, "complete");
        let target_path = plan.rows[0].target.clone();
        let target = Path::new(&target_path);
        let original = target.with_extension("saved");
        let file = LockedFile::open(target, true).unwrap();
        file.rename(&original).unwrap();
        drop(file);
        fs::write(target, b"replaced").unwrap();
        plan.rows[0].state = "failed".into();
        plan.rows[0].error = "文件标识或状态已变化".into();
        plan.rows[0].remote.as_mut().unwrap().target.identity = "stale-target".into();
        plan.rows[0].remote.as_mut().unwrap().ready = false;
        plan.rows[0].remote.as_mut().unwrap().sha256 = "stale-hash".into();
        remote_row(&f.store, &mut plan, 0, &f.tv).unwrap();
        assert_eq!(plan.rows[0].state, "complete");
        assert!(plan.rows[0].error.is_empty());
        assert_eq!(fs::read(target).unwrap(), b"replaced");
        fs::write(target, b"different-size").unwrap();
        plan.rows[0].state = "failed".into();
        assert!(remote_row(&f.store, &mut plan, 0, &f.tv)
            .unwrap_err()
            .contains("大小"));
        assert_eq!(fs::read(target).unwrap(), b"different-size");
    }
    #[cfg(windows)]
    #[test]
    fn committed_remote_copy_resumes_and_move_cleans_source_despite_stale_metadata() {
        use crate::remote_transfer::LockedFile;
        for operation in ["copy", "move"] {
            let f = Fixture::new();
            let mut p = operation_plan(
                &f,
                vec![f.media("01.mkv", b"original", "1")],
                operation,
                "copy",
            );
            let row = &mut p.rows[0];
            let target = Path::new(&row.target);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(&row.source, target).unwrap();
            let mut source_stamp = LockedFile::open(Path::new(&row.source), false)
                .unwrap()
                .stamp()
                .unwrap();
            let mut target_stamp = LockedFile::open(target, false).unwrap().stamp().unwrap();
            source_stamp.identity = "stale-source-id".into();
            target_stamp.identity = "stale-target-id".into();
            source_stamp.modified += 1;
            target_stamp.created += 1;
            row.remote = Some(RemoteTransfer {
                method: "smb_copy".into(),
                source: source_stamp,
                target: target_stamp,
                ready: true,
                sha256: String::new(),
            });
            row.state = "failed".into();
            remote_row(&f.store, &mut p, 0, &f.tv).unwrap();
            assert_eq!(p.rows[0].state, "complete");
            assert_eq!(Path::new(&p.rows[0].source).exists(), operation == "copy");
            assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"original");
        }
    }

    #[test]
    fn already_moved_media_and_subtitles_resume_by_size_without_checkpoints() {
        let f = Fixture::new();
        let mapping = f.media("01.mkv", b"video", "1");
        fs::write(f.source.join("01.zh.ass"), b"subtitle").unwrap();
        let mut p = operation_plan(&f, vec![mapping], "move", "copy");
        assert_eq!(p.rows.len(), 2);
        for row in &mut p.rows {
            let target = Path::new(&row.target);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::rename(&row.source, target).unwrap();
            row.state = "failed".into();
            row.error = "previous failure".into();
            row.digest = "old-digest".into();
            assert!(row.remote.is_none());
            assert!(!row.source_cleanup_pending);
        }
        p.state = "failed".into();
        f.store.put("plans", &p.id, &p).unwrap();
        let done = f.run(&p);
        assert_eq!(done.state, "complete", "{:?}", done.rows);
        for row in done.rows {
            assert_eq!(row.state, "complete");
            assert!(row.error.is_empty());
            assert!(!Path::new(&row.source).exists());
            assert_eq!(fs::metadata(&row.target).unwrap().len(), row.signature.size);
        }
    }

    #[test]
    fn moved_file_recovery_requires_existing_target_with_matching_size() {
        for content in [None, Some(b"wrong-size".as_slice())] {
            let f = Fixture::new();
            let m = f.media("01.mkv", b"video", "1");
            let p = operation_plan(&f, vec![m.clone()], "move", "copy");
            fs::remove_file(&m.source).unwrap();
            if let Some(content) = content {
                let target = Path::new(&p.rows[0].target);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(target, content).unwrap();
            }
            let done = f.run(&p);
            assert_eq!(done.state, "failed");
            assert_eq!(done.rows[0].state, "failed");
            if let Some(content) = content {
                assert!(done.rows[0].error.contains("大小"));
                assert_eq!(fs::read(&done.rows[0].target).unwrap(), content);
            }
        }
    }
    #[test]
    fn batch_cleanup_is_atomic_and_preserves_unfinished_tasks() {
        let f = Fixture::new();
        let a = f.plan(vec![f.media("01.mkv", b"first", "1")], "copy");
        let a = f.run(&a);
        let b = f.plan(vec![f.media("02.mkv", b"second", "2")], "copy");
        cancel_plan(&f.store, &b.id).unwrap();
        let c = f.plan(vec![f.media("03.mkv", b"third", "3")], "copy");
        assert!(delete_plans(&f.store, &[a.id.clone(), c.id.clone(), b.id.clone()]).is_err());
        assert_eq!(f.store.list::<Plan>("plans").unwrap().len(), 3);
        start_plan(&f.store, &c.id).unwrap();
        delete_plans(&f.store, &[a.id.clone(), b.id.clone()]).unwrap();
        let remaining = f.store.list::<Plan>("plans").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, c.id);
        assert_eq!(remaining[0].state, "running");
        assert!(!f.store.cancel.load(Ordering::SeqCst));
        assert_eq!(fs::read(&a.rows[0].target).unwrap(), b"first");
        assert_eq!(fs::read(&b.rows[0].source).unwrap(), b"second");
        assert_eq!(fs::read(&c.rows[0].source).unwrap(), b"third");
    }
    #[test]
    fn deleting_terminal_plans_keeps_media_metadata_backups_and_catalog() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"first", "1")], "copy");
        let p = f.run(&p);
        assert_eq!(p.state, "complete");
        let backup = f.backup.join("retained.mkv");
        let partial = f.source.join("retained.partial");
        fs::write(&backup, b"backup").unwrap();
        fs::write(&partial, b"partial").unwrap();
        let index = scan::index_library(&f.store).unwrap();
        delete_plan(&f.store, &p.id).unwrap();
        assert!(f.store.get::<Plan>("plans", &p.id).unwrap().is_none());
        assert_eq!(fs::read(&p.rows[0].source).unwrap(), b"first");
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"first");
        assert!(Path::new(&p.rows[0].target).with_extension("nfo").exists());
        assert_eq!(fs::read(&backup).unwrap(), b"backup");
        assert_eq!(fs::read(&partial).unwrap(), b"partial");
        assert_eq!(
            scan::index_library(&f.store).unwrap().assets.len(),
            index.assets.len()
        );
        assert!(f.store.get::<Work>("works", "show").unwrap().is_some());
        let q = f.plan(vec![f.media("02.mkv", b"second", "2")], "copy");
        cancel_plan(&f.store, &q.id).unwrap();
        delete_plan(&f.store, &q.id).unwrap();
        assert_eq!(fs::read(&q.rows[0].source).unwrap(), b"second");
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        assert!(reopened.list::<Plan>("plans").unwrap().is_empty());
        assert!(start_plan(&f.store, &p.id).is_err());
    }
    #[test]
    fn deletion_rejects_unfinished_tasks_and_waits_for_worker_release() {
        let f = Fixture::new();
        let mut p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        for state in [
            "draft",
            "running",
            "paused",
            "failed",
            "metadata_failed",
            "cancelling",
        ] {
            p.state = state.into();
            f.store.put("plans", &p.id, &p).unwrap();
            assert!(delete_plan(&f.store, &p.id)
                .unwrap_err()
                .contains("只能删除"));
            assert!(f.store.get::<Plan>("plans", &p.id).unwrap().is_some());
        }
        p.state = "draft".into();
        f.store.put("plans", &p.id, &p).unwrap();
        start_plan(&f.store, &p.id).unwrap();
        cancel_plan(&f.store, &p.id).unwrap();
        let p = execute(f.store.clone(), p.id.clone(), |_| {}).unwrap();
        assert_eq!(p.state, "cancelled");
        assert!(delete_plan(&f.store, &p.id)
            .unwrap_err()
            .contains("尚未停止"));
        finish_plan(&f.store, &p.id, None).unwrap();
        delete_plan(&f.store, &p.id).unwrap();
        assert!(delete_plan(&f.store, &p.id).is_err());
    }
    #[test]
    fn copy_persists_metadata_subtitles_and_preserves_sources() {
        let f = Fixture::new();
        let m = f.media("[FICTIONAL_VOYAGE_2][01].mkv", b"video bytes", "1");
        fs::write(
            f.source.join("[FICTIONAL_VOYAGE_2][01].ja.ass"),
            b"subtitle",
        )
        .unwrap();
        let p = f.plan(vec![m.clone()], "copy");
        assert_eq!(p.rows.len(), 2);
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.metadata_errors);
        assert_eq!(
            fs::read(f.tv.join("FICTIONAL VOYAGE/Season 02/S02E01.mkv")).unwrap(),
            b"video bytes"
        );
        assert!(f
            .tv
            .join("FICTIONAL VOYAGE/Season 02/S02E01.ja.ass")
            .exists());
        assert!(f.tv.join("FICTIONAL VOYAGE/tvshow.nfo").exists());
        assert!(f.tv.join("FICTIONAL VOYAGE/bangumi-manager.json").exists());
        assert!(Path::new(&m.source).exists());
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.assets.len(), 1);
        assert_eq!(index.assets[0].season, 2);
        assert_eq!(
            f.run(&p)
                .rows
                .iter()
                .filter(|r| r.state == "complete")
                .count(),
            2
        );
    }
    fn operation_plan(f: &Fixture, mappings: Vec<Mapping>, operation: &str, action: &str) -> Plan {
        let p = f.plan(mappings, action);
        decisions(
            &f.store,
            &p.id,
            p.rows
                .iter()
                .filter(|r| !r.attachment)
                .map(|r| Decision {
                    id: r.id.clone(),
                    action: action.into(),
                    operation: operation.into(),
                    reviewed: true,
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn move_verifies_and_removes_sources_including_subtitles() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        fs::write(f.source.join("01.ja.ass"), b"subtitle").unwrap();
        scan::scan_sources(&f.store, vec![paths::display(&f.source)], |_| {}).unwrap();
        let p = operation_plan(&f, vec![m.clone()], "move", "copy");
        assert!(p.rows.iter().all(|r| r.operation == "move"));
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert!(!Path::new(&m.source).exists());
        assert!(!f.source.join("01.ja.ass").exists());
        assert!(f.store.list::<SourceFile>("sources").unwrap().is_empty());
        assert_eq!(
            fs::read(&p.rows.iter().find(|r| !r.attachment).unwrap().target).unwrap(),
            b"video"
        );
        assert_eq!(scan::index_library(&f.store).unwrap().assets.len(), 1);
        assert_eq!(f.run(&p).state, "complete");
    }
    #[test]
    fn successful_move_removes_only_empty_immediate_source_directory_after_subtitles() {
        let f = Fixture::new();
        let source = f.source.join("show");
        fs::create_dir(&source).unwrap();
        let m = f.media("show/01.mkv", b"video", "1");
        fs::write(source.join("01.zh.ass"), b"subtitle").unwrap();
        let p = f.run(&operation_plan(&f, vec![m], "move", "copy"));
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert_eq!(p.rows.len(), 2);
        assert!(p
            .rows
            .iter()
            .all(|r| r.state == "complete" && Path::new(&r.target).is_file()));
        assert!(!source.exists());
        assert!(
            f.source.is_dir(),
            "Do not climb into the parent of the source directory"
        );
        assert!(p.cleanup_notices.is_empty());
        assert_eq!(
            f.run(&p).state,
            "complete",
            "Cleanup is idempotent on retry"
        );
    }
    #[test]
    fn source_directory_with_leftovers_is_always_preserved() {
        for name in [
            "OP.mkv",
            "ED.mkv",
            "menu.mkv",
            ".hidden",
            "unmatched.srt",
            "subdir",
        ] {
            let f = Fixture::new();
            let m = f.media("01.mkv", b"video", "1");
            let leftover = f.source.join(name);
            if name == "subdir" {
                fs::create_dir(&leftover).unwrap();
            } else {
                fs::write(&leftover, b"keep").unwrap();
            }
            let p = f.run(&operation_plan(&f, vec![m], "move", "copy"));
            assert_eq!(p.state, "complete", "{:?}", p.rows);
            assert!(f.source.is_dir());
            assert!(leftover.exists());
            assert!(p.cleanup_notices.is_empty());
        }
    }
    #[test]
    fn cleanup_excludes_skipped_failed_unfinished_and_non_move_rows_and_library_roots() {
        for (state, operation, action) in [
            ("failed", "move", "copy"),
            ("pending", "move", "copy"),
            ("cancelled", "move", "copy"),
            ("skipped", "move", "skip"),
            ("complete", "copy", "copy"),
            ("complete", "hardlink", "copy"),
            ("complete", "symlink", "copy"),
        ] {
            let f = Fixture::new();
            let m = f.media("01.mkv", b"video", "1");
            let mut p = operation_plan(&f, vec![m.clone()], "move", "copy");
            fs::remove_file(&m.source).unwrap();
            p.rows[0].state = state.into();
            p.rows[0].operation = operation.into();
            p.rows[0].action = action.into();
            assert!(cleanup_source_directories(&f.store, &p).is_empty());
            assert!(f.source.is_dir());
        }
        let f = Fixture::new();
        let mut p = operation_plan(&f, vec![f.media("01.mkv", b"video", "1")], "move", "copy");
        p.rows[0].source = paths::display(&f.tv.join("moved.mkv"));
        p.rows[0].state = "complete".into();
        assert!(cleanup_source_directories(&f.store, &p).is_empty());
        assert!(f.tv.is_dir());
        fs::remove_file(f.source.join("01.mkv")).unwrap();
        p.rows[0].source = paths::display(&f.source.join("01.mkv"));
        f.store.cancel.store(true, Ordering::Relaxed);
        assert!(cleanup_source_directories(&f.store, &p).is_empty());
        assert!(
            f.source.is_dir(),
            "A stop request also stops directory cleanup"
        );
    }
    #[cfg(windows)]
    #[test]
    fn directory_cleanup_permission_error_does_not_fail_completed_media() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        let p = operation_plan(&f, vec![m], "move", "copy");
        let locked = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&f.source)
            .unwrap();
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert!(p.metadata_errors.is_empty());
        assert_eq!(p.cleanup_notices.len(), 1);
        assert!(p.cleanup_notices[0].contains(&paths::display(&f.source)));
        assert!(f.source.is_dir());
        drop(locked);
        assert!(cleanup_source_directories(&f.store, &p).is_empty());
        assert!(!f.source.exists());
    }
    #[cfg(windows)]
    #[test]
    fn cleanup_preserves_directory_symlinks_and_their_targets() {
        let f = Fixture::new();
        let mut p = operation_plan(&f, vec![f.media("01.mkv", b"video", "1")], "move", "copy");
        let other = f.source.join("other");
        let link = f.source.join("link");
        fs::create_dir(&other).unwrap();
        if std::os::windows::fs::symlink_dir(&other, &link).is_err() {
            return;
        }
        p.rows[0].source = paths::display(&link.join("moved.mkv"));
        p.rows[0].state = "complete".into();
        assert!(cleanup_source_directories(&f.store, &p).is_empty());
        assert!(link.exists());
        assert!(other.exists());
    }
    #[test]
    fn hardlinks_share_storage_and_do_not_delete_source() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        fs::write(f.source.join("01.srt"), b"subtitle").unwrap();
        let p = operation_plan(&f, vec![m.clone()], "hardlink", "copy");
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert!(Path::new(&m.source).exists());
        let media = p.rows.iter().find(|r| !r.attachment).unwrap();
        fs::write(&media.target, b"same inode changed").unwrap();
        assert_eq!(fs::read(&m.source).unwrap(), b"same inode changed");
        let sub = p.rows.iter().find(|r| r.attachment).unwrap();
        fs::write(&sub.target, b"changed subtitle").unwrap();
        assert_eq!(fs::read(&sub.source).unwrap(), b"changed subtitle");
    }
    #[test]
    fn symlinks_preserve_sources_index_media_and_resume() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        let p = operation_plan(&f, vec![m.clone()], "symlink", "copy");
        let mut p = f.run(&p);
        if p.rows[0].state == "failed" && p.rows[0].error.contains("无法创建软链接") {
            // Some Windows hosts intentionally disable symbolic link privileges.
            assert!(Path::new(&m.source).exists());
            assert!(!Path::new(&p.rows[0].target).exists());
            eprintln!("symlink unavailable on this host: {}", p.rows[0].error);
            return;
        }
        assert_eq!(p.state, "complete", "{:?} {:?}", p.rows, p.metadata_errors);
        assert_eq!(
            paths::key(&fs::read_link(&p.rows[0].target).unwrap()),
            paths::key(Path::new(&m.source))
        );
        assert!(Path::new(&m.source).exists());
        assert_eq!(scan::index_library(&f.store).unwrap().assets.len(), 1);
        p.rows[0].state = "committing".into();
        p.state = "paused".into();
        f.store.put("plans", &p.id, &p).unwrap();
        assert_eq!(f.run(&p).state, "complete");
        assert_eq!(fs::read(&m.source).unwrap(), b"video");
    }
    #[test]
    fn move_cleanup_resumes_after_commit_or_after_source_deletion() {
        for removed in [false, true] {
            let f = Fixture::new();
            let m = f.media("01.mkv", b"video", "1");
            let mut p = operation_plan(&f, vec![m.clone()], "move", "copy");
            let row = &mut p.rows[0];
            let target = Path::new(&row.target);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(&m.source, target).unwrap();
            row.stage = paths::display(&target.with_extension("partial"));
            row.digest = digest(target).unwrap();
            row.source_cleanup_pending = removed;
            row.state = "failed".into();
            if removed {
                fs::remove_file(&m.source).unwrap();
            }
            f.store.put("plans", &p.id, &p).unwrap();
            assert_eq!(f.run(&p).state, "complete");
            assert!(!Path::new(&m.source).exists());
            assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"video");
        }
    }
    #[test]
    fn move_refuses_changed_source_after_commit_even_with_same_size() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        let mut p = operation_plan(&f, vec![m.clone()], "move", "copy");
        let row = &mut p.rows[0];
        let target = Path::new(&row.target);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(&m.source, target).unwrap();
        row.stage = paths::display(&target.with_extension("partial"));
        row.digest = digest(target).unwrap();
        row.state = "committing".into();
        fs::write(&m.source, b"other").unwrap();
        row.signature = paths::signature(Path::new(&m.source)).unwrap();
        f.store.put("plans", &p.id, &p).unwrap();
        let p = f.run(&p);
        assert_eq!(p.state, "failed");
        assert_eq!(fs::read(&m.source).unwrap(), b"other");
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"video");
    }
    #[cfg(windows)]
    #[test]
    fn locked_move_source_keeps_verified_target_and_retries_only_cleanup() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let f = Fixture::new();
        let m = f.media("01.mkv", b"video", "1");
        let p = operation_plan(&f, vec![m.clone()], "move", "copy");
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&m.source)
            .unwrap();
        let p = f.run(&p);
        assert_eq!(p.state, "failed");
        assert!(p.rows[0].source_cleanup_pending);
        let target_signature = paths::signature(Path::new(&p.rows[0].target)).unwrap();
        assert_eq!(fs::read(&m.source).unwrap(), b"video");
        drop(lock);
        assert_eq!(f.run(&p).state, "complete");
        assert!(!Path::new(&m.source).exists());
        assert_eq!(
            paths::signature(Path::new(&p.rows[0].target)).unwrap(),
            target_signature
        );
    }
    #[test]
    fn new_operations_replace_only_after_backup_and_reject_late_targets() {
        for operation in ["move", "hardlink", "symlink"] {
            let f = Fixture::new();
            let old = f.plan(vec![f.media("old.mkv", b"old", "1")], "copy");
            f.run(&old);
            let m = f.media("new.mkv", b"new", "1");
            let p = operation_plan(&f, vec![m.clone()], operation, "replace");
            let p = f.run(&p);
            if operation == "symlink" && p.rows[0].error.contains("无法创建软链接") {
                assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"old");
                continue;
            }
            assert_eq!(p.state, "complete", "{} {:?}", operation, p.rows);
            assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"new");
            assert!(p.rows[0]
                .backups
                .iter()
                .any(|b| fs::read(&b.backup).unwrap() == b"old"));
            assert_eq!(Path::new(&m.source).exists(), operation != "move");
        }
        for operation in ["move", "hardlink", "symlink"] {
            let f = Fixture::new();
            let m = f.media("01.mkv", b"video", "1");
            let p = operation_plan(&f, vec![m.clone()], operation, "copy");
            let target = Path::new(&p.rows[0].target);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"unrelated file").unwrap();
            assert_eq!(f.run(&p).state, "failed");
            assert_eq!(fs::read(target).unwrap(), b"unrelated file");
            assert_eq!(fs::read(&m.source).unwrap(), b"video");
        }
    }
    #[test]
    fn a_failed_link_can_switch_to_copy_before_any_file_is_committed() {
        let f = Fixture::new();
        let mut p = operation_plan(
            &f,
            vec![f.media("01.mkv", b"video", "1")],
            "symlink",
            "copy",
        );
        p.state = "failed".into();
        p.rows[0].state = "failed".into();
        p.rows[0].stage = paths::display(&f.tv.join("missing.partial"));
        f.store.put("plans", &p.id, &p).unwrap();
        let p = decisions(
            &f.store,
            &p.id,
            vec![Decision {
                id: p.rows[0].id.clone(),
                action: "copy".into(),
                operation: "copy".into(),
                reviewed: true,
            }],
        )
        .unwrap();
        assert_eq!(f.run(&p).state, "complete");
    }
    #[test]
    fn older_saved_plans_and_choices_default_to_copy() {
        let choice: Decision = serde_json::from_value(
            serde_json::json!({ "id": "old", "action": "copy", "reviewed": true }),
        )
        .unwrap();
        assert_eq!(choice.operation, "copy");
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"video", "1")], "copy");
        let mut json = serde_json::to_value(&p).unwrap();
        json.as_object_mut().unwrap().remove("cleanup_notices");
        let row = json["rows"][0].as_object_mut().unwrap();
        row.remove("operation");
        row.remove("source_cleanup_pending");
        let p: Plan = serde_json::from_value(json).unwrap();
        assert_eq!(p.rows[0].operation, "copy");
        assert!(!p.rows[0].source_cleanup_pending);
        assert!(p.cleanup_notices.is_empty());
    }
    #[test]
    fn categorized_output_copies_all_media_types_with_bilingual_metadata_and_artwork() {
        for (kind, extension, category, tag) in [
            ("tv", "mkv", "电视剧/日番", "tvshow"),
            ("movie", "mkv", "电影/动画电影", "movie"),
            ("music", "flac", "音乐/专辑", "album"),
        ] {
            let f = Fixture::new();
            let mut s = f.store.settings().unwrap();
            s.output_root = paths::display(&f._dir.path().join("output"));
            s.tmdb_token = "secret-must-not-be-exported".into();
            f.store.put("settings", "main", &s).unwrap();
            let mut w = work();
            w.kind = kind.into();
            w.category.clear();
            w.info = MediaInfo {
                original_title: "原題".into(),
                localized_title: "中文译名".into(),
                language: "zh-CN".into(),
                original_language: "ja".into(),
                genres: vec![16],
                countries: vec!["JP".into()],
                ..Default::default()
            };
            w.episodes[0].title = "新旅程".into();
            w.episodes[0].info = MediaInfo {
                original_title: "新しい旅".into(),
                language: "zh-CN".into(),
                original_language: "ja".into(),
                ..Default::default()
            };
            let art = f.store.data_dir.join("artwork");
            fs::create_dir_all(&art).unwrap();
            fs::write(art.join("poster.jpg"), b"cached image").unwrap();
            w.info.local_poster = paths::display(&art.join("poster.jpg"));
            w.poster = "https://example.invalid/poster.jpg".into();
            w.seasons = vec![Season {
                binding_id: "b".into(),
                number: 2,
                remote_number: 2,
                title: "第二章".into(),
                ..Default::default()
            }];
            f.store.put("works", "show", &w).unwrap();
            let m = f.media(&format!("source.{extension}"), b"media", "1");
            let p = f.plan(vec![m], "copy");
            assert!(p.rows[0].target.replace('\\', "/").contains(category));
            assert!(!serde_json::to_string(&p)
                .unwrap()
                .contains("secret-must-not-be-exported"));
            let result = f.run(&p);
            assert_eq!(result.state, "complete", "{:?}", result.metadata_errors);
            assert!(
                result.metadata_errors.is_empty(),
                "{:?}",
                result.metadata_errors
            );
            let folder = Path::new(&p.rows[0].media_folder);
            let nfo_path = if kind == "movie" {
                Path::new(&p.rows[0].target).with_extension("nfo")
            } else {
                folder.join(format!("{tag}.nfo"))
            };
            let nfo = fs::read_to_string(&nfo_path).unwrap();
            assert_eq!(
                crate::nfo::one(&nfo_path, tag).unwrap().value("title"),
                w.title
            );
            if kind == "movie" {
                assert!(!folder.join("movie.nfo").exists());
            }
            if kind == "tv" {
                let season = crate::nfo::one(
                    &Path::new(&p.rows[0].target)
                        .parent()
                        .unwrap()
                        .join("season.nfo"),
                    "season",
                )
                .unwrap();
                assert_eq!(season.number("seasonnumber"), Some(2));
            }
            assert!(nfo.contains("原題") && nfo.contains("中文译名") && nfo.contains("zh-CN"));
            assert_eq!(
                fs::read(folder.join("poster.jpg")).unwrap(),
                b"cached image"
            );
            if kind != "movie" {
                assert!(
                    fs::read_to_string(Path::new(&p.rows[0].target).with_extension("nfo"))
                        .unwrap()
                        .contains("新しい旅")
                );
            }
            let index = scan::index_library(&f.store).unwrap();
            assert_eq!(index.assets.len(), 1);
            assert_eq!(index.assets[0].season, 2);
            if kind == "tv" {
                w.seasons[0].title = "可读季名".into();
                crate::metadata::refresh_existing(&f.store, &w).unwrap();
                assert!(fs::read_to_string(folder.join("tvshow.nfo"))
                    .unwrap()
                    .contains("可读季名"));
            }
            // A fresh catalog must recover the generated NFOs without our JSON sidecar.
            let fresh = Store::open(f._dir.path().join("fresh-catalog")).unwrap();
            fresh.put("settings", "main", &s).unwrap();
            fs::remove_file(folder.join("bangumi-manager.json")).unwrap();
            let restored = scan::index_library(&fresh).unwrap();
            assert_eq!(restored.imported, 1, "{:?}", restored.errors);
            assert_eq!(restored.assets.len(), 1, "{:?}", restored.errors);
            let restored_work = fresh.list::<Work>("works").unwrap().remove(0);
            assert_eq!(restored_work.title, w.title);
            assert_eq!(restored_work.info.original_title, "原題");
            if kind != "movie" {
                assert_eq!(restored_work.episodes[0].number, 1);
                assert_eq!(restored_work.episodes[0].info.original_title, "新しい旅");
            }
            s.naming.tv += " ";
            f.store.put("settings", "main", &s).unwrap();
            assert!(preflight(&f.store, &p).is_err());
        }
    }
    #[test]
    fn duplicate_files_cannot_execute_until_one_is_skipped() {
        let f = Fixture::new();
        let a = f.media("01.mkv", b"a", "1");
        let b = f.media("01.mp4", b"b", "1");
        let p = f.plan(vec![a, b], "copy");
        assert!(validate_plan(&p).is_err());
        assert!(!f.tv.join("FICTIONAL VOYAGE").exists());
        let choices = p
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| Decision {
                id: r.id.clone(),
                operation: "copy".into(),
                action: if i == 0 { "copy" } else { "skip" }.into(),
                reviewed: true,
            })
            .collect();
        let p = decisions(&f.store, &p.id, choices).unwrap();
        assert_eq!(f.run(&p).state, "complete");
    }
    #[test]
    fn replacing_another_extension_backs_up_old_media_and_subtitle() {
        let f = Fixture::new();
        let first = f.media("web.mp4", b"old web version", "1");
        fs::write(f.source.join("web.en.srt"), b"old subtitles").unwrap();
        let p = f.plan(vec![first], "copy");
        assert_eq!(f.run(&p).state, "complete");
        let new = f.media("bluray.mkv", b"larger new blu-ray version", "1");
        let p = f.plan(vec![new], "replace");
        assert!(!p.rows[0].backups.is_empty());
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert!(!f.tv.join("FICTIONAL VOYAGE/Season 02/S02E01.mp4").exists());
        assert!(!f
            .tv
            .join("FICTIONAL VOYAGE/Season 02/S02E01.en.srt")
            .exists());
        assert!(p.rows[0]
            .backups
            .iter()
            .any(|b| fs::read(&b.backup).unwrap() == b"old web version"));
        assert!(p.rows[0]
            .backups
            .iter()
            .all(|b| paths::contained(Path::new(&b.backup), &f.backup)));
    }
    #[test]
    fn replacing_same_extension_and_subtitle_is_recoverable() {
        let f = Fixture::new();
        let first = f.media("old.mkv", b"old", "1");
        fs::write(f.source.join("old.srt"), b"old sub").unwrap();
        let p = f.plan(vec![first], "copy");
        f.run(&p);
        let new = f.media("new.mkv", b"new version", "1");
        fs::write(f.source.join("new.srt"), b"new subtitles").unwrap();
        let p = f.plan(vec![new], "replace");
        let p = f.run(&p);
        assert_eq!(p.state, "complete", "{:?}", p.rows);
        assert_eq!(
            fs::read(f.tv.join("FICTIONAL VOYAGE/Season 02/S02E01.srt")).unwrap(),
            b"new subtitles"
        );
    }
    #[test]
    fn refuses_changed_source_and_late_target_without_overwriting() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"source", "1");
        let p = f.plan(vec![m.clone()], "copy");
        fs::write(&m.source, b"changed source length").unwrap();
        assert_eq!(f.run(&p).state, "failed");
        assert!(!Path::new(&p.rows[0].target).exists());
        let p = f.plan(vec![m], "copy");
        let target = Path::new(&p.rows[0].target);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, b"unrelated file").unwrap();
        assert_eq!(f.run(&p).state, "failed");
        assert_eq!(fs::read(target).unwrap(), b"unrelated file");
    }
    #[test]
    fn cancellation_and_interrupted_commit_resume_safely() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"source", "1");
        let p = f.plan(vec![m], "copy");
        f.store.cancel.store(true, Ordering::SeqCst);
        assert_eq!(f.run(&p).state, "paused");
        f.store.cancel.store(false, Ordering::SeqCst);
        let mut p = f.store.get::<Plan>("plans", &p.id).unwrap().unwrap();
        let row = &mut p.rows[0];
        let target = Path::new(&row.target);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, b"source").unwrap();
        row.stage = paths::display(&target.with_extension("partial"));
        row.digest = digest(target).unwrap();
        row.state = "committing".into();
        f.store.put("plans", &p.id, &p).unwrap();
        assert_eq!(f.run(&p).state, "complete");
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"source");
    }
    #[test]
    fn cancelled_draft_is_durable_and_cannot_execute_or_change_decisions() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        let cancelled = cancel_plan(&f.store, &p.id).unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.rows[0].state, "cancelled");
        assert!(Path::new(&p.rows[0].source).exists());
        assert!(!Path::new(&p.rows[0].target).exists());
        assert!(start_plan(&f.store, &p.id).is_err());
        assert!(execute(f.store.clone(), p.id.clone(), |_| {}).is_err());
        assert!(decisions(&f.store, &p.id, vec![]).is_err());
        assert_eq!(cancel_plan(&f.store, &p.id).unwrap().state, "cancelled");
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        assert_eq!(
            reopened.get::<Plan>("plans", &p.id).unwrap().unwrap().state,
            "cancelled"
        );
    }
    #[test]
    fn cancel_running_move_preserves_completed_files_and_stops_remaining_rows() {
        let f = Fixture::new();
        let a = f.media("01.mkv", b"first", "1");
        let b = f.media("02.mkv", b"second", "2");
        let p = operation_plan(&f, vec![a.clone(), b.clone()], "move", "copy");
        start_plan(&f.store, &p.id).unwrap();
        let result = execute(f.store.clone(), p.id.clone(), |progress| {
            if progress.state == "running" && progress.rows[0].state == "complete" {
                assert_eq!(cancel_plan(&f.store, &p.id).unwrap().state, "cancelling");
            }
        })
        .unwrap();
        assert_eq!(result.state, "cancelled");
        assert_eq!(result.rows[0].state, "complete");
        assert_eq!(result.rows[1].state, "cancelled");
        assert!(!Path::new(&a.source).exists());
        assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"first");
        assert_eq!(fs::read(&b.source).unwrap(), b"second");
        assert!(!Path::new(&p.rows[1].target).exists());
        assert_eq!(
            finish_plan(&f.store, &p.id, None).unwrap().state,
            "cancelled"
        );
        assert!(!f.store.running.load(Ordering::SeqCst));
    }
    #[test]
    fn immediate_cancel_survives_stale_worker_progress_and_restart() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        start_plan(&f.store, &p.id).unwrap();
        let mut stale = f.store.get::<Plan>("plans", &p.id).unwrap().unwrap();
        cancel_plan(&f.store, &p.id).unwrap();
        save(&f.store, &mut stale).unwrap();
        assert_eq!(stale.state, "cancelling");
        let result = execute(f.store.clone(), p.id.clone(), |_| {}).unwrap();
        assert_eq!(result.state, "cancelled");
        finish_plan(&f.store, &p.id, None).unwrap();
        assert!(!Path::new(&p.rows[0].target).exists());
        let q = f.plan(vec![f.media("02.mkv", b"another", "2")], "copy");
        start_plan(&f.store, &q.id).unwrap();
        cancel_plan(&f.store, &q.id).unwrap();
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        let restored = reopened.get::<Plan>("plans", &q.id).unwrap().unwrap();
        assert_eq!(restored.state, "cancelled");
        assert_eq!(restored.rows[0].state, "cancelled");
        assert!(!Path::new(&q.rows[0].target).exists());
    }
    #[test]
    fn cancelling_another_plan_does_not_stop_the_active_job() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"first", "1")], "copy");
        let q = f.plan(vec![f.media("02.mkv", b"second", "2")], "copy");
        start_plan(&f.store, &p.id).unwrap();
        assert!(start_plan(&f.store, &q.id).is_err());
        assert_eq!(cancel_plan(&f.store, &q.id).unwrap().state, "cancelled");
        assert!(!f.store.cancel.load(Ordering::SeqCst));
        assert!(pause_plan(&f.store, &q.id).is_err());
        assert!(!f.store.cancel.load(Ordering::SeqCst));
        assert_eq!(f.run(&p).state, "complete");
        finish_plan(&f.store, &p.id, None).unwrap();
        assert_eq!(cancel_plan(&f.store, &p.id).unwrap().state, "complete");
    }
    #[test]
    fn cancel_during_worker_shutdown_wins_over_pause_or_error() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        start_plan(&f.store, &p.id).unwrap();
        pause_plan(&f.store, &p.id).unwrap();
        assert_eq!(f.run(&p).state, "paused");
        cancel_plan(&f.store, &p.id).unwrap();
        assert_eq!(
            finish_plan(&f.store, &p.id, Some("worker error"))
                .unwrap()
                .state,
            "cancelled"
        );
        assert!(!f.store.running.load(Ordering::SeqCst));
        assert!(f.store.active_plan.lock().unwrap().is_none());
    }
    #[test]
    fn offline_library_keeps_previous_assets_as_unknown() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        f.run(&p);
        fs::rename(&f.tv, f.tv.with_file_name("offline")).unwrap();
        let index = scan::index_library(&f.store).unwrap();
        assert!(!index.available);
        assert_eq!(index.assets.len(), 1);
        assert!(!index.errors.is_empty());
    }
    #[test]
    fn extras_copy_and_move_preserve_subdirectories_and_file_contents() {
        for operation in ["copy", "move"] {
            let f = Fixture::new();
            let files = [
                ("Bonus/Disc 1/cover.jpg", "Disc 1/cover.jpg", "first cover"),
                ("Bonus/Disc 2/cover.jpg", "Disc 2/cover.jpg", "second cover"),
                ("Extra/Interview/clip.mkv", "Interview/clip.mkv", "video"),
                ("Extra/Interview/clip.srt", "Interview/clip.srt", "subtitle"),
                ("Scans/Book/01.jpg", "Scans/Book/01.jpg", "scan"),
                ("CDs/Disc 1/01.flac", "CDs/Disc 1/01.flac", "audio"),
            ];
            let mappings = files.iter().map(|(source, _, content)| {
                let path = f.source.join(source);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, content).unwrap();
                Mapping { part: 0, source: paths::display(&path), work_id: "show".into(), episode_ids: vec![], extra: true }
            }).collect();
            let plan = create_plan(&f.store, mappings, |_, _| {}).unwrap();
            assert_eq!(plan.rows.len(), files.len());
            assert!(plan.rows.iter().all(|row| row.extra && row.conflicts.is_empty() && row.probe.is_none()));
            let plan = decisions(&f.store, &plan.id, plan.rows.iter().map(|row| Decision {
                id: row.id.clone(), action: "copy".into(), operation: operation.into(), reviewed: true,
            }).collect()).unwrap();
            let result = f.run(&plan);
            assert_eq!(result.state, "complete", "{:?}", result.rows.iter().map(|row| &row.error).collect::<Vec<_>>());
            let target_root = PathBuf::from(&plan.extras_root).join(work().folder_name);
            for (source, relative, content) in files {
                assert_eq!(fs::read(target_root.join(relative)).unwrap(), content.as_bytes());
                assert_eq!(f.source.join(source).exists(), operation == "copy");
            }
            assert!(!target_root.join("Bonus").exists());
            assert!(!target_root.join("cover.jpg").exists());
        }
    }
    #[test]
    fn explicitly_selected_extra_subtitle_is_not_also_attached_to_media() {
        for extra_first in [false, true] {
            let f = Fixture::new();
            fs::create_dir_all(f.source.join("Bonus/Interview")).unwrap();
            let media = f.media("Bonus/Interview/clip.mkv", b"video", "1");
            let subtitle = f.source.join("Bonus/Interview/clip.srt");
            fs::write(&subtitle, b"subtitle").unwrap();
            let extra = Mapping {
                part: 0,
                source: paths::display(&subtitle), work_id: "show".into(), episode_ids: vec![], extra: true,
            };
            let mappings = if extra_first { vec![extra, media] } else { vec![media, extra] };
            let plan = operation_plan(&f, mappings, "move", "copy");
            assert_eq!(plan.rows.len(), 2);
            assert!(plan.rows.iter().all(|row| !row.attachment && row.conflicts.is_empty()));
            let result = f.run(&plan);
            assert_eq!(result.state, "complete");
            let media = result.rows.iter().find(|row| !row.extra).unwrap();
            let extra = result.rows.iter().find(|row| row.extra).unwrap();
            assert_eq!(fs::read(&media.target).unwrap(), b"video");
            assert_eq!(fs::read(&extra.target).unwrap(), b"subtitle");
            assert!(!Path::new(&media.target).with_extension("srt").exists());
            assert!(!Path::new(&media.source).exists());
            assert!(!subtitle.exists());
        }
    }
    #[test]
    fn extras_still_report_collisions_after_removing_packaging_directories() {
        let f = Fixture::new();
        let mappings = ["Bonus/Book/cover.jpg", "Extras/Book/cover.jpg"].into_iter().map(|relative| {
            let path = f.source.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative).unwrap();
            Mapping { part: 0, source: paths::display(&path), work_id: "show".into(), episode_ids: vec![], extra: true }
        }).collect();
        let plan = create_plan(&f.store, mappings, |_, _| {}).unwrap();
        assert_eq!(plan.rows[0].target, plan.rows[1].target);
        assert!(plan.rows.iter().all(|row| !row.conflicts.is_empty()));
        assert!(plan.rows.iter().all(|row| Path::new(&row.source).exists() && !Path::new(&row.target).exists()));
    }
    #[test]
    fn movie_and_extras_have_separate_roots() {
        let f = Fixture::new();
        let mut movie = work();
        movie.kind = "movie".into();
        movie.id = "film".into();
        movie.folder_name = "劇場版 (2020)".into();
        f.store.put("works", "film", &movie).unwrap();
        let mut m = f.media("movie.mkv", b"film", "1");
        m.work_id = "film".into();
        let p = f.plan(vec![m], "copy");
        let p = f.run(&p);
        assert_eq!(p.state, "complete");
        assert!(p.rows[0].target.contains("Movies"));
        assert!(p.rows[0].target.ends_with("劇場版 (2020).mkv"));
        let mut extra = f.media("[NCOP].mkv", b"opening", "1");
        extra.extra = true;
        extra.episode_ids.clear();
        let p = f.plan(vec![extra], "copy");
        let p = f.run(&p);
        assert_eq!(p.state, "complete");
        assert!(p.rows[0].target.contains("Extras"));
    }
    #[test]
    fn an_unbound_existing_folder_requires_confirmation() {
        let f = Fixture::new();
        let folder = f.tv.join("FICTIONAL VOYAGE/Season 02");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("S02E01.mp4"), b"old").unwrap();
        let m = f.media("01.mkv", b"source", "1");
        assert!(create_plan(&f.store, vec![m.clone()], |_, _| {}).is_err());
        scan::bind_folder(
            &f.store,
            &paths::display(&f.tv.join("FICTIONAL VOYAGE")),
            "show",
        )
        .unwrap();
        let p = f.plan(vec![m], "replace");
        assert!(!p.rows[0].conflicts.is_empty());
    }
    #[cfg(windows)]
    #[test]
    fn locked_existing_video_does_not_get_lost() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = Fixture::new();
        let p = f.plan(vec![f.media("old.mkv", b"old", "1")], "copy");
        f.run(&p);
        let target = f.tv.join("FICTIONAL VOYAGE/Season 02/S02E01.mkv");
        let p = f.plan(vec![f.media("new.mkv", b"new version", "1")], "replace");
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        let p = f.run(&p);
        assert_eq!(p.state, "failed");
        drop(lock);
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert_eq!(f.run(&p).state, "complete");
    }
    #[test]
    fn stale_season_mapping_requires_new_preview() {
        let f = Fixture::new();
        let p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        let mut w = work();
        w.episodes[0].season = 3;
        f.store.put("works", "show", &w).unwrap();
        assert!(execute(f.store.clone(), p.id, |_| {})
            .unwrap_err()
            .contains("分季映射"));
        assert!(!f.tv.join("FICTIONAL VOYAGE").exists());
    }
    #[test]
    fn partial_replacement_of_a_multi_episode_file_is_refused() {
        let f = Fixture::new();
        let mut m = f.media("01-02.mkv", b"two episodes", "1");
        m.episode_ids.push("2".into());
        let p = f.plan(vec![m], "copy");
        f.run(&p);
        let m = f.media("01.mp4", b"one episode", "1");
        assert!(create_plan(&f.store, vec![m], |_, _| {})
            .unwrap_err()
            .contains("合并多集"));
        assert_eq!(
            fs::read(f.tv.join("FICTIONAL VOYAGE/Season 02/S02E01-E02.mkv")).unwrap(),
            b"two episodes"
        );
    }
    #[test]
    fn foreign_nfos_are_replaced_and_existing_images_only_add_notices() {
        for kind in ["tv", "movie", "music"] {
            let f = Fixture::new();
            let mut settings = f.store.settings().unwrap();
            settings.output_root = paths::display(&f._dir.path().join("output"));
            f.store.put("settings", "main", &settings).unwrap();
            let mut w = work();
            w.kind = kind.into();
            // Invalid URLs prove existing images are skipped before attempting downloads.
            w.poster = "invalid image URL".into();
            w.info.backdrop = "invalid image URL".into();
            w.episodes[0].info.poster = "invalid image URL".into();
            w.seasons.push(Season {
                binding_id: "b".into(),
                number: 2,
                title: "第二季".into(),
                info: MediaInfo {
                    poster: "invalid image URL".into(),
                    ..Default::default()
                },
                ..Default::default()
            });
            f.store.put("works", "show", &w).unwrap();
            let extension = if kind == "music" { "flac" } else { "mkv" };
            let p = f.plan(
                vec![f.media(&format!("01.{extension}"), b"source", "1")],
                "copy",
            );
            let target = Path::new(&p.rows[0].target);
            let folder = Path::new(&p.rows[0].media_folder);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let mut nfos = vec![target.with_extension("nfo")];
            let mut images = vec![folder.join("poster.jpg"), folder.join("fanart.jpg")];
            if kind != "movie" {
                nfos.push(folder.join(if kind == "tv" {
                    "tvshow.nfo"
                } else {
                    "album.nfo"
                }));
                images.push(target.with_file_name(format!(
                    "{}-thumb.jpg",
                    target.file_stem().unwrap().to_string_lossy()
                )));
            }
            if kind == "tv" {
                nfos.push(target.parent().unwrap().join("season.nfo"));
                images.push(folder.join("season02-poster.jpg"));
            }
            for nfo in &nfos {
                fs::write(nfo, b"external metadata").unwrap();
            }
            for image in &images {
                fs::write(image, b"existing artwork").unwrap();
            }
            // Even images previously managed by this application are preserved.
            f.store
                .put("metadata_owned", &paths::key(&images[0]), &true)
                .unwrap();
            let mut result = f.run(&p);
            assert_eq!(result.state, "complete", "{:?}", result.metadata_errors);
            assert!(result.metadata_errors.is_empty());
            assert_eq!(result.metadata_notices.len(), images.len());
            for image in &images {
                assert_eq!(fs::read(image).unwrap(), b"existing artwork");
                assert!(result
                    .metadata_notices
                    .iter()
                    .any(|n| n.contains(&paths::display(image))));
            }
            for nfo in &nfos {
                assert!(fs::read_to_string(nfo).unwrap().contains("<?xml"));
                fs::write(nfo, b"another external update").unwrap();
            }
            let signature = paths::signature(target).unwrap();
            fs::remove_file(&p.rows[0].source).unwrap();
            // Simulate retrying a task that previously failed only on foreign metadata.
            result.state = "metadata_failed".into();
            result
                .metadata_errors
                .push("已有元数据未由本应用管理".into());
            f.store.put("plans", &p.id, &result).unwrap();
            let retried = f.run(&result);
            assert_eq!(retried.state, "complete", "{:?}", retried.metadata_errors);
            assert_eq!(paths::signature(target).unwrap(), signature);
            assert_eq!(retried.metadata_notices.len(), images.len());
            for nfo in &nfos {
                assert!(fs::read_to_string(nfo).unwrap().contains("<?xml"));
            }
            crate::metadata::refresh_existing(&f.store, &w).unwrap();
            let saved = f.store.get::<Work>("works", &w.id).unwrap().unwrap();
            assert!(saved.metadata_errors.is_empty());
            assert_eq!(saved.metadata_notices.len(), images.len());
        }
    }
    #[test]
    fn creating_tasks_skips_probe_and_reuses_only_valid_cached_results() {
        let f = Fixture::new();
        let m = f.media("01.mkv", b"media", "1");
        let p = f.plan(vec![m.clone()], "copy");
        assert!(p.rows[0].probe.is_none());
        assert!(p.rows[0].warnings.is_empty());
        let sig = paths::signature(Path::new(&m.source)).unwrap();
        let key = scan::probe_key(&f.store, Path::new(&m.source), &sig).unwrap();
        f.store
            .put(
                "probes",
                &key,
                &Probe {
                    height: Some(1080),
                    duration: Some(1440.0),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            f.plan(vec![m.clone()], "copy").rows[0]
                .probe
                .as_ref()
                .unwrap()
                .height,
            Some(1080)
        );
        fs::write(&m.source, b"changed size").unwrap();
        assert!(f.plan(vec![m], "copy").rows[0].probe.is_none());
    }
    #[test]
    fn media_dependent_templates_request_manual_probe_without_creating_a_bad_plan() {
        let f = Fixture::new();
        let mut settings = f.store.settings().unwrap();
        settings.output_root = paths::display(&f._dir.path().join("output"));
        settings.naming.tv =
            "{{title}}/Season {{season}}/{{season_episode}} {{videoFormat}}{{fileExt}}".into();
        f.store.put("settings", "main", &settings).unwrap();
        let m = f.media("01.mkv", b"media", "1");
        assert!(create_plan(&f.store, vec![m], |_, _| {})
            .unwrap_err()
            .contains("刷新媒体信息"));
        assert!(f.store.list::<Plan>("plans").unwrap().is_empty());
    }
    #[test]
    fn queue_executes_in_submission_order_and_keeps_drafts_unexecuted() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let a = f.plan(vec![f.media("01.mkv", b"one", "1")], "copy");
        let b = f.plan(vec![f.media("02.mkv", b"two", "2")], "copy");
        let draft = f.plan(vec![f.media("03.mkv", b"three", "3")], "copy");
        queue::enqueue(&f.store, &a.id).unwrap();
        queue::enqueue(&f.store, &b.id).unwrap();
        assert!(queue::reserve_worker(&f.store).unwrap());
        assert!(!queue::reserve_worker(&f.store).unwrap());
        let finished = std::cell::RefCell::new(vec![]);
        queue::drain(f.store.clone(), |event| {
            if let queue::Event::Finished(p) = event {
                assert_eq!(p.state, "complete", "{:?}", p.metadata_errors);
                finished.borrow_mut().push(p.id);
            }
        });
        assert_eq!(*finished.borrow(), vec![a.id, b.id]);
        assert!(!Path::new(&draft.rows[0].target).exists());
        assert!(!f.store.queue_worker.load(Ordering::SeqCst));
        assert!(queue::state(&f.store).unwrap().ids.is_empty());
    }
    #[test]
    fn queue_stops_on_first_file_failure_and_resumes_remaining_tasks_explicitly() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let a = f.plan(
            vec![
                f.media("01.mkv", b"one", "1"),
                f.media("02.mkv", b"two", "2"),
            ],
            "copy",
        );
        let b = f.plan(vec![f.media("03.mkv", b"three", "3")], "copy");
        queue::enqueue(&f.store, &a.id).unwrap();
        queue::enqueue(&f.store, &b.id).unwrap();
        fs::write(&a.rows[0].source, b"changed size").unwrap();
        queue::drain(f.store.clone(), |_| {});
        let failed = f.store.get::<Plan>("plans", &a.id).unwrap().unwrap();
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.rows[1].state, "pending");
        assert!(!Path::new(&failed.rows[1].target).exists());
        assert!(!Path::new(&b.rows[0].target).exists());
        let q = queue::state(&f.store).unwrap();
        assert!(q.paused && !q.error.is_empty());
        assert_eq!(q.ids, vec![b.id.clone()]);
        assert!(!queue::reserve_worker(&f.store).unwrap());
        queue::set_paused(&f.store, false).unwrap();
        queue::drain(f.store.clone(), |_| {});
        assert_eq!(
            f.store.get::<Plan>("plans", &b.id).unwrap().unwrap().state,
            "complete"
        );
    }
    #[test]
    fn queue_cancel_restart_and_duplicate_reservations_are_durable() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let a = f.plan(vec![f.media("01.mkv", b"one", "1")], "copy");
        let b = f.plan(vec![f.media("02.mkv", b"two", "2")], "copy");
        let duplicate = f.plan(vec![f.media("other.mkv", b"one", "1")], "copy");
        queue::enqueue(&f.store, &a.id).unwrap();
        queue::enqueue(&f.store, &a.id).unwrap();
        assert!(queue::enqueue(&f.store, &duplicate.id)
            .unwrap_err()
            .contains("重复"));
        queue::enqueue(&f.store, &b.id).unwrap();
        assert!(decisions(&f.store, &a.id, vec![]).is_err());
        assert_eq!(queue::cancel(&f.store, &a.id).unwrap().state, "cancelled");
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        let q = queue::state(&reopened).unwrap();
        assert_eq!(q.ids, vec![b.id.clone()]);
        assert!(q.paused);
        assert!(!queue::reserve_worker(&reopened).unwrap());
        queue::set_paused(&reopened, false).unwrap();
        queue::drain(reopened.clone(), |_| {});
        assert!(Path::new(&b.rows[0].target).exists());
        assert!(!Path::new(&a.rows[0].target).exists());
    }
    #[test]
    fn another_task_can_be_created_and_configured_during_background_execution() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let a = f.plan(vec![f.media("01.mkv", b"one", "1")], "copy");
        start_plan(&f.store, &a.id).unwrap();
        let b = f.plan(vec![f.media("02.mkv", b"two", "2")], "copy");
        assert_eq!(queue::enqueue(&f.store, &b.id).unwrap().state, "queued");
        let result = f.run(&a);
        finish_plan(&f.store, &a.id, None).unwrap();
        assert_eq!(result.state, "complete");
        queue::drain(f.store.clone(), |_| {});
        let result = f.store.get::<Plan>("plans", &b.id).unwrap().unwrap();
        assert_eq!(result.state, "complete", "{:?}", result.metadata_errors);
    }
    #[test]
    fn queue_drains_tasks_added_while_its_worker_is_running() {
        use crate::task_queue as queue;
        let f = Fixture::new();
        let a = f.plan(vec![f.media("01.mkv", b"one", "1")], "copy");
        let added = std::cell::RefCell::new(None::<Plan>);
        queue::enqueue(&f.store, &a.id).unwrap();
        assert!(queue::reserve_worker(&f.store).unwrap());
        queue::drain(f.store.clone(), |event| {
            if let queue::Event::Progress(p) = event {
                if p.id == a.id && added.borrow().is_none() {
                    let b = f.plan(vec![f.media("02.mkv", b"two", "2")], "copy");
                    queue::enqueue(&f.store, &b.id).unwrap();
                    assert!(!queue::reserve_worker(&f.store).unwrap());
                    *added.borrow_mut() = Some(b);
                }
            }
        });
        let b = added.into_inner().unwrap();
        assert_eq!(
            f.store.get::<Plan>("plans", &b.id).unwrap().unwrap().state,
            "complete"
        );
        assert_eq!(fs::read(&b.rows[0].target).unwrap(), b"two");
        assert!(!f.store.running.load(Ordering::SeqCst));
        assert!(!f.store.queue_worker.load(Ordering::SeqCst));
    }
    #[test]
    fn queue_pauses_on_metadata_and_late_preflight_failures() {
        use crate::task_queue as queue;
        for metadata_failure in [true, false] {
            let f = Fixture::new();
            let a = f.plan(vec![f.media("01.mkv", b"one", "1")], "copy");
            let b = f.plan(vec![f.media("02.mkv", b"two", "2")], "copy");
            queue::enqueue(&f.store, &a.id).unwrap();
            queue::enqueue(&f.store, &b.id).unwrap();
            if metadata_failure {
                let show_root = Path::new(&a.rows[0].target)
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap();
                fs::create_dir_all(show_root.join("tvshow.nfo")).unwrap();
            } else {
                let mut work = a.works[0].clone();
                work.title = "changed after enqueue".into();
                f.store.put("works", &work.id, &work).unwrap();
            }
            assert!(queue::reserve_worker(&f.store).unwrap());
            queue::drain(f.store.clone(), |_| {});
            let failed = f.store.get::<Plan>("plans", &a.id).unwrap().unwrap();
            assert_eq!(
                failed.state,
                if metadata_failure {
                    "metadata_failed"
                } else {
                    "failed"
                }
            );
            assert!(!failed.metadata_errors.is_empty());
            assert!(!Path::new(&b.rows[0].target).exists());
            let q = queue::state(&f.store).unwrap();
            assert!(q.paused && !q.error.is_empty());
            assert_eq!(q.ids, vec![b.id]);
            assert!(!f.store.running.load(Ordering::SeqCst));
            assert!(!f.store.queue_worker.load(Ordering::SeqCst));
        }
    }
    #[test]
    fn persisted_plan_survives_store_restart() {
        let f = Fixture::new();
        let mut p = f.plan(vec![f.media("01.mkv", b"source", "1")], "copy");
        p.state = "running".into();
        f.store.put("plans", &p.id, &p).unwrap();
        let reopened = Store::open(f.store.data_dir.clone()).unwrap();
        assert_eq!(
            reopened.get::<Plan>("plans", &p.id).unwrap().unwrap().state,
            "paused"
        );
        assert_eq!(execute(reopened, p.id, |_| {}).unwrap().state, "complete");
    }
}
