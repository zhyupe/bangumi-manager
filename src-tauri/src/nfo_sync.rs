use crate::{db::Store, library_import, model::*, nfo, paths};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NfoMatch {
    pub path: String,
    pub nfo_path: String,
    pub title: String,
    pub kind: String,
    pub ids: BTreeMap<String, String>,
    pub episode_count: usize,
    pub existing_title: String,
    pub fingerprint: String,
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct Selection {
    pub path: String,
    pub fingerprint: String,
}

#[derive(Debug, Default, Serialize)]
pub struct SyncResult {
    pub mappings: Vec<Mapping>,
    pub errors: Vec<NfoMatch>,
}

fn named(dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let direct = dir.join(name);
    if paths::optional_signature(&direct)?.is_some() {
        return Ok(Some(direct));
    }
    if cfg!(windows) {
        return Ok(None);
    }
    let mut matches = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err("存在多个同名 NFO，请先消除大小写歧义".into());
    }
    if let Some(path) = matches.pop() {
        paths::optional_signature(&path)?.ok_or("NFO 已不存在")?;
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

fn fingerprint(media: &Path, documents: &[PathBuf]) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(
        serde_json::to_vec(&paths::optional_signature(media)?.ok_or("媒体文件已不存在")?).unwrap(),
    );
    for path in documents {
        paths::optional_signature(path)?.ok_or("NFO 已不存在")?;
        let mut data = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 4 * 1024 * 1024 {
            return Err("NFO 超过 4 MB".into());
        }
        hash.update(paths::key(path).as_bytes());
        hash.update((data.len() as u64).to_le_bytes());
        hash.update(data);
    }
    Ok(hex::encode(hash.finalize()))
}

fn matching_work(store: &Store, work: &Work) -> Result<Option<Work>> {
    let matches = store
        .list::<Work>("works")?
        .into_iter()
        .filter(|old| {
            old.kind == work.kind
                && work
                    .info
                    .external_ids
                    .iter()
                    .any(|(provider, id)| library_import::same_id(old, provider, id))
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err("NFO ID 指向多个本地作品，请先核对关联".into());
    }
    if let Some(old) = matches.into_iter().next() {
        if store
            .get::<bool>("episode_order", &old.id)?
            .unwrap_or(false)
        {
            return Err("该作品已切换剧集组，请在匹配设置中核对集数".into());
        }
        library_import::merge_work(old.clone(), work)?;
        Ok(Some(old))
    } else {
        Ok(None)
    }
}

fn inspect_one(store: &Store, raw: &str) -> Result<Option<(NfoMatch, Work)>> {
    let media = paths::normalize(raw)?;
    if !paths::media(&media) {
        return Ok(None);
    }
    let parent = media.parent().ok_or("媒体文件没有父目录")?;
    let expected = media.with_extension("nfo");
    let Some(sidecar) = named(parent, &expected.file_name().unwrap().to_string_lossy())? else {
        return Ok(None);
    };
    let source = store
        .get::<SourceFile>("sources", &paths::key(&media))?
        .ok_or("文件不在扫描列表中，请重新扫描")?;
    if paths::optional_signature(&media)?.as_ref() != Some(&source.signature) {
        return Err("媒体文件已变化，请重新扫描".into());
    }
    let roots = nfo::read(&sidecar)?;
    let mut documents = vec![sidecar.clone()];
    let (kind, dir, main) = match roots.first().map(|n| n.name.as_str()) {
        Some("movie") if roots.len() == 1 && paths::video(&media) => {
            ("movie", parent.to_path_buf(), roots[0].clone())
        }
        Some(tag @ ("episodedetails" | "song")) if roots.iter().all(|n| n.name == tag) => {
            let (kind, root_tag, root_name) = if tag == "song" {
                ("music", "album", "album.nfo")
            } else {
                ("tv", "tvshow", "tvshow.nfo")
            };
            if (kind == "music") != paths::audio(&media) {
                return Err("NFO 类型与媒体文件不一致".into());
            }
            let mut context = None;
            // Only the containing directory and its parent can supply series/album identity.
            for dir in [Some(parent), parent.parent()].into_iter().flatten() {
                if let Some(path) = named(dir, root_name)? {
                    let main = nfo::one(&path, root_tag)?;
                    documents.push(path);
                    context = Some((kind, dir.to_path_buf(), main));
                    break;
                }
            }
            let context = context.ok_or(format!(
                "单集/音轨 ID 不能作为作品 ID，需要同目录或上级目录的 {root_name}"
            ))?;
            if let Some(path) = named(parent, "season.nfo")? {
                documents.push(path);
            }
            context
        }
        _ => return Err("同名 NFO 需为电影、单集或音轨资料".into()),
    };
    let ids = main.ids()?;
    if ids.is_empty() {
        return Err("NFO 中没有可识别的作品数据库 ID".into());
    }
    let before = fingerprint(&media, &documents)?;
    let fallback = if kind == "movie" {
        media
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    };
    let mut files = documents.clone();
    files.push(media.clone());
    let (mut work, _) = library_import::load(
        store,
        &dir,
        kind,
        "",
        &files,
        &store.settings()?.language,
        Some(&fallback),
    )?
    .ok_or("无法读取 NFO 资料")?;
    if work.info.external_ids != ids {
        return Err("NFO 读取期间发生变化，请重新检测".into());
    }
    work.folder_name = paths::clean_name(&work.title);
    crate::providers::validate_work(&work)?;
    if work.episodes.is_empty() {
        return Err("NFO 中没有可匹配的集数或音轨".into());
    }
    if fingerprint(&media, &documents)? != before {
        return Err("NFO 读取期间发生变化，请重新检测".into());
    }
    let existing = matching_work(store, &work)?;
    Ok(Some((
        NfoMatch {
            path: source.path,
            nfo_path: paths::display(&sidecar),
            title: work.title.clone(),
            kind: work.kind.clone(),
            ids,
            episode_count: work.episodes.len(),
            existing_title: existing.map(|w| w.title).unwrap_or_default(),
            fingerprint: before,
            error: String::new(),
        },
        work,
    )))
}

fn error_match(path: String, error: String) -> NfoMatch {
    NfoMatch {
        path,
        error,
        ..Default::default()
    }
}

pub fn inspect(store: &Store, paths: Vec<String>) -> Vec<NfoMatch> {
    paths
        .into_iter()
        .filter_map(|path| match inspect_one(store, &path) {
            Ok(Some((matched, _))) => Some(matched),
            Ok(None) => None,
            Err(error) => Some(error_match(path, error)),
        })
        .collect()
}

fn sync_one(store: &Store, selection: Selection) -> Result<Mapping> {
    let (current, candidate) = inspect_one(store, &selection.path)?.ok_or("同名 NFO 已不存在")?;
    if current.fingerprint != selection.fingerprint {
        return Err("媒体或 NFO 已变化，请重新检测并确认".into());
    }
    let work = match matching_work(store, &candidate)? {
        Some(old) => library_import::merge_work(old, &candidate)?,
        None => candidate.clone(),
    };
    let mut episode_ids = Vec::new();
    for ep in &candidate.episodes {
        let matches = work
            .episodes
            .iter()
            .filter(|e| {
                e.enabled
                    && if work.kind == "movie" {
                        true
                    } else {
                        e.season == ep.season && e.number == ep.number
                    }
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("本地集数存在歧义或已禁用，请手动匹配".into());
        }
        episode_ids.push(matches[0].id.clone());
    }
    crate::providers::validate_work(&work)?;
    store.put("works", &work.id, &work)?;
    Ok(Mapping {
        part: if work.kind == "music" { 0 } else { crate::multipart::number(Path::new(&current.path)) },
        source: current.path,
        work_id: work.id,
        episode_ids,
        extra: false,
    })
}

pub fn sync(store: &Store, selections: Vec<Selection>) -> Result<SyncResult> {
    let _queue = store
        .queue_lock
        .try_lock()
        .map_err(|_| "整理队列正忙，请稍后重试")?;
    if store.running.load(Ordering::SeqCst) {
        return Err("请先暂停整理任务再同步作品资料".into());
    }
    let _library = store
        .library_scan
        .try_lock()
        .map_err(|_| "媒体库正在扫描，请稍后重试")?;
    let mut result = SyncResult::default();
    for selection in selections {
        let path = selection.path.clone();
        match sync_one(store, selection) {
            Ok(mapping) => result.mappings.push(mapping),
            Err(error) => result.errors.push(error_match(path, error)),
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn inspect_source_nfos(
    state: tauri::State<'_, crate::AppState>,
    paths: Vec<String>,
) -> Result<Vec<NfoMatch>> {
    let store = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || inspect(&store, paths))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_source_nfos(
    state: tauri::State<'_, crate::AppState>,
    selections: Vec<Selection>,
) -> Result<SyncResult> {
    let store = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sync(&store, selections))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fixture {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        input: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::open(dir.path().join("db")).unwrap();
            let input = dir.path().join("incoming");
            fs::create_dir(&input).unwrap();
            Self {
                _dir: dir,
                store,
                input,
            }
        }
        fn media(&self, name: &str, xml: &str) -> String {
            let media = self.input.join(name);
            fs::create_dir_all(media.parent().unwrap()).unwrap();
            fs::write(&media, b"media fixture").unwrap();
            fs::write(media.with_extension("nfo"), xml).unwrap();
            let raw = paths::display(&media);
            self.store
                .put(
                    "sources",
                    &paths::key(&media),
                    &SourceFile {
                        path: raw.clone(),
                        parent: paths::display(media.parent().unwrap()),
                        name: media.file_name().unwrap().to_string_lossy().into(),
                        extension: paths::ext(&media),
                        signature: paths::signature(&media).unwrap(),
                        media: true,
                        hint: "episode".into(),
                        extra_relative_path: None,
                        probe: None,
                    },
                )
                .unwrap();
            raw
        }
        fn select(&self, path: &str) -> Selection {
            let matches = inspect(&self.store, vec![path.into()]);
            assert_eq!(matches.len(), 1);
            assert!(matches[0].error.is_empty(), "{}", matches[0].error);
            Selection {
                path: path.into(),
                fingerprint: matches[0].fingerprint.clone(),
            }
        }
    }
    const MOVIE: &str = "<movie><title>虚构影片甲</title><uniqueid type='tmdb'>910000001</uniqueid><imdbid>tt910000001</imdbid><actor><tmdbid>910000099</tmdbid></actor></movie>";

    #[test]
    fn movie_import_is_offline_idempotent_and_does_not_bind_shared_source_directory() {
        let f = Fixture::new();
        let path = f.media("sample.mkv", MOVIE);
        let selected = f.select(&path);
        let preview = inspect(&f.store, vec![path.clone()]);
        assert_eq!(preview[0].ids["tmdb"], "910000001");
        assert!(f.store.list::<Work>("works").unwrap().is_empty());
        let first = sync(&f.store, vec![selected]).unwrap();
        assert!(first.errors.is_empty());
        assert_eq!(first.mappings.len(), 1);
        let second = sync(&f.store, vec![f.select(&path)]).unwrap();
        assert!(second.errors.is_empty());
        assert_eq!(first.mappings[0].work_id, second.mappings[0].work_id);
        assert_eq!(
            first.mappings[0].episode_ids,
            second.mappings[0].episode_ids
        );
        assert_eq!(f.store.list::<Work>("works").unwrap().len(), 1);
        assert!(f.store.list::<String>("folders").unwrap().is_empty());
        assert_eq!(fs::read(&path).unwrap(), b"media fixture");
        assert_eq!(
            fs::read_to_string(Path::new(&path).with_extension("nfo")).unwrap(),
            MOVIE
        );
    }

    #[test]
    fn multiple_movies_in_one_directory_remain_separate_and_id_only_uses_filename() {
        let f = Fixture::new();
        let a = f.media("sample-a.mkv", MOVIE);
        let b = f.media("sample-b.mp4", "<movie><tmdbid>910000002</tmdbid></movie>");
        let result = sync(&f.store, vec![f.select(&a), f.select(&b)]).unwrap();
        assert!(result.errors.is_empty());
        assert_ne!(result.mappings[0].work_id, result.mappings[1].work_id);
        assert!(f
            .store
            .list::<Work>("works")
            .unwrap()
            .iter()
            .any(|w| w.title == "sample-b"));
    }

    #[test]
    fn episode_identity_uses_show_nfo_and_preserves_complete_multi_episode_range() {
        let f = Fixture::new();
        let path = f.media("Season 1/sample S01E01-E02.mkv", "<episodedetails><season>1</season><episode>1</episode><tmdbid>920000001</tmdbid></episodedetails><episodedetails><season>1</season><episode>2</episode><tmdbid>920000002</tmdbid></episodedetails>");
        let missing = inspect(&f.store, vec![path.clone()]);
        assert!(missing[0].error.contains("单集/音轨 ID"));
        assert!(f.store.list::<Work>("works").unwrap().is_empty());
        fs::write(
            f.input.join("tvshow.nfo"),
            "<tvshow><title>虚构剧集甲</title><tmdbid>930000001</tmdbid></tvshow>",
        )
        .unwrap();
        let result = sync(&f.store, vec![f.select(&path)]).unwrap();
        assert!(result.errors.is_empty());
        assert_eq!(result.mappings[0].episode_ids.len(), 2);
        let work = f.store.list::<Work>("works").unwrap().remove(0);
        assert_eq!(work.tmdb_id, "930000001");
        assert_eq!(work.episodes[0].remote_id, "920000001");
    }

    #[test]
    fn song_uses_album_identity_and_non_tmdb_ids_are_preserved() {
        let f = Fixture::new();
        let path = f.media(
            "01.flac",
            "<song><title>虚构音轨甲</title><track>1</track></song>",
        );
        fs::write(
            f.input.join("album.nfo"),
            "<album><title>虚构专辑甲</title><bangumiid>940000001</bangumiid></album>",
        )
        .unwrap();
        let result = sync(&f.store, vec![f.select(&path)]).unwrap();
        assert!(result.errors.is_empty());
        let work = f.store.list::<Work>("works").unwrap().remove(0);
        assert_eq!(work.kind, "music");
        assert_eq!(work.info.external_ids["bangumi"], "940000001");
    }

    #[test]
    fn changed_nfo_media_and_removed_sidecars_are_rejected_without_writes() {
        for change in 0..3 {
            let f = Fixture::new();
            let path = f.media("sample.mkv", MOVIE);
            let selected = f.select(&path);
            let sidecar = Path::new(&path).with_extension("nfo");
            match change {
                0 => fs::write(&sidecar, MOVIE.replace("910000001", "910000002")).unwrap(),
                1 => fs::write(&path, b"changed media fixture").unwrap(),
                _ => fs::remove_file(&sidecar).unwrap(),
            }
            let result = sync(&f.store, vec![selected]).unwrap();
            assert_eq!(result.errors.len(), 1);
            assert!(result.mappings.is_empty());
            assert!(f.store.list::<Work>("works").unwrap().is_empty());
        }
    }

    #[test]
    fn batch_failure_is_reported_per_file_and_successful_work_remains_idempotent() {
        let f = Fixture::new();
        let a = f.media("a.mkv", MOVIE);
        let b = f.media("b.mkv", MOVIE);
        let selections = vec![f.select(&a), f.select(&b)];
        fs::write(Path::new(&b).with_extension("nfo"), "<movie>").unwrap();
        let result = sync(&f.store, selections).unwrap();
        assert_eq!(result.mappings.len(), 1);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].path, b);
    }

    #[test]
    fn duplicate_and_conflicting_local_ids_are_never_silently_selected() {
        let f = Fixture::new();
        let path = f.media("sample.mkv", MOVIE);
        sync(&f.store, vec![f.select(&path)]).unwrap();
        let mut work = f.store.list::<Work>("works").unwrap().remove(0);
        work.info
            .external_ids
            .insert("imdb".into(), "tt910000002".into());
        f.store.put("works", &work.id, &work).unwrap();
        assert!(inspect(&f.store, vec![path.clone()])[0]
            .error
            .contains("ID"));
        work.id = id();
        f.store.put("works", &work.id, &work).unwrap();
        assert!(inspect(&f.store, vec![path])[0].error.contains("多个"));
    }

    #[test]
    fn invalid_xml_ids_and_filename_conflicts_stay_available_for_manual_matching() {
        let f = Fixture::new();
        for (index, xml) in [
            "<movie>",
            "<movie><tmdbid>-1</tmdbid></movie>",
            "<movie><actor><tmdbid>910000001</tmdbid></actor></movie>",
            "<movie><tmdbid>910000001</tmdbid><tmdbid>910000002</tmdbid></movie>",
        ]
        .into_iter()
        .enumerate()
        {
            let path = f.media(&format!("invalid-{index}.mkv"), xml);
            assert!(!inspect(&f.store, vec![path])[0].error.is_empty());
        }
        fs::write(
            f.input.join("tvshow.nfo"),
            "<tvshow><title>虚构剧集乙</title><tvdbid>930000002</tvdbid></tvshow>",
        )
        .unwrap();
        let path = f.media(
            "S01E02.mkv",
            "<episodedetails><season>1</season><episode>1</episode></episodedetails>",
        );
        assert!(inspect(&f.store, vec![path])[0]
            .error
            .contains("文件名与 NFO"));
    }

    #[test]
    fn inspection_does_not_wait_for_copy_queue_and_sync_refuses_busy_queue() {
        let f = Fixture::new();
        let path = f.media("sample.mkv", MOVIE);
        let _guard = f.store.queue_lock.lock().unwrap();
        f.store.running.store(true, Ordering::SeqCst);
        assert!(inspect(&f.store, vec![path.clone()])[0].error.is_empty());
        assert!(sync(&f.store, vec![f.select(&path)]).is_err());
    }

    #[test]
    fn legacy_and_typed_ids_are_supported_without_guessing_untyped_numeric_ids() {
        let parse_ids = |xml| nfo::parse(xml).unwrap()[0].ids();
        assert_eq!(
            parse_ids("<movie><id>tt910000003</id></movie>").unwrap()["imdb"],
            "tt910000003"
        );
        assert_eq!(
            parse_ids("<movie><id type='TMDB'>910000003</id></movie>").unwrap()["tmdb"],
            "910000003"
        );
        assert!(parse_ids("<movie><id>910000003</id></movie>")
            .unwrap()
            .is_empty());
        assert!(parse_ids("<movie><imdbid>bad-id</imdbid></movie>").is_err());
    }
}
