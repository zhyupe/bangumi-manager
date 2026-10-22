use crate::{copy, db::Store, model::*, paths, resources};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const DEFAULT_PATTERN: &str =
    r"([a-z]{2,3}(?:-[a-z0-9_]+)*(?:&[a-z]{2,3}(?:-[a-z0-9_]+)*)*)(?:_[^.]+)?";
static SAFE_LANGUAGE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9&_-]*$").unwrap());

pub fn language_pattern(pattern: &str) -> Result<regex::Regex> {
    let pattern = if pattern.trim().is_empty() {
        DEFAULT_PATTERN
    } else {
        pattern.trim()
    };
    if pattern.len() > 512 || pattern.replace("(?:", "").contains("(?") {
        return Err("字幕语言正则无效：不支持特殊分组或超过 512 字符的表达式".into());
    }
    let regex = regex::RegexBuilder::new(&format!(r"^(?:{})$", pattern))
        .case_insensitive(true)
        .build()
        .map_err(|_| "字幕语言正则无效，请检查表达式".to_string())?;
    if regex.captures_len() < 2 {
        return Err("字幕语言正则必须包含第一捕获组，用于保留语言标记".into());
    }
    Ok(regex)
}

pub(crate) fn language(stem: &str, pattern: &str) -> Result<Option<String>> {
    let Some((_, segment)) = stem.rsplit_once('.') else {
        return Ok(None);
    };
    let regex = language_pattern(pattern)?;
    Ok(regex.captures(segment).and_then(|c| {
        if c.get(0)?.as_str() != segment {
            return None;
        }
        let value = c.get(1)?.as_str();
        SAFE_LANGUAGE.is_match(value).then(|| value.to_string())
    }))
}

fn destination(source: &Path, video: &Path, pattern: &str) -> Result<PathBuf> {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let language = language(&stem, pattern)?
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    Ok(video.with_file_name(format!(
        "{}{}.{}",
        video.file_stem().unwrap_or_default().to_string_lossy(),
        language,
        paths::ext(source)
    )))
}

fn matching_asset<'a>(index: &'a LibraryIndex, work: &Work, ids: &[String], part: u32) -> Result<&'a Asset> {
    if work.kind == "music" {
        return Err("字幕只能匹配到电影或剧集的视频文件".into());
    }
    let eps = copy::episode_range(work, ids)?;
    let candidates: Vec<_> = index
        .assets
        .iter()
        .filter(|a| {
            a.work_id == work.id
                && paths::video(Path::new(&a.path))
                && eps
                    .iter()
                    .any(|e| e.season == a.season && (a.number..=a.end).contains(&e.number))
        })
        .collect();
    let matches: Vec<_> = candidates.iter().copied().filter(|a| a.part == part).collect();
    let asset = match matches.as_slice() {
        [] if !candidates.is_empty() && part == 0 => return Err("对应视频已分段，请为字幕选择对应的 Part 编号".into()),
        [] if !candidates.is_empty() => return Err("未找到对应 Part 的已入库视频，请先整理该分段".into()),
        [] => return Err("对应集数尚无已入库的视频文件，请先整理视频后再导入字幕".into()),
        [asset] => *asset,
        _ => return Err("对应集数存在多个视频版本，请先确认并整理版本后再导入字幕".into()),
    };
    if eps[0].season != asset.season
        || eps[0].number != asset.number
        || eps.last().unwrap().number != asset.end
    {
        return Err("字幕必须匹配对应视频的完整集数范围".into());
    }
    Ok(asset)
}

fn media_folder(store: &Store, video: &Path, root: &Path, work_id: &str) -> Result<PathBuf> {
    for parent in video
        .ancestors()
        .skip(1)
        .take_while(|p| paths::contained(p, root))
    {
        if store
            .get::<String>("folders", &paths::key(parent))?
            .as_deref()
            == Some(work_id)
        {
            return Ok(parent.to_path_buf());
        }
    }
    Err("资源目录尚未明确关联作品，请先扫描并关联".into())
}

pub fn new_row(
    store: &Store,
    settings: &Settings,
    index: &LibraryIndex,
    mapping: &Mapping,
    work: &Work,
) -> Result<PlanRow> {
    let asset = matching_asset(index, work, &mapping.episode_ids, mapping.part)?;
    let video = paths::normalize(&asset.path)?;
    let source = paths::normalize(&mapping.source)?;
    let root = resources::root_for(settings, &video)?;
    if resources::regular(&video, Some(&root))? != asset.signature {
        return Err("字幕对应的视频已变化，请重新扫描并生成计划".into());
    }
    let target = destination(&source, &video, &settings.subtitle_language_pattern)?;
    paths::safe_target_entry(&target, &root)?;
    let folder = media_folder(store, &video, &root, &work.id)?;
    let mut row = copy::new_row(store, mapping, work, target, true, None, None)?;
    row.subtitle = Some(SubtitleContext {
        asset: asset.clone(),
        root: paths::display(&root),
    });
    row.media_folder = paths::display(&folder);
    Ok(row)
}

fn validate_episodes(store: &Store, asset: &Asset, work: &Work, ids: &[String]) -> Result<()> {
    let mut expected = crate::tmdb_groups::episode_ids(store, asset, work)?;
    let mut selected = ids.to_vec();
    expected.sort();
    selected.sort();
    if expected != selected {
        return Err("字幕对应的视频关联已变化，请重新生成计划".into());
    }
    Ok(())
}

pub fn validate_row(store: &Store, plan: &Plan, row: &PlanRow) -> Result<()> {
    let Some(context) = &row.subtitle else {
        return Ok(());
    };
    if row.action == "skip" || row.state == "complete" {
        return Ok(());
    }
    let work = plan
        .works
        .iter()
        .find(|w| w.id == row.work_id)
        .ok_or("条目快照不存在")?;
    let index = store
        .get::<LibraryIndex>("index", "main")?
        .unwrap_or_default();
    let current = matching_asset(&index, work, &row.episode_ids, row.part)?;
    validate_episodes(store, current, work, &row.episode_ids)?;
    let video = paths::normalize(&context.asset.path)?;
    let root = paths::normalize(&context.root)?;
    let source = paths::normalize(&row.source)?;
    let target = paths::normalize(&row.target)?;
    if !row.attachment
        || row.extra
        || row.parent_id.is_some()
        || !paths::subtitle(&source)
        || serde_json::to_value(current).unwrap() != serde_json::to_value(&context.asset).unwrap()
        || paths::key(&resources::root_for(&store.settings()?, &video)?) != paths::key(&root)
        || paths::key(&destination(
            &source,
            &video,
            plan
                .settings
                .as_ref()
                .map(|s| s.subtitle_language_pattern.as_str())
                .unwrap_or(""),
        )?) != paths::key(&target)
        || paths::key(&media_folder(store, &video, &root, &row.work_id)?)
            != paths::key(Path::new(&row.media_folder))
    {
        return Err("字幕对应的视频关联已变化，请重新生成计划".into());
    }
    paths::safe_target_entry(&target, &root)?;
    if resources::regular(&video, Some(&root))? != context.asset.signature {
        return Err("字幕对应的视频已变化，请重新扫描并生成计划".into());
    }
    for other in plan
        .rows
        .iter()
        .filter(|r| !r.attachment && !r.extra && r.action != "skip")
    {
        if other.work_id == row.work_id
            && (other.part == 0 || row.part == 0 || other.part == row.part)
            && copy::episode_range(work, &other.episode_ids)?
                .iter()
                .any(|e| {
                    e.season == current.season && (current.number..=current.end).contains(&e.number)
                })
        {
            return Err("字幕对应的视频也在本计划中整理，请先完成视频整理后再导入字幕".into());
        }
    }
    Ok(())
}

pub fn validate_context(store: &Store, plan: &Plan) -> Result<()> {
    for row in plan.rows.iter().filter(|r| r.subtitle.is_some()) {
        validate_row(store, plan, row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::Arc};

    struct Fixture {
        dir: tempfile::TempDir,
        store: Arc<Store>,
        folder: PathBuf,
    }
    impl Fixture {
        fn new(kind: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::open(dir.path().join("state")).unwrap();
            let settings = Settings {
                tv_root: paths::display(&dir.path().join("tv")),
                movie_root: paths::display(&dir.path().join("movies")),
                extras_root: paths::display(&dir.path().join("extras")),
                backup_root: paths::display(&dir.path().join("backups")),
                ffprobe: "__no_probe_in_subtitle_tests__".into(),
                verify_hash: true,
                ..Default::default()
            };
            for root in [
                &settings.tv_root,
                &settings.movie_root,
                &settings.extras_root,
                &settings.backup_root,
            ] {
                fs::create_dir_all(root).unwrap();
            }
            store.put("settings", "main", &settings).unwrap();
            store
                .put(
                    "works",
                    "show",
                    &Work {
                        id: "show".into(),
                        title: "Fictional Voyage".into(),
                        folder_name: "Fictional Voyage".into(),
                        kind: kind.into(),
                        episodes: (1..=3)
                            .map(|n| Episode {
                                id: n.to_string(),
                                season: 1,
                                number: n,
                                enabled: true,
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    },
                )
                .unwrap();
            let folder = Path::new(if kind == "movie" {
                &settings.movie_root
            } else {
                &settings.tv_root
            })
            .join("Legacy folder");
            fs::create_dir_all(&folder).unwrap();
            store.put("folders", &paths::key(&folder), &"show").unwrap();
            Self { dir, store, folder }
        }
        fn video(&self, name: &str) -> PathBuf {
            let path = self.folder.join(name);
            fs::write(&path, b"original video").unwrap();
            path
        }
        fn mapping(&self, name: &str, ids: &[&str]) -> Mapping {
            let path = self.dir.path().join("incoming").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"new subtitle").unwrap();
            Mapping {
                part: 0,
                source: paths::display(&path),
                work_id: "show".into(),
                episode_ids: ids.iter().map(|v| v.to_string()).collect(),
                extra: false,
            }
        }
        fn plan(&self, mappings: Vec<Mapping>) -> Result<Plan> {
            copy::create_plan(&self.store, mappings, |_, _| {})
        }
        fn run(&self, plan: &Plan) -> Result<Plan> {
            copy::execute(self.store.clone(), plan.id.clone(), |_| {})
        }
    }

    #[test]
    fn custom_language_rules_validate_and_use_safe_captures() {
        assert_eq!(
            language("Release.subtitle-CHS_123", "subtitle-([a-z]+)_[0-9]+").unwrap(),
            Some("CHS".into())
        );
        assert_eq!(language("Release.sc_tag", "(sc)").unwrap(), None);
        assert_eq!(language("Release.sc/bad", "(.*)").unwrap(), None);
        for pattern in ["(", "sc_.*", "(?i)(sc)", "(?=(sc))"] {
            assert!(language_pattern(pattern).is_err());
        }
    }

    #[test]
    fn configured_subtitle_import_keeps_task_rules_after_settings_change() {
        let f = Fixture::new("tv");
        f.video("Legacy.S01E01.mkv");
        let mut settings = f.store.settings().unwrap();
        settings.subtitle_language_pattern = "subtitle-([a-z]+)_[0-9]+".into();
        f.store.put("settings", "main", &settings).unwrap();
        let mapping = f.mapping("Release.E01.subtitle-sc_123.ass", &["1"]);
        let plan = f.plan(vec![mapping]).unwrap();
        assert!(plan.rows[0].target.ends_with("Legacy.S01E01.sc.ass"));
        settings.subtitle_language_pattern.clear();
        f.store.put("settings", "main", &settings).unwrap();
        assert_eq!(f.run(&plan).unwrap().state, "complete");
        assert_eq!(
            fs::read(f.folder.join("Legacy.S01E01.sc.ass")).unwrap(),
            b"new subtitle"
        );
    }

    #[test]
    fn preserves_only_the_trailing_language_suffix() {
        for (source, expected) in [
            ("Release.E01.zh-Hans.ass", "S02E01.zh-Hans.ass"),
            ("Release.E01.EN.srt", "S02E01.EN.srt"),
            ("Release.E01.ja.vtt", "S02E01.ja.vtt"),
            ("Release.E01.ass", "S02E01.ass"),
            ("Release.01.ass", "S02E01.ass"),
            ("Release.E01.chs.ass", "S02E01.chs.ass"),
            ("Release.E01.sc_xxxxxx.ass", "S02E01.sc.ass"),
            ("Release.E01.cht.ass", "S02E01.cht.ass"),
            ("Release.E01.eng.srt", "S02E01.eng.srt"),
            ("Release.E01.zh-Hans-CN.ass", "S02E01.zh-Hans-CN.ass"),
            ("Release.E01.chs&jpn.ass", "S02E01.chs&jpn.ass"),
            ("Release.E01.CHT&JPN.ASS", "S02E01.CHT&JPN.ass"),
            ("Release.E01.zh-Hans&ja.srt", "S02E01.zh-Hans&ja.srt"),
            ("Release.E01.chs&jpn&eng.ass", "S02E01.chs&jpn&eng.ass"),
            ("Release.E01.chs&.ass", "S02E01.ass"),
            ("Release.E01.chs&123.ass", "S02E01.ass"),
        ] {
            assert_eq!(
                destination(Path::new(source), Path::new("S02E01.mkv"), "").unwrap(),
                PathBuf::from(expected)
            );
        }
    }

    #[test]
    fn standalone_subtitles_use_actual_video_name_and_leave_metadata_untouched() {
        let f = Fixture::new("tv");
        let video = f.video("Legacy.S01E01.1080p.mkv");
        let nfo = video.with_extension("nfo");
        fs::write(
            &nfo,
            b"<episodedetails><title>Fictional Chapter</title></episodedetails>",
        )
        .unwrap();
        let nfo_before = fs::read(&nfo).unwrap();
        let a = f.mapping("Release.E01.zh-Hans.ass", &["1"]);
        let b = f.mapping("Another.E01.en.srt", &["1"]);
        let plan = f.plan(vec![a, b]).unwrap();
        assert!(plan
            .rows
            .iter()
            .all(|r| r.attachment && r.parent_id.is_none() && r.conflicts.is_empty()));
        assert_eq!(plan.rows.len(), 2);
        let completed = f.run(&plan).unwrap();
        assert_eq!(completed.state, "complete");
        assert_eq!(
            fs::read(f.folder.join("Legacy.S01E01.1080p.zh-Hans.ass")).unwrap(),
            b"new subtitle"
        );
        assert_eq!(
            fs::read(f.folder.join("Legacy.S01E01.1080p.en.srt")).unwrap(),
            b"new subtitle"
        );
        assert_eq!(fs::read(&video).unwrap(), b"original video");
        assert_eq!(fs::read(nfo).unwrap(), nfo_before);
        assert_eq!(
            crate::scan::index_library(&f.store).unwrap().assets.len(),
            1
        );
    }

    #[test]
    fn bilingual_subtitles_keep_distinct_names_when_imported_together() {
        let f = Fixture::new("tv");
        let video = f.video("Legacy.S01E01.mkv");
        let suffixes = [".ass", ".chs.ass", ".chs&jpn.ass", ".cht&jpn.ass"];
        let mappings = suffixes
            .iter()
            .map(|suffix| {
                let mapping = f.mapping(&format!("Release.E01{suffix}"), &["1"]);
                fs::write(&mapping.source, suffix.as_bytes()).unwrap();
                mapping
            })
            .collect();
        let plan = f.plan(mappings).unwrap();
        assert_eq!(plan.rows.len(), suffixes.len());
        assert!(plan.rows.iter().all(|row| row.conflicts.is_empty()));
        assert_eq!(f.run(&plan).unwrap().state, "complete");
        for suffix in suffixes {
            assert_eq!(
                fs::read(f.folder.join(format!("Legacy.S01E01{suffix}"))).unwrap(),
                suffix.as_bytes()
            );
        }
        assert_eq!(fs::read(video).unwrap(), b"original video");
    }

    #[test]
    fn standalone_movie_subtitle_uses_the_existing_movie_basename() {
        let f = Fixture::new("movie");
        let video = f.video("Fictional Feature.mkv");
        let plan = f.plan(vec![f.mapping("Captions.zh.srt", &["1"])]).unwrap();
        assert_eq!(
            Path::new(&plan.rows[0].target),
            video.with_file_name("Fictional Feature.zh.srt")
        );
        assert_eq!(f.run(&plan).unwrap().state, "complete");
    }

    #[test]
    fn standalone_subtitle_selects_the_requested_part_and_preserves_chs() {
        let f = Fixture::new("tv");
        let first = f.video("S01E01 - part1.mkv");
        let second = f.video("S01E01 - part2.mkv");
        let mut mapping = f.mapping("Fictional.E01.Part2.chs.ass", &["1"]);
        assert!(f.plan(vec![mapping.clone()]).is_err());
        mapping.part = 2;
        let plan = f.plan(vec![mapping]).unwrap();
        assert_eq!(
            plan.rows[0].target,
            paths::display(&f.folder.join("S01E01 - part2.chs.ass"))
        );
        assert_eq!(f.run(&plan).unwrap().state, "complete");
        assert!(first.exists() && second.exists());
        assert!(!f.folder.join("S01E01 - part1.chs.ass").exists());
    }

    #[test]
    fn standalone_subtitle_requires_video_at_preview_and_at_execution() {
        let f = Fixture::new("tv");
        let mapping = f.mapping("Release.E01.zh.ass", &["1"]);
        assert!(f.plan(vec![mapping.clone()]).unwrap_err().contains("尚无"));
        let video = f.video("S01E01.mkv");
        let plan = f.plan(vec![mapping]).unwrap();
        fs::remove_file(&video).unwrap();
        assert!(f.run(&plan).is_err());
        assert!(!Path::new(&plan.rows[0].target).exists());
        assert!(Path::new(&plan.rows[0].source).exists());
    }

    #[test]
    fn standalone_subtitle_rejects_changed_video_or_association() {
        let f = Fixture::new("tv");
        let video = f.video("S01E01.mkv");
        let plan = f.plan(vec![f.mapping("Release.E01.srt", &["1"])]).unwrap();
        fs::write(video, b"changed video contents").unwrap();
        assert!(f.run(&plan).unwrap_err().contains("视频已变化"));
        let plan = f.plan(vec![f.mapping("Release.E01.srt", &["1"])]).unwrap();
        f.store
            .put("folders", &paths::key(&f.folder), &"different-work")
            .unwrap();
        assert!(f.run(&plan).is_err());
        assert!(!Path::new(&plan.rows[0].target).exists());
    }

    #[test]
    fn standalone_subtitle_requires_one_video_and_its_full_episode_range() {
        let f = Fixture::new("tv");
        f.video("S01E01-E02.mkv");
        assert!(f
            .plan(vec![f.mapping("Release.E01.ass", &["1"])])
            .unwrap_err()
            .contains("完整集数"));
        let plan = f
            .plan(vec![f.mapping("Release.E01-E02.zh.ass", &["1", "2"])])
            .unwrap();
        assert!(plan.rows[0].target.ends_with("S01E01-E02.zh.ass"));
        f.video("Another.S01E01-E02.mp4");
        assert!(f
            .plan(vec![f.mapping("Release.E01-E02.zh.ass", &["1", "2"])])
            .unwrap_err()
            .contains("多个视频版本"));
    }

    #[test]
    fn standalone_subtitle_replacement_backs_up_only_its_target_and_can_move() {
        let f = Fixture::new("tv");
        let video = f.video("S01E01.mkv");
        let target = f.folder.join("S01E01.zh.ass");
        fs::write(&target, b"old subtitle").unwrap();
        let sidecar = f.folder.join("S01E01.zh.nfo");
        fs::write(&sidecar, b"sidecar untouched").unwrap();
        let plan = f
            .plan(vec![f.mapping("Release.E01.zh.ass", &["1"])])
            .unwrap();
        assert_eq!(plan.rows[0].action, "undecided");
        assert!(f.run(&plan).is_err());
        let plan = copy::decisions(
            &f.store,
            &plan.id,
            vec![Decision {
                id: plan.rows[0].id.clone(),
                action: "replace".into(),
                operation: "move".into(),
                reviewed: true,
            }],
        )
        .unwrap();
        let completed = f.run(&plan).unwrap();
        assert_eq!(completed.state, "complete");
        assert_eq!(completed.rows[0].backups.len(), 1);
        assert_eq!(
            fs::read(&completed.rows[0].backups[0].backup).unwrap(),
            b"old subtitle"
        );
        assert_eq!(fs::read(target).unwrap(), b"new subtitle");
        assert_eq!(fs::read(video).unwrap(), b"original video");
        assert_eq!(fs::read(sidecar).unwrap(), b"sidecar untouched");
        assert!(!Path::new(&plan.rows[0].source).exists());
        assert_eq!(f.run(&completed).unwrap().state, "complete");
        f.store.remove("plans", &completed.id).unwrap();
        let backups = crate::backups::list(&f.store).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].state, "available");
    }

    #[test]
    fn standalone_subtitle_blocks_overlapping_video_tasks_and_changed_targets() {
        let f = Fixture::new("tv");
        f.video("S01E01.mkv");
        let sub = f.mapping("Captions.E01.zh.ass", &["1"]);
        let media = f.mapping("New.E01.mkv", &["1"]);
        assert!(f.plan(vec![sub.clone(), media]).is_err());
        let plan = f.plan(vec![sub]).unwrap();
        fs::write(&plan.rows[0].target, b"another subtitle").unwrap();
        let stopped = f.run(&plan).unwrap();
        assert_eq!(stopped.state, "failed");
        assert_eq!(fs::read(&plan.rows[0].target).unwrap(), b"another subtitle");
        fs::remove_file(&plan.rows[0].target).unwrap();
        assert_eq!(f.run(&stopped).unwrap().state, "complete");
        assert_eq!(fs::read(&plan.rows[0].target).unwrap(), b"new subtitle");
    }
}
