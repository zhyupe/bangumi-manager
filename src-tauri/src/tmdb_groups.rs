use crate::{
    db::Store,
    model::*,
    providers::{request, tmdb_info},
};
use serde_json::Value;
use std::collections::HashSet;

#[derive(serde::Serialize, serde::Deserialize)]
struct AssetMapping {
    #[serde(default)]
    part: Option<u32>,
    work_id: String,
    signature: Signature,
    episode_ids: Vec<String>,
    physical_numbers: (u32, u32, u32),
}

fn mapped_numbers(work: &Work, ids: &[String]) -> Result<(u32, u32, u32)> {
    let mut episodes = ids
        .iter()
        .map(|id| {
            work.episodes
                .iter()
                .find(|e| &e.id == id && e.enabled)
                .ok_or("文件包含新剧集组未收录或停用的集数，请先处理该文件关联")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    episodes.sort_by_key(|e| (e.season, e.number));
    let first = episodes.first().ok_or("文件没有明确的集数关联")?;
    if episodes
        .iter()
        .enumerate()
        .any(|(i, e)| e.season != first.season || e.number != first.number + i as u32)
    {
        return Err("合并文件在新剧集组中不再连续，请先拆分或重新关联".into());
    }
    Ok((first.season, first.number, episodes.last().unwrap().number))
}

pub fn save_reordered_work(store: &Store, work: &Work, binding_id: &str) -> Result<()> {
    let _scan = store.library_scan.lock().map_err(|e| e.to_string())?;
    let old = store
        .get::<Work>("works", &work.id)?
        .ok_or("原作品已不存在")?;
    let mut index = store
        .get::<LibraryIndex>("index", "main")?
        .unwrap_or_default();
    let mut documents = vec![];
    for asset in index.assets.iter_mut().filter(|a| a.work_id == work.id) {
        let mut ids = vec![];
        for n in asset.number..=asset.end {
            let matches: Vec<_> = old
                .episodes
                .iter()
                .filter(|e| e.enabled && e.season == asset.season && e.number == n)
                .collect();
            if !matches.iter().any(|e| binding_id.is_empty() || e.binding_id == binding_id) {
                continue;
            }
            if matches.len() != 1 {
                return Err("资源对应多个资料源，切换剧集组前请明确文件关联".into());
            }
            ids.push(matches[0].id.clone());
        }
        if ids.is_empty() {
            continue;
        }
        if ids.len() != (asset.end - asset.number + 1) as usize {
            return Err("合并文件的集数跨越多个资料源，无法安全切换剧集组".into());
        }
        let key = crate::paths::key(std::path::Path::new(&asset.path));
        let physical_numbers = store
            .get::<AssetMapping>("group_asset_identity", &key)?
            .map(|m| m.physical_numbers)
            .unwrap_or((asset.season, asset.number, asset.end));
        let (season, number, end) = mapped_numbers(work, &ids)?;
        asset.season = season;
        asset.number = number;
        asset.end = end;
        let mapping = AssetMapping {
            part: Some(asset.part),
            work_id: work.id.clone(),
            signature: asset.signature.clone(),
            episode_ids: ids,
            physical_numbers,
        };
        documents.push((
            "group_asset_identity",
            crate::paths::key(std::path::Path::new(&asset.path)),
            serde_json::to_string(&mapping).map_err(|e| e.to_string())?,
        ));
    }
    for resource in &mut index.resources {
        if let Some(asset) = index.assets.iter().find(|a| {
            crate::paths::key(std::path::Path::new(&a.path))
                == crate::paths::key(std::path::Path::new(&resource.path))
        }) {
            resource.asset = Some(asset.clone());
        }
    }
    documents.push((
        "works",
        work.id.clone(),
        serde_json::to_string(work).map_err(|e| e.to_string())?,
    ));
    documents.push((
        "index",
        "main".into(),
        serde_json::to_string(&index).map_err(|e| e.to_string())?,
    ));
    documents.push(("episode_order", work.id.clone(), "true".into()));
    store.put_documents(documents)
}

pub fn requires_reorganization(store: &Store, path: &std::path::Path, work: &Work) -> Result<bool> {
    let Some(mapping) =
        store.get::<AssetMapping>("group_asset_identity", &crate::paths::key(path))?
    else {
        return Ok(false);
    };
    Ok(mapping.work_id == work.id
        && mapped_numbers(work, &mapping.episode_ids)? != mapping.physical_numbers)
}

pub fn register_completed(store: &Store, row: &PlanRow) -> Result<()> {
    if row.extra || row.attachment {
        return Ok(());
    }
    let work = store
        .get::<Work>("works", &row.work_id)?
        .ok_or("作品不存在")?;
    let path = std::path::Path::new(&row.target);
    let mapping = AssetMapping {
        part: Some(row.part),
        work_id: row.work_id.clone(),
        signature: crate::paths::signature(path)?,
        episode_ids: row.episode_ids.clone(),
        physical_numbers: mapped_numbers(&work, &row.episode_ids)?,
    };
    store.put("group_asset_identity", &crate::paths::key(path), &mapping)
}
pub fn episode_ids(store: &Store, asset: &Asset, work: &Work) -> Result<Vec<String>> {
    if let Some(mapping) = store.get::<AssetMapping>("group_asset_identity",
        &crate::paths::key(std::path::Path::new(&asset.path)))? {
        if mapping.work_id != work.id || mapping.signature != asset.signature
            || mapping.part.is_some_and(|part| part != asset.part)
            || mapped_numbers(work, &mapping.episode_ids)? != (asset.season, asset.number, asset.end) {
            return Err("文件或集数关联已变化，请重新确认资源关联".into());
        }
        return Ok(mapping.episode_ids);
    }
    let mut ids = vec![];
    for n in asset.number..=asset.end {
        let matches: Vec<_> = work.episodes.iter().filter(|e|
            e.enabled && e.season == asset.season && e.number == n).collect();
        if matches.len() != 1 {
            return Err(format!("{} 的集数关联不明确，请先确认作品与集数", asset.path));
        }
        ids.push(matches[0].id.clone());
    }
    mapped_numbers(work, &ids)?;
    Ok(ids)
}

pub fn asset_numbers(
    store: &Store,
    path: &std::path::Path,
    work_id: &str,
    signature: &Signature,
) -> Result<Option<(u32, u32, u32, u32)>> {
    let Some(mapping) =
        store.get::<AssetMapping>("group_asset_identity", &crate::paths::key(path))?
    else {
        return Ok(None);
    };
    if mapping.work_id != work_id || mapping.signature != *signature {
        return Err("剧集组切换后文件发生变化，请重新确认资源关联".into());
    }
    let work = store.get::<Work>("works", work_id)?.ok_or("作品不存在")?;
    let (season, number, end) = mapped_numbers(&work, &mapping.episode_ids)?;
    Ok(Some((season, number, end, mapping.part.unwrap_or_else(|| crate::multipart::number(path)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn group() -> EpisodeGroup {
        EpisodeGroup {
            id: "abcdef0123456789abcdef01".into(),
            name: "虚构群岛纪事".into(),
            group_count: 19,
            episode_count: 114,
            group_type: 1,
            ..Default::default()
        }
    }
    fn subject() -> RemoteSubject {
        RemoteSubject {
            provider: "tmdb".into(),
            id: "910000030".into(),
            kind: "tv".into(),
            title: "虚构群岛纪事".into(),
            original_title: "虚构群岛纪事".into(),
            episodes: vec![RemoteEpisode {
                id: "910000031".into(),
                sort: 6.,
                season: 0,
                title: "虚构群岛：序章".into(),
                ..Default::default()
            }],
            seasons: vec![Season {
                number: 0,
                remote_number: 0,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    fn bind(
        store: &Store,
        subject: RemoteSubject,
        id: Option<String>,
        confirmed: bool,
    ) -> Result<Work> {
        crate::providers::bind(
            store,
            BindRequest {
                subject,
                work_id: id,
                group_change_confirmed: confirmed,
                title: "虚构群岛纪事".into(),
                kind: "tv".into(),
                season: 1,
                tmdb_id: "910000030".into(),
                parent_id: String::new(),
            },
        )
    }
    fn selected() -> RemoteSubject {
        let mut s = subject();
        s.episodes[0].season = 3;
        s.episodes[0].sort = 1.;
        s.seasons[0].number = 3;
        s.seasons[0].remote_number = 3;
        s.episode_group = Some(group());
        s
    }
    #[test]
    fn preserves_empty_groups_order_and_original_episode_identity() {
        let mut s = subject();
        let v = json!({"groups":[{"order":14,"name":"虚构群岛：终章","episodes":[{"id":42,"order":0,"season_number":0,"episode_number":35,"name":"EP"}]},{"order":13,"name":"虚构群岛：前章","episodes":[]}]});
        project(&mut s, group(), &v, &v, "zh-CN", "ja", None).unwrap();
        assert_eq!(
            s.seasons.iter().map(|s| s.number).collect::<Vec<_>>(),
            vec![13, 14]
        );
        assert_eq!((s.episodes[0].season, s.episodes[0].sort), (14, 1.));
        assert_eq!(s.episodes[0].id, "42");
        let invalid =
            json!({"groups":[{"order":1,"episodes":[{"id":42,"order":0},{"id":42,"order":1}]}]});
        assert!(project(&mut s, group(), &invalid, &invalid, "zh-CN", "ja", None).is_err());
    }
    #[test]
    fn selected_group_fetch_does_not_fetch_default_seasons_and_supports_scoped_sync() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let settings = Settings {
            language: "zh-CN".into(),
            ..Default::default()
        };
        store.put("settings", "main", &settings).unwrap();
        store.cache_put("https://api.themoviedb.org/3/tv/910000030?language=zh-CN:",&json!({"id":910000030,"name":"虚构群岛纪事","original_language":"zh-CN","seasons":[{"season_number":1}]})).unwrap();
        store
            .cache_put(
                "https://api.themoviedb.org/3/tv/910000030/episode_groups?language=zh-CN:",
                &json!({"results":[group()]}),
            )
            .unwrap();
        let v = json!({"groups":[{"order":3,"name":"虚构群岛：序章","episodes":[{"id":910000031,"order":0,"name":"EP"}]},{"order":13,"name":"虚构群岛：前章","episodes":[]}]});
        store
            .cache_put(
                &format!(
                    "https://api.themoviedb.org/3/tv/episode_group/{}?language=zh-CN:",
                    group().id
                ),
                &v,
            )
            .unwrap();
        let s = crate::providers::detail_ordered(
            &store,
            "tmdb",
            "910000030",
            "tv",
            false,
            Some(3),
            Some(&group().id),
        )
        .unwrap();
        assert_eq!(s.episodes[0].id, "910000031");
        assert_eq!(s.seasons.len(), 1);
        assert!(apply(
            &store,
            &mut subject(),
            "unrelated",
            "zh-CN",
            "ja",
            false,
            None
        )
        .is_err());
    }
    #[test]
    fn switching_requires_confirmation_keeps_ids_and_survives_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("db")).unwrap();
        let root = dir.path().join("tv");
        let folder = root.join("虚构群岛纪事");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("S00E06.mkv");
        std::fs::write(&file, "unchanged video").unwrap();
        std::fs::write(
            folder.join("tvshow.nfo"),
            "<tvshow><title>虚构群岛纪事</title><uniqueid type=\"tmdb\">910000030</uniqueid></tvshow>",
        )
        .unwrap();
        let sidecar = file.with_extension("nfo");
        let original_nfo = "<episodedetails><title>虚构群岛：序章</title><season>0</season><episode>6</episode><uniqueid type=\"tmdb\">910000031</uniqueid></episodedetails>";
        std::fs::write(&sidecar, original_nfo).unwrap();
        store
            .put(
                "settings",
                "main",
                &Settings {
                    tv_root: crate::paths::display(&root),
                    ..Default::default()
                },
            )
            .unwrap();
        let old = bind(&store, subject(), None, false).unwrap();
        store
            .put("folders", &crate::paths::key(&folder), &old.id)
            .unwrap();
        crate::scan::index_library(&store).unwrap();
        let mut incompatible = selected();
        incompatible.episodes.clear();
        assert!(bind(&store, incompatible, Some(old.id.clone()), true).is_err());
        assert_eq!(
            store
                .get::<Work>("works", &old.id)
                .unwrap()
                .unwrap()
                .episodes[0]
                .season,
            0
        );
        assert!(bind(&store, selected(), Some(old.id.clone()), false).is_err());
        let new = bind(&store, selected(), Some(old.id.clone()), true).unwrap();
        assert_eq!(old.episodes[0].id, new.episodes[0].id);
        assert_eq!(new.episodes[0].season, 3);
        let index = crate::scan::index_library(&store).unwrap();
        assert_eq!(index.assets.len(), 1);
        assert_eq!((index.assets[0].season, index.assets[0].number), (3, 1));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "unchanged video");
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), original_nfo);
        assert!(requires_reorganization(&store, &file, &new).unwrap());
        // A later scan must never silently assign a changed file to the recorded episode.
        std::fs::write(&file, "modified video with different length").unwrap();
        let index = crate::scan::index_library(&store).unwrap();
        assert!(index.assets.is_empty());
        assert!(!index.errors.is_empty());
    }
    #[test]
    fn unlisted_episodes_stay_disabled_and_group_sync_preserves_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let mut s = subject();
        s.episodes.push(RemoteEpisode {
            id: "removed".into(),
            sort: 7.,
            season: 0,
            ..Default::default()
        });
        let old = bind(&store, s, None, false).unwrap();
        let mut new = bind(&store, selected(), Some(old.id), true).unwrap();
        assert!(
            !new.episodes
                .iter()
                .find(|e| e.remote_id == "removed")
                .unwrap()
                .enabled
        );
        new.episodes[0].number = 9;
        store.put("works", &new.id, &new).unwrap();
        let synced = bind(&store, selected(), Some(new.id), false).unwrap();
        assert_eq!(synced.episodes[0].number, 9);
        assert!(synced.bindings[0].episode_group.is_some());
    }
    #[test]
    fn noncontiguous_multiepisode_files_cannot_change_order() {
        let work = Work {
            episodes: vec![
                Episode {
                    id: "a".into(),
                    season: 1,
                    number: 1,
                    enabled: true,
                    ..Default::default()
                },
                Episode {
                    id: "b".into(),
                    season: 2,
                    number: 1,
                    enabled: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(mapped_numbers(&work, &["a".into(), "b".into()]).is_err());
    }
}

pub fn list(store: &Store, id: &str, refresh: bool) -> Result<Vec<EpisodeGroup>> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("TMDB ID 必须为数字".into());
    }
    let lang = store.settings()?.language;
    let (v, _) = request(
        store,
        &format!("https://api.themoviedb.org/3/tv/{id}/episode_groups?language={lang}"),
        None,
        refresh,
    )?;
    serde_json::from_value(v["results"].clone()).map_err(|_| "TMDB 返回了无效的剧集组列表".into())
}

pub fn apply(
    store: &Store,
    subject: &mut RemoteSubject,
    group_id: &str,
    lang: &str,
    original_lang: &str,
    refresh: bool,
    only_season: Option<u32>,
) -> Result<()> {
    if group_id.is_empty() || !group_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("无效的剧集组 ID".into());
    }
    let group = list(store, &subject.id, refresh)?
        .into_iter()
        .find(|g| g.id == group_id)
        .ok_or("剧集组不属于当前 TMDB 作品")?;
    let (v, stale) = request(
        store,
        &format!("https://api.themoviedb.org/3/tv/episode_group/{group_id}?language={lang}"),
        None,
        refresh,
    )?;
    let (original, cached) = if !original_lang.is_empty() && original_lang != lang {
        request(
            store,
            &format!(
                "https://api.themoviedb.org/3/tv/episode_group/{group_id}?language={original_lang}"
            ),
            None,
            refresh,
        )?
    } else {
        (v.clone(), false)
    };
    subject.cached |= stale || cached;
    project(
        subject,
        group,
        &v,
        &original,
        lang,
        original_lang,
        only_season,
    )
}

fn project(
    subject: &mut RemoteSubject,
    group: EpisodeGroup,
    v: &Value,
    original: &Value,
    lang: &str,
    original_lang: &str,
    only_season: Option<u32>,
) -> Result<()> {
    let groups = v["groups"].as_array().ok_or("TMDB 返回了无效的剧集组")?;
    let original_episodes: std::collections::HashMap<_, _> = original["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["episodes"].as_array().into_iter().flatten())
        .filter_map(|e| e["id"].as_u64().map(|id| (id, e)))
        .collect();
    let mut seasons = vec![];
    let mut episodes = vec![];
    let mut season_numbers = HashSet::new();
    let mut episode_ids = HashSet::new();
    for g in groups {
        // Group order is the season number; episode order is zero based.
        // Preserve empty groups and numbering gaps without shifting later seasons.
        let n = g["order"]
            .as_u64()
            .filter(|n| *n <= 999)
            .ok_or("剧集组季号无效")? as u32;
        if !season_numbers.insert(n) {
            return Err("剧集组包含重复季号".into());
        }
        if only_season.is_some_and(|only| only != n) {
            continue;
        }
        let title = g["name"].as_str().unwrap_or("").to_string();
        seasons.push(Season {
            number: n,
            remote_number: n,
            title: title.clone(),
            remote_title: title.clone(),
            info: MediaInfo {
                localized_title: title,
                language: lang.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let mut numbers = HashSet::new();
        for ep in g["episodes"].as_array().ok_or("剧集组缺少集数列表")? {
            let id = ep["id"].as_u64().ok_or("剧集组集数 ID 无效")?;
            let number = ep["order"]
                .as_u64()
                .filter(|n| *n < 9999)
                .ok_or("剧集组集号无效")? as u32
                + 1;
            if !episode_ids.insert(id) || !numbers.insert(number) {
                return Err("剧集组包含重复集数，无法安全关联".into());
            }
            let original_ep = original_episodes.get(&id).copied().unwrap_or(ep);
            let mut info = tmdb_info(ep, original_ep, lang, original_lang, "name");
            info.external_ids.insert("tmdb".into(), id.to_string());
            episodes.push(RemoteEpisode {
                id: id.to_string(),
                sort: number as f64,
                season: n,
                title: ep["name"].as_str().unwrap_or("").into(),
                kind: if n == 0 { 1 } else { 0 },
                info,
            });
        }
    }
    seasons.sort_by_key(|s| s.number);
    episodes.sort_by(|a, b| a.season.cmp(&b.season).then(a.sort.total_cmp(&b.sort)));
    subject.seasons = seasons;
    subject.episodes = episodes;
    subject.episode_group = Some(group);
    Ok(())
}
