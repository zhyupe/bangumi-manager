use crate::{copy, db::Store, model::*, paths};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn node(key: &str, s: &str) -> String {
    format!("  <{key}>{}</{key}>\n", xml(s))
}
fn info_nodes(info: &MediaInfo) -> String {
    node("language", &info.language)
        + &node("originallanguage", &info.original_language)
        + &node("localizedtitle", &info.localized_title)
        + &node("originalplot", &info.original_overview)
        + &node("aired", &info.air_date)
}
pub fn cache_image(store: &Store, raw: &str) -> Result<String> {
    store.diagnostics.measure_with(
        "artwork.cache",
        crate::diagnostics::endpoint(raw),
        |timing| {
            if raw.is_empty() {
                return Ok(String::new());
            }
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let url = url::Url::parse(raw).map_err(|e| e.to_string())?;
            if url.scheme() != "https" {
                return Err("图片地址必须为 HTTPS".into());
            }
            let dir = store.data_dir.join("artwork");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let file = dir.join(format!(
                "{}.jpg",
                hex::encode(Sha256::digest(raw.as_bytes()))
            ));
            timing.field("cache", if file.exists() { "fresh_hit" } else { "miss" });
            if !file.exists() {
                let response = store.diagnostics.measure(
                    "artwork.headers",
                    crate::diagnostics::endpoint(raw),
                    || {
                        store
                            .http_client()?
                            .get(url)
                            .timeout(Duration::from_secs(12))
                            .send()
                            .and_then(|r| r.error_for_status())
                            .map_err(|e| e.without_url().to_string())
                    },
                )?;
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if !content_type.starts_with("image/") {
                    return Err("远端返回的内容不是图片".into());
                }
                if response.content_length().unwrap_or(0) > 10 * 1024 * 1024 {
                    return Err("图片超过 10 MB".into());
                }
                let mut bytes = vec![];
                store
                    .diagnostics
                    .measure("artwork.body", serde_json::json!({}), || {
                        response
                            .take(10 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())
                    })?;
                if bytes.len() > 10 * 1024 * 1024 {
                    return Err("图片超过 10 MB".into());
                }
                timing.field("bytes", bytes.len());
                store.diagnostics.measure(
                    "artwork.write",
                    serde_json::json!({"bytes":bytes.len()}),
                    || atomic_write(&file, &bytes),
                )?;
            }
            Ok(paths::display(&file))
        },
    )
}
pub fn cache_work(store: &Store, w: &mut Work) {
    let mut timing = store.diagnostics.span(
        "artwork.work",
        serde_json::json!({"work_id":w.id,"seasons":w.seasons.len()}),
    );
    let _scope = timing.enter();
    w.metadata_errors.clear();
    if w.info.poster.is_empty() {
        w.info.poster = w.poster.clone();
    }
    for info in std::iter::once(&mut w.info).chain(w.seasons.iter_mut().map(|s| &mut s.info)) {
        match cache_image(store, &info.poster) {
            Ok(p) => info.local_poster = p,
            Err(e) => w.metadata_errors.push(e),
        }
        match cache_image(store, &info.backdrop) {
            Ok(p) => info.local_backdrop = p,
            Err(e) => w.metadata_errors.push(e),
        }
    }
    timing.field("errors", w.metadata_errors.len());
    timing.finish(if w.metadata_errors.is_empty() {
        "ok"
    } else {
        "error"
    });
}
fn write_art(
    store: &Store,
    url: &str,
    local: &str,
    target: &Path,
    notices: &mut Vec<String>,
) -> Result<()> {
    if target.try_exists().map_err(|e| e.to_string())? {
        notices.push(format!("已有图片，已跳过：{}", paths::display(target)));
        return Ok(());
    }
    if url.is_empty() && local.is_empty() {
        return Ok(());
    }
    let file = if !local.is_empty()
        && paths::contained(Path::new(local), &store.data_dir.join("artwork"))
        && Path::new(local).is_file()
    {
        local.into()
    } else {
        cache_image(store, url)?
    };
    if file.is_empty() {
        return Ok(());
    }
    managed_write(store, target, &fs::read(file).map_err(|e| e.to_string())?)
}
// Accumulate complete season information across episodes before touching the share.
#[derive(Default)]
pub struct SeasonMetadataBatch {
    entries: BTreeMap<String, PendingSeason>,
}
struct PendingSeason {
    path: PathBuf,
    root: PathBuf,
    poster: PathBuf,
    work_id: String,
    number: u32,
    custom_title: bool,
    fields: BTreeMap<&'static str, String>,
    ids: BTreeMap<String, String>,
    image_url: String,
    local_image: String,
}
impl SeasonMetadataBatch {
    fn collect(
        &mut self,
        root: &Path,
        folder: &Path,
        target: &Path,
        w: &Work,
        ep: &Episode,
    ) -> Result<()> {
        let fallback = Season {
            number: ep.season,
            title: format!("第 {} 季", ep.season),
            ..Default::default()
        };
        let season = w
            .seasons
            .iter()
            .find(|s| s.number == ep.season && s.binding_id == ep.binding_id)
            .or_else(|| w.seasons.iter().find(|s| s.number == ep.season))
            .unwrap_or(&fallback);
        let path = target.parent().ok_or("无效目标目录")?.join("season.nfo");
        let pending = self
            .entries
            .entry(paths::key(&path))
            .or_insert_with(|| PendingSeason {
                path,
                root: root.to_path_buf(),
                poster: folder.join(format!("season{:02}-poster.jpg", season.number)),
                work_id: w.id.clone(),
                number: season.number,
                custom_title: false,
                fields: BTreeMap::new(),
                ids: BTreeMap::new(),
                image_url: String::new(),
                local_image: String::new(),
            });
        if pending.work_id != w.id || pending.number != season.number {
            return Err(format!(
                "不同作品或季度对应同一个 season.nfo：{}",
                paths::display(&pending.path)
            ));
        }
        // Nonempty values supplement earlier episodes; a manually named season wins.
        if !season.title.is_empty() && (!pending.custom_title || season.custom_title) {
            pending.fields.insert("title", season.title.clone());
        }
        pending.custom_title |= season.custom_title;
        for (tag, value) in [
            ("originaltitle", &season.info.original_title),
            ("plot", &season.info.overview),
            ("premiered", &season.info.air_date),
            ("language", &season.info.language),
            ("originallanguage", &season.info.original_language),
            ("localizedtitle", &season.info.localized_title),
            ("originalplot", &season.info.original_overview),
            ("aired", &season.info.air_date),
        ] {
            if !value.is_empty() {
                pending.fields.insert(tag, value.clone());
            }
        }
        pending.ids.extend(
            season
                .info
                .external_ids
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some(binding) = w
            .bindings
            .iter()
            .find(|b| b.id == ep.binding_id && b.provider == "bangumi")
        {
            pending
                .ids
                .entry("bangumi".into())
                .or_insert(binding.remote_id.clone());
        }
        if !season.info.poster.is_empty() || !season.info.local_poster.is_empty() {
            pending.image_url = season.info.poster.clone();
            pending.local_image = season.info.local_poster.clone();
        }
        Ok(())
    }
    pub fn flush(self, store: &Store, notices: &mut Vec<String>) -> Vec<String> {
        let mut errors = vec![];
        for pending in self.entries.into_values() {
            let mut content =
                String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<season>\n");
            for (tag, value) in &pending.fields {
                content += &node(tag, value);
            }
            content += &node("seasonnumber", &pending.number.to_string());
            content += &unique_ids(&pending.ids);
            content += "</season>\n";
            if let Err(error) = paths::safe_target_entry(&pending.path, &pending.root)
                .and_then(|_| managed_write(store, &pending.path, content.as_bytes()))
            {
                errors.push(format!("{}：{error}", paths::display(&pending.path)));
            }
            if let Err(error) =
                paths::safe_target_entry(&pending.poster, &pending.root).and_then(|_| {
                    write_art(
                        store,
                        &pending.image_url,
                        &pending.local_image,
                        &pending.poster,
                        notices,
                    )
                })
            {
                errors.push(format!("{}：{error}", paths::display(&pending.poster)));
            }
        }
        errors
    }
}
pub fn refresh_existing(store: &Store, work: &Work) -> Result<()> {
    store.diagnostics.measure(
        "metadata.refresh",
        serde_json::json!({"work_id":work.id}),
        || {
            let mut seen = std::collections::HashSet::new();
            let mut errors = vec![];
            let mut notices = vec![];
            let mut seasons = SeasonMetadataBatch::default();
            for p in store.list::<Plan>("plans")? {
                for row in p.rows.iter().filter(|r| {
                    r.work_id == work.id && !r.extra && !r.attachment && r.state == "complete"
                }) {
                    if crate::tmdb_groups::requires_reorganization(
                        store,
                        Path::new(&row.target),
                        work,
                    )? {
                        notices.push(format!(
                            "剧集组编号与原路径不同，保留原 NFO，请重新整理：{}",
                            row.target
                        ));
                        continue;
                    }
                    if let Some(previous) = p.works.iter().find(|w| w.id == work.id) {
                        if row.episode_ids.iter().any(|id| {
                            let old = previous.episodes.iter().find(|e| &e.id == id);
                            let new = work.episodes.iter().find(|e| &e.id == id);
                            match (old, new) {
                                (Some(a), Some(b)) => {
                                    (a.season, a.number) != (b.season, b.number) || !b.enabled
                                }
                                _ => true,
                            }
                        }) {
                            notices.push(format!(
                                "季集编号已更改，保留旧路径的 NFO，重新整理文件后再更新：{}",
                                row.target
                            ));
                            continue;
                        }
                    }
                    if Path::new(&row.target).is_file()
                        && seen.insert(paths::key(Path::new(&row.target)))
                    {
                        if let Err(e) =
                            write_metadata(store, &p, work, row, true, &mut notices, &mut seasons)
                        {
                            errors.push(e);
                        }
                    }
                }
            }
            errors.extend(seasons.flush(store, &mut notices));
            let mut saved = work.clone();
            saved.metadata_errors.extend(errors);
            saved.metadata_errors.sort();
            saved.metadata_errors.dedup();
            notices.sort();
            notices.dedup();
            saved.metadata_notices = notices;
            store.put("works", &saved.id, &saved)
        },
    )
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_file_name(format!(".bm-meta-{}.tmp", id()));
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    fs::rename(&temp, path).map_err(|e| e.to_string())
}
fn managed_write(store: &Store, path: &Path, bytes: &[u8]) -> Result<()> {
    store.diagnostics.measure(
        "metadata.write_file",
        serde_json::json!({"bytes":bytes.len()}),
        || {
            let is_nfo = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nfo"));
            if !is_nfo
                && path.exists()
                && !store
                    .get::<bool>("metadata_owned", &paths::key(path))?
                    .unwrap_or(false)
            {
                return Err(format!(
                    "已有元数据未由本应用管理，已保留：{}；移开该文件后可重试",
                    paths::display(path)
                ));
            }
            atomic_write(path, bytes)?;
            store.put("metadata_owned", &paths::key(path), &true)
        },
    )
}
fn common(w: &Work) -> String {
    let mut s = node("title", &w.title)
        + &node(
            "originaltitle",
            if w.info.original_title.is_empty() {
                &w.title
            } else {
                &w.info.original_title
            },
        )
        + &node("year", &w.year)
        + &node("plot", &w.overview)
        + &info_nodes(&w.info);
    s += &node("premiered", &w.info.air_date);
    let mut ids = w.info.external_ids.clone();
    if !w.tmdb_id.is_empty() {
        ids.insert("tmdb".into(), w.tmdb_id.clone());
    }
    if let Some(binding) = w
        .bindings
        .iter()
        .filter(|b| b.provider == "bangumi")
        .min_by_key(|b| if b.season == 0 { u32::MAX } else { b.season })
    {
        ids.entry("bangumi".into())
            .or_insert(binding.remote_id.clone());
    }
    s += &unique_ids(&ids);
    if w.kind == "music" {
        if let Some(id) = ids.get("musicbrainz") { s += &node("musicbrainzalbumid", id); }
        if let Some(id) = ids.get("musicbrainz_releasegroup") { s += &node("musicbrainzreleasegroupid", id); }
    }
    s
}
fn unique_ids(ids: &std::collections::BTreeMap<String, String>) -> String {
    let preferred = if ids.contains_key("tmdb") {
        "tmdb"
    } else if ids.contains_key("bangumi") {
        "bangumi"
    } else {
        ids.keys().next().map(String::as_str).unwrap_or("")
    };
    let mut text = String::new();
    for (provider, id) in ids.iter().filter(|(_, id)| !id.is_empty()) {
        text += &format!(
            "  <uniqueid type=\"{}\" default=\"{}\">{}</uniqueid>\n",
            xml(provider),
            provider == preferred,
            xml(id)
        );
        if ["tmdb", "tvdb", "imdb", "bangumi"].contains(&provider.as_str()) {
            text += &node(&format!("{provider}id"), id);
        }
    }
    text
}
pub fn tvshow_nfo(w: &Work) -> String {
    let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tvshow>\n");
    text += &common(w);
    let mut seasons = std::collections::BTreeMap::new();
    for b in &w.bindings {
        if b.provider == "bangumi" {
            seasons.entry(b.season).or_insert(&b.source_title);
        }
    }
    for season in &w.seasons {
        seasons.insert(season.number, &season.title);
    }
    for (n, title) in seasons {
        text += &format!(
            "  <namedseason number=\"{n}\">{}</namedseason>\n",
            xml(title)
        );
    }
    text + "</tvshow>\n"
}
pub fn write_metadata(
    store: &Store,
    p: &Plan,
    w: &Work,
    row: &PlanRow,
    shared: bool,
    notices: &mut Vec<String>,
    seasons: &mut SeasonMetadataBatch,
) -> Result<()> {
    store.diagnostics.measure(
        "metadata.write",
        serde_json::json!({"work_id":w.id,"row_id":row.id}),
        || {
            let target = Path::new(&row.target);
            let root = paths::normalize(if let Some(rename) = &row.rename {
                &rename.root
            } else if let Some(replacement) = &row.replacement {
                &replacement.root
            } else if w.kind == "movie" {
                &p.movie_root
            } else if w.kind == "music" {
                &p.music_root
            } else {
                &p.tv_root
            })?;
            let folder = if row.media_folder.is_empty() {
                root.join(&w.folder_name)
            } else {
                paths::normalize(&row.media_folder)?
            };
            paths::safe_target(&folder, &root)?;
            paths::safe_target_entry(target, &root)?;
            let sidecar = folder.join("bangumi-manager.json");
            store.put("folders", &paths::key(&folder), &w.id)?;
            let eps = copy::episode_range(w, &row.episode_ids)?;
            store.put(
                "asset_identity",
                &paths::key(target),
                &(eps[0].season, eps[0].number, eps.last().unwrap().number),
            )?;
            let mut errors = vec![];
            let nfo = if w.kind == "movie" {
                target.with_extension("nfo")
            } else {
                folder.join(if w.kind == "music" {
                    "album.nfo"
                } else {
                    "tvshow.nfo"
                })
            };
            if shared || w.kind == "movie" {
                let content = if w.kind == "music" {
                    format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<album>\n{}{}</album>\n",
                        common(w),
                        node("artist", &w.info.artist)
                    )
                } else if w.kind == "movie" {
                    let mut body = common(w);
                    if !w.parent_id.is_empty() {
                        if let Some(parent) = store.get::<Work>("works", &w.parent_id)? {
                            body += &format!("  <set><name>{}</name></set>\n", xml(&parent.title));
                        }
                    }
                    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<movie>\n{body}</movie>\n")
                } else {
                    tvshow_nfo(w)
                };
                if let Err(e) = managed_write(store, &nfo, content.as_bytes()) {
                    errors.push(e);
                }
            }
            if w.kind != "movie" {
                let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
                for ep in copy::episode_range(w, &row.episode_ids)? {
                    let tag = if w.kind == "music" {
                        "song"
                    } else {
                        "episodedetails"
                    };
                    text += &format!("<{tag}>\n");
                    text += &node("title", &ep.title);
                    text += &node("showtitle", &w.title);
                    text += &node("season", &ep.season.to_string());
                    text += &node("episode", &ep.number.to_string());
                    text += &node("originaltitle", &ep.info.original_title);
                    text += &node("plot", &ep.info.overview);
                    text += &info_nodes(&ep.info);
                    if w.kind == "music" {
                        text += &node("track", &ep.number.to_string());
                        text += &node("album", &w.title);
                        text += &node("artist", if ep.info.artist.is_empty() { &w.info.artist } else { &ep.info.artist });
                    }
                    let mut ids = ep.info.external_ids.clone();
                    if let Some(binding) = w.bindings.iter().find(|b| b.id == ep.binding_id) {
                        if binding.provider != "nfo"
                            && !ep.remote_id.starts_with("nfo:")
                            && !ep.remote_id.is_empty()
                        {
                            ids.entry(binding.provider.clone())
                                .or_insert(ep.remote_id.clone());
                        }
                    }
                    text += &unique_ids(&ids);
                    if w.kind == "music" {
                        if let Some(id) = ids.get("musicbrainz") { text += &node("musicbrainztrackid", id); }
                        if let Some(id) = ids.get("musicbrainz_track") { text += &node("musicbrainzreleasetrackid", id); }
                    }
                    text += &format!("</{tag}>\n");
                    if let Err(e) = write_art(
                        store,
                        &ep.info.poster,
                        &ep.info.local_poster,
                        &target.with_file_name(format!(
                            "{}-thumb.jpg",
                            target.file_stem().unwrap_or_default().to_string_lossy()
                        )),
                        notices,
                    ) {
                        errors.push(e);
                    }
                    if w.kind == "tv" {
                        if let Err(e) = seasons.collect(&root, &folder, target, w, ep) {
                            errors.push(e);
                        }
                    }
                }
                if let Err(e) = managed_write(store, &target.with_extension("nfo"), text.as_bytes())
                {
                    errors.push(e);
                }
                if shared && w.kind == "tv" {
                    let mut plexmatch = format!("Title: {}\n", w.title.replace(['\n', '\r'], " "));
                    if !w.year.is_empty() {
                        plexmatch += &format!("Year: {}\n", w.year);
                    }
                    if !w.tmdb_id.is_empty() {
                        plexmatch += &format!("TMDBID: {}\n", w.tmdb_id);
                    }
                    if let Err(e) =
                        managed_write(store, &folder.join(".plexmatch"), plexmatch.as_bytes())
                    {
                        errors.push(e);
                    }
                }
            }
            if shared {
                if let Err(e) = atomic_write(
                    &sidecar,
                    &serde_json::to_vec_pretty(w).map_err(|e| e.to_string())?,
                ) {
                    errors.push(e);
                }
                let poster = folder.join("poster.jpg");
                if let Err(e) = write_art(
                    store,
                    &w.info.backdrop,
                    &w.info.local_backdrop,
                    &folder.join("fanart.jpg"),
                    notices,
                ) {
                    errors.push(e);
                }
                if let Err(e) = write_art(store, &w.poster, &w.info.local_poster, &poster, notices)
                {
                    errors.push(format!("海报下载：{e}"));
                }
            }
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("；"))
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seasons_merge_before_writing_and_remain_separate_by_directory() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().join("state")).unwrap();
        let root = d.path().join("tv");
        let folder = root.join("show");
        let dir = folder.join("Season 02");
        fs::create_dir_all(&dir).unwrap();
        let nfo = dir.join("season.nfo");
        fs::write(&nfo, b"existing metadata").unwrap();
        fs::write(folder.join("season02-poster.jpg"), b"existing art").unwrap();
        let w = Work {
            id: "show".into(),
            seasons: vec![
                Season {
                    number: 2,
                    binding_id: "a".into(),
                    title: "手动季名".into(),
                    custom_title: true,
                    info: MediaInfo {
                        original_title: "原題".into(),
                        overview: "简介".into(),
                        external_ids: [("bangumi".into(), "100".into())].into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Season {
                    number: 2,
                    binding_id: "b".into(),
                    title: "远端季名".into(),
                    info: MediaInfo {
                        language: "zh-CN".into(),
                        original_language: "ja".into(),
                        localized_title: "中文季名".into(),
                        external_ids: [("tmdb".into(), "200".into())].into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let ep = Episode {
            season: 2,
            binding_id: "a".into(),
            ..Default::default()
        };
        let mut batch = SeasonMetadataBatch::default();
        batch
            .collect(&root, &folder, &dir.join("S02E01.mkv"), &w, &ep)
            .unwrap();
        batch
            .collect(
                &root,
                &folder,
                &dir.join("S02E02.mkv"),
                &w,
                &Episode {
                    binding_id: "b".into(),
                    ..ep.clone()
                },
            )
            .unwrap();
        batch
            .collect(&root, &folder, &dir.join("S02E03-E04.mkv"), &w, &ep)
            .unwrap();
        assert_eq!(batch.entries.len(), 1);
        assert_eq!(fs::read(&nfo).unwrap(), b"existing metadata");
        for (subfolder, number, work_id) in [("show", 3, "show"), ("other", 2, "other")] {
            let folder = root.join(subfolder);
            let target = folder.join(format!("Season {number:02}/episode.mkv"));
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let other = Work {
                id: work_id.into(),
                ..Default::default()
            };
            batch
                .collect(
                    &root,
                    &folder,
                    &target,
                    &other,
                    &Episode {
                        season: number,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(batch.entries.len(), 3);
        let mut notices = vec![];
        assert!(batch.flush(&store, &mut notices).is_empty());
        let nfo = crate::nfo::one(&nfo, "season").unwrap();
        assert_eq!(nfo.value("title"), "手动季名");
        assert_eq!(nfo.value("originaltitle"), "原題");
        assert_eq!(nfo.value("plot"), "简介");
        assert_eq!(nfo.value("language"), "zh-CN");
        assert_eq!(nfo.value("localizedtitle"), "中文季名");
        assert_eq!(nfo.value("bangumiid"), "100");
        assert_eq!(nfo.value("tmdbid"), "200");
        assert_eq!(
            notices.len(),
            1,
            "One seasonal artwork check for the whole batch"
        );
        assert!(folder.join("Season 03/season.nfo").is_file());
        assert!(root.join("other/Season 02/season.nfo").is_file());
    }
    #[test]
    fn failed_season_write_does_not_block_other_seasons_and_can_retry() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().join("state")).unwrap();
        let root = d.path().join("tv");
        let folder = root.join("show");
        let blocked = folder.join("Season 01/season.nfo");
        fs::create_dir_all(&blocked).unwrap();
        fs::create_dir_all(folder.join("Season 02")).unwrap();
        let w = Work {
            id: "show".into(),
            ..Default::default()
        };
        let collect = |batch: &mut SeasonMetadataBatch, number| {
            batch
                .collect(
                    &root,
                    &folder,
                    &folder.join(format!("Season {number:02}/episode.mkv")),
                    &w,
                    &Episode {
                        season: number,
                        ..Default::default()
                    },
                )
                .unwrap();
        };
        let mut batch = SeasonMetadataBatch::default();
        collect(&mut batch, 1);
        collect(&mut batch, 2);
        let errors = batch.flush(&store, &mut vec![]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("season.nfo"));
        assert!(folder.join("Season 02/season.nfo").is_file());
        fs::remove_dir(&blocked).unwrap();
        let mut retry = SeasonMetadataBatch::default();
        collect(&mut retry, 1);
        assert!(retry.flush(&store, &mut vec![]).is_empty());
        assert!(blocked.is_file());
    }
    #[test]
    fn escapes_metadata() {
        assert_eq!(xml("A&B <3 \""), "A&amp;B &lt;3 &quot;");
    }
}
