use super::{analysis::*, process};
use crate::{copy, db::Store, model::*, musicbrainz, naming, paths};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackChoice {
    pub number: u32,
    pub remote_id: String,
    pub title: String,
    pub performer: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRequest {
    pub analysis_id: String,
    pub release_id: String,
    pub work_id: Option<String>,
    pub disc: u32,
    pub title: String,
    pub artist: String,
    pub date: String,
    pub tracks: Vec<TrackChoice>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    #[serde(default)]
    pub cleanup: super::cleanup::Cleanup,
    pub analysis: Analysis,
    pub original_work: Option<Work>,
    pub plan: Plan,
    pub ffmpeg: String,
    pub request: SplitRequest,
    pub total_discs: u32,
    pub total_tracks: usize,
}
pub fn context(store: &Store, id: &str) -> Result<Context> {
    store
        .get("cue_tasks", id)?
        .ok_or("CUE recovery context is missing".into())
}
pub fn preview(store: &Store, request: SplitRequest) -> Result<Plan> {
    store
        .diagnostics
        .measure("cue.preview", serde_json::json!({}), || {
            let a: Analysis = store
                .get("cue_analyses", &request.analysis_id)?
                .ok_or("Analyze CUE first")?;
            verify(store, &a.cue)?;
            for source in &a.sources {
                verify(store, &source.file)?;
            }
            if request.title.trim().is_empty()
                || request.disc == 0
                || request.disc > 999
                || request.tracks.len() != a.tracks.len()
            {
                return Err("Confirm album title, disc and every CUE track".into());
            }
            let subject = if request.release_id.is_empty() {
                None
            } else {
                Some(musicbrainz::detail(
                    store,
                    &request.release_id,
                    false,
                    None,
                )?)
            };
            let existing = if let Some(id) = &request.work_id {
                Some(
                    store
                        .get::<Work>("works", id)?
                        .ok_or("Local album no longer exists")?,
                )
            } else {
                store.list::<Work>("works")?.into_iter().find(|w| {
                    !request.release_id.is_empty()
                        && w.kind == "music"
                        && w.bindings.iter().any(|b| {
                            b.provider == "musicbrainz" && b.remote_id == request.release_id
                        })
                })
            };
            if existing.as_ref().is_some_and(|w| w.kind != "music") {
                return Err("CUE requires a music album".into());
            }
            let mut work = existing.clone().unwrap_or_else(|| Work {
                id: id(),
                kind: "music".into(),
                ..Default::default()
            });
            if let Some(subject) = &subject {
                work.info = subject.info.clone();
                work.poster = subject.poster.clone();
                work.overview = subject.overview.clone();
            }
            work.title = request.title.trim().into();
            work.info.artist = request.artist.clone();
            work.info.air_date = request.date.clone();
            work.year = request.date.chars().take(4).collect();
            if work.folder_name.is_empty() {
                work.folder_name = paths::clean_name(&work.title);
            }
            let provider = if subject.is_some() {
                "musicbrainz"
            } else {
                "cue"
            };
            let binding_id = work
                .bindings
                .iter()
                .find(|b| {
                    b.provider == provider
                        && (provider == "cue" || b.remote_id == request.release_id)
                })
                .map(|b| b.id.clone())
                .unwrap_or_else(id);
            if !work.bindings.iter().any(|b| b.id == binding_id) {
                work.bindings.push(Binding {
                    id: binding_id.clone(),
                    provider: provider.into(),
                    remote_id: request.release_id.clone(),
                    source_kind: "music".into(),
                    source_title: work.title.clone(),
                    season: request.disc,
                    episode_group: None,
                });
            }
            if !work
                .seasons
                .iter()
                .any(|s| s.binding_id == binding_id && s.number == request.disc)
            {
                work.seasons.push(Season {
                    binding_id: binding_id.clone(),
                    number: request.disc,
                    remote_number: request.disc,
                    title: format!("Disc {:02}", request.disc),
                    ..Default::default()
                });
            }
            let settings = store.settings()?;
            paths::validate_settings(&settings)?;
            let root = naming::root(&settings, "music")?;
            let mut snapshot = settings.clone();
            snapshot.tmdb_token.clear();
            let mut plan = Plan {
                kind: "cue_split".into(),
                settings: Some(snapshot),
                id: id(),
                created_at: now(),
                state: "draft".into(),
                rows: vec![],
                music_root: paths::display(&root),
                tv_root: naming::root(&settings, "tv")
                    .map(|p| paths::display(&p))
                    .unwrap_or_default(),
                movie_root: naming::root(&settings, "movie")
                    .map(|p| paths::display(&p))
                    .unwrap_or_default(),
                extras_root: settings.extras_root.clone(),
                backup_root: settings.backup_root.clone(),
                verify_hash: true,
                works: vec![],
                metadata_errors: vec![],
                metadata_notices: vec![],
                cleanup_notices: vec![],
            };
            let mut selected = HashSet::new();
            let mut targets = HashSet::new();
            for (track, choice) in a.tracks.iter().zip(&request.tracks) {
                if choice.number != track.number || choice.title.trim().is_empty() {
                    return Err("Confirm each CUE track title in order".into());
                }
                let remote = subject
                    .as_ref()
                    .map(|s| {
                        s.episodes
                            .iter()
                            .find(|e| {
                                e.id == choice.remote_id && e.season == request.disc && e.sort > 0.0
                            })
                            .ok_or("Choose a matching track on the selected disc")
                    })
                    .transpose()?;
                let number = remote.map(|e| e.sort as u32).unwrap_or(track.number);
                if !selected.insert(number) {
                    return Err("Two CUE tracks cannot map to the same album track".into());
                }
                let position = work
                    .episodes
                    .iter()
                    .position(|e| e.season == request.disc && e.number == number);
                let mut episode = position
                    .map(|i| work.episodes[i].clone())
                    .unwrap_or_else(|| Episode {
                        id: id(),
                        ..Default::default()
                    });
                episode.binding_id = binding_id.clone();
                episode.remote_id = remote
                    .map(|e| e.id.clone())
                    .unwrap_or_else(|| format!("nfo:{}:{number}", request.disc));
                episode.title = choice.title.trim().into();
                episode.number = number;
                episode.sort = number as f64;
                episode.season = request.disc;
                episode.enabled = true;
                episode.info = remote.map(|e| e.info.clone()).unwrap_or_default();
                episode.info.artist = choice.performer.clone();
                if !track.isrc.is_empty() {
                    episode
                        .info
                        .external_ids
                        .insert("isrc".into(), track.isrc.clone());
                }
                if let Some(i) = position {
                    work.episodes[i] = episode.clone();
                } else {
                    work.episodes.push(episode.clone());
                }
                let synthetic = Path::new("track").with_extension(&track.extension);
                let (target, folder) =
                    naming::target(&settings, &work, &[&episode], &synthetic, None)?;
                paths::safe_target_entry(&target, &root)?;
                if !targets.insert(paths::key(&target))
                    || a.sources
                        .iter()
                        .any(|s| paths::key(Path::new(&s.file.path)) == paths::key(&target))
                {
                    return Err("CUE output paths collide".into());
                }
                let mapping = Mapping {
                    source: a.sources[track.segments[0].source].file.path.clone(),
                    work_id: work.id.clone(),
                    episode_ids: vec![episode.id.clone()],
                    extra: false,
                    part: 0,
                };
                let mut row = copy::new_row(store, &mapping, &work, target, false, None, None)?;
                row.operation = "split".into();
                row.media_folder = paths::display(&folder);
                row.warnings = a.warnings.clone();
                if remote.and_then(|e| e.info.duration_ms).is_some_and(|ms| {
                    (ms as f64 / 1000.0 - track.duration).abs() > 2f64.max(track.duration * 0.01)
                }) {
                    row.warnings
                        .push("Database track duration differs from CUE".into());
                }
                plan.rows.push(row);
            }
            if let Some(subject) = &subject {
                if subject
                    .episodes
                    .iter()
                    .filter(|e| e.season == request.disc && e.sort > 0.0)
                    .count()
                    != a.tracks.len()
                {
                    for row in &mut plan.rows {
                        row.warnings.push(
                            "CUE and database track counts differ; confirm manual mapping".into(),
                        );
                    }
                }
            }
            plan.works.push(work);
            let backup_root = paths::normalize(&plan.backup_root)?;
            for i in 0..plan.rows.len() {
                let mut backups = copy::backup_candidates(&plan, i)?;
                for (n, b) in backups.iter_mut().enumerate() {
                    b.backup = paths::display(
                        &backup_root
                            .join(&plan.id)
                            .join(&plan.rows[i].id)
                            .join(format!(
                                "{n:03}-{}",
                                Path::new(&b.original)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )),
                    );
                }
                plan.rows[i].backups = backups;
            }
            let total_discs = subject
                .as_ref()
                .and_then(|s| s.seasons.iter().map(|s| s.number).max())
                .unwrap_or(request.disc);
            let total_tracks = subject
                .as_ref()
                .map(|s| {
                    s.episodes
                        .iter()
                        .filter(|e| e.season == request.disc && e.sort > 0.0)
                        .count()
                })
                .unwrap_or(a.tracks.len());
            let ctx = Context {
                cleanup: Default::default(),
                analysis: a,
                original_work: existing,
                ffmpeg: process::ffmpeg(store)?,
                request,
                plan: plan.clone(),
                total_discs,
                total_tracks,
            };
            store.put("cue_tasks", &plan.id, &ctx)?;
            Ok(plan)
        })
}
pub fn preflight(store: &Store, plan: &Plan, creating: bool) -> Result<()> {
    let ctx = context(store, &plan.id)?;
    if plan.rows.len() != ctx.plan.rows.len()
        || plan.kind != "cue_split"
        || plan.works.len() != 1
        || serde_json::to_value(&plan.works).unwrap()
            != serde_json::to_value(&ctx.plan.works).unwrap()
    {
        return Err("Invalid CUE plan".into());
    }
    let settings = store.settings()?;
    if plan.music_root != paths::display(&naming::root(&settings, "music")?)
        || plan.backup_root != settings.backup_root
        || process::ffmpeg(store)? != ctx.ffmpeg
        || plan
            .settings
            .as_ref()
            .is_none_or(|s| s.ffprobe != settings.ffprobe)
        || serde_json::to_value(
            &plan
                .settings
                .as_ref()
                .ok_or("Settings snapshot missing")?
                .naming,
        )
        .unwrap()
            != serde_json::to_value(&settings.naming).unwrap()
    {
        return Err("Audio tools or library settings changed; preview again".into());
    }
    for file in ctx
        .analysis
        .sources
        .iter()
        .map(|s| &s.file)
        .chain([&ctx.analysis.cue])
    {
        if !creating && ctx.cleanup.started {
            continue;
        }
        if creating {
            verify(store, file)?;
        } else if crate::resources::regular(Path::new(&file.path), None)? != file.signature {
            return Err("CUE input changed; preview again".into());
        }
    }
    let current = store.get::<Work>("works", &plan.works[0].id)?;
    if creating {
        if serde_json::to_value(current).unwrap()
            != serde_json::to_value(&ctx.original_work).unwrap()
        {
            return Err("Album changed; preview CUE again".into());
        }
    } else {
        let current = current.ok_or("Album no longer exists")?;
        let planned = &plan.works[0];
        if current.title != planned.title
            || current.folder_name != planned.folder_name
            || current.year != planned.year
            || serde_json::to_value(&current.info).unwrap()
                != serde_json::to_value(&planned.info).unwrap()
        {
            return Err("Album changed after CUE preview".into());
        }
        for row in &plan.rows {
            for id in &row.episode_ids {
                if serde_json::to_value(current.episodes.iter().find(|e| &e.id == id)).unwrap()
                    != serde_json::to_value(planned.episodes.iter().find(|e| &e.id == id)).unwrap()
                {
                    return Err("Track mapping changed after CUE preview".into());
                }
            }
        }
    }
    for (row, original) in plan.rows.iter().zip(&ctx.plan.rows) {
        if row.source != original.source
            || row.target != original.target
            || row.episode_ids != original.episode_ids
            || row.operation != "split"
        {
            return Err("Invalid CUE file operation".into());
        }
        if !["copy", "replace", "skip"].contains(&row.action.as_str())
            || row.action != "skip" && !row.warnings.is_empty() && !row.reviewed
        {
            return Err("Resolve CUE conflicts and confirm warnings".into());
        }
        if row.action == "copy" && (row.target_signature.is_some() || !row.conflicts.is_empty()) {
            return Err("Existing CUE output requires explicit replacement or skip".into());
        }
        paths::safe_target_entry(Path::new(&row.target), Path::new(&plan.music_root))?;
        if creating && paths::optional_signature(Path::new(&row.target))? != row.target_signature {
            return Err("CUE destination changed; preview again".into());
        }
    }
    Ok(())
}
pub fn submit(store: &Store, id: &str, choices: Vec<Decision>) -> Result<Plan> {
    if let Some(plan) = store.get("plans", id)? {
        return Ok(plan);
    }
    let ctx = context(store, id)?;
    let mut plan = ctx.plan;
    if choices.len() != plan.rows.len() {
        return Err("Confirm every CUE output".into());
    }
    let mut seen = HashSet::new();
    for choice in choices {
        if !seen.insert(choice.id.clone()) || choice.operation != "split" {
            return Err("Invalid CUE decisions".into());
        }
        let row = plan
            .rows
            .iter_mut()
            .find(|r| r.id == choice.id)
            .ok_or("Unknown CUE output")?;
        row.action = choice.action;
        row.reviewed = choice.reviewed;
    }
    preflight(store, &plan, true)?;
    let inputs = input_paths(store, &plan)?;
    for other in store.list::<Plan>("plans")?.iter().filter(|p| {
        p.id != plan.id
            && (["queued", "running", "cancelling"].contains(&p.state.as_str())
                || (p.kind == "cue_split"
                    && !["complete", "cancelled"].contains(&p.state.as_str())))
    }) {
        if !inputs.is_disjoint(&input_paths(store, other)?)
            || other
                .rows
                .iter()
                .any(|r| r.action != "skip" && inputs.contains(&paths::key(Path::new(&r.target))))
        {
            return Err("An audio source or destination is reserved by another task".into());
        }
    }
    if plan.rows.iter().all(|r| r.action == "skip") {
        return Err("Select at least one CUE track".into());
    }
    let mut conn = store.connection()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (collection, id, data) in [
        (
            "works",
            &plan.works[0].id,
            serde_json::to_string(&plan.works[0]),
        ),
        ("plans", &plan.id, serde_json::to_string(&plan)),
    ] {
        tx.execute("INSERT INTO documents(collection,id,data) VALUES(?1,?2,?3) ON CONFLICT(collection,id) DO UPDATE SET data=excluded.data", rusqlite::params![collection,id,data.map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(plan)
}
pub fn input_paths(store: &Store, plan: &Plan) -> Result<HashSet<String>> {
    if plan.kind != "cue_split" {
        return Ok(plan
            .rows
            .iter()
            .filter(|r| r.action != "skip")
            .map(|r| paths::key(Path::new(&r.source)))
            .collect());
    }
    let context = context(store, &plan.id)?;
    Ok(context
        .analysis
        .sources
        .iter()
        .map(|s| paths::key(Path::new(&s.file.path)))
        .chain([paths::key(Path::new(&context.analysis.cue.path))])
        .collect())
}
