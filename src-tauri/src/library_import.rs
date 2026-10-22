use crate::{
    db::Store,
    model::*,
    nfo::{self, Node},
    paths,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

pub struct ImportedFolder {
    pub work_id: String,
    pub imported: bool,
    pub updated: bool,
    pub numbers: HashMap<String, (u32, u32, u32)>,
}
fn file_named<'a>(files: &'a [PathBuf], dir: &Path, name: &str) -> Option<&'a PathBuf> {
    files.iter().find(|p| {
        p.parent() == Some(dir)
            && p.file_name()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(name))
    })
}
fn merge_info(target: &mut MediaInfo, source: &MediaInfo) -> Result<()> {
    for (provider, id) in &source.external_ids {
        if target
            .external_ids
            .get(provider)
            .is_some_and(|old| old != id)
        {
            return Err(format!("同一条目的 {provider} ID 冲突"));
        }
        target.external_ids.insert(provider.clone(), id.clone());
    }
    macro_rules! fill { ($($field:ident),*) => { $(if target.$field.is_empty() { target.$field = source.$field.clone(); })* }; }
    fill!(
        localized_title,
        original_title,
        language,
        original_language,
        overview,
        original_overview,
        poster,
        local_poster,
        backdrop,
        local_backdrop,
        air_date,
        artist,
        genres,
        countries
    );
    Ok(())
}
fn ids(w: &Work) -> BTreeMap<String, String> {
    let mut ids = w.info.external_ids.clone();
    // A release group joins distinct editions; it cannot identify one local album.
    ids.retain(|provider, _| !provider.starts_with("musicbrainz_"));
    if !w.tmdb_id.is_empty() {
        ids.insert("tmdb".into(), w.tmdb_id.clone());
    }
    for b in &w.bindings {
        if ["tmdb", "bangumi", "musicbrainz"].contains(&b.provider.as_str()) {
            ids.entry(b.provider.clone()).or_insert(b.remote_id.clone());
        }
    }
    ids
}
pub(crate) fn same_id(w: &Work, provider: &str, id: &str) -> bool {
    ids(w).get(provider).is_some_and(|v| v == id)
        || w.bindings
            .iter()
            .any(|b| b.provider == provider && b.remote_id == id)
}
fn check_identity(a: &Work, b: &Work) -> Result<()> {
    if a.kind != b.kind {
        return Err("NFO 类型与已关联作品不一致".into());
    }
    for (provider, value) in ids(b) {
        if ids(a).contains_key(&provider) && !same_id(a, &provider, &value) {
            return Err(format!("NFO 的 {provider} ID 与已关联作品不一致"));
        }
    }
    Ok(())
}
pub(crate) fn merge_work(mut target: Work, source: &Work) -> Result<Work> {
    check_identity(&target, source)?;
    merge_info(&mut target.info, &source.info)?;
    macro_rules! fill { ($($field:ident),*) => { $(if target.$field.is_empty() { target.$field = source.$field.clone(); })* }; }
    fill!(title, year, overview, poster, category, tmdb_id);
    let mut bindings = HashMap::new();
    for binding in &source.bindings {
        let existing = target.bindings.iter().find(|b| {
            b.provider == binding.provider
                && b.remote_id == binding.remote_id
                && b.source_kind == binding.source_kind
        });
        let id = if let Some(existing) = existing {
            existing.id.clone()
        } else {
            target.bindings.push(binding.clone());
            binding.id.clone()
        };
        bindings.insert(binding.id.clone(), id);
    }
    for season in &source.seasons {
        if let Some(old) = target.seasons.iter_mut().find(|s| {
            (s.binding_id
                == *bindings
                    .get(&season.binding_id)
                    .unwrap_or(&season.binding_id)
                && s.remote_number == season.remote_number)
                || s.number == season.number
        }) {
            merge_info(&mut old.info, &season.info)?;
        } else {
            let mut season = season.clone();
            season.binding_id = bindings
                .get(&season.binding_id)
                .cloned()
                .unwrap_or(season.binding_id);
            target.seasons.push(season);
        }
    }
    for ep in &source.episodes {
        let mapped_binding = bindings.get(&ep.binding_id).unwrap_or(&ep.binding_id);
        let mapped_season = target
            .seasons
            .iter()
            .find(|s| s.binding_id == *mapped_binding && s.remote_number == ep.season)
            .map(|s| s.number)
            .unwrap_or(ep.season);
        if let Some(old) = target.episodes.iter_mut().find(|e| {
            (!ep.remote_id.is_empty()
                && e.remote_id == ep.remote_id
                && e.binding_id == *mapped_binding)
                || (ep.remote_id.starts_with("nfo:")
                    && e.binding_id == *mapped_binding
                    && e.sort == ep.sort
                    && e.season == mapped_season)
                || (e.season == ep.season && e.number == ep.number)
        }) {
            merge_info(&mut old.info, &ep.info)?;
            if old.title.is_empty() || old.remote_id.starts_with("nfo:") {
                old.title = ep.title.clone();
            }
            if old.remote_id.starts_with("nfo:") && !ep.remote_id.starts_with("nfo:") {
                old.remote_id = ep.remote_id.clone();
                old.binding_id = bindings
                    .get(&ep.binding_id)
                    .cloned()
                    .unwrap_or_else(|| ep.binding_id.clone());
            }
        } else {
            let mut ep = ep.clone();
            ep.binding_id = bindings
                .get(&ep.binding_id)
                .cloned()
                .unwrap_or(ep.binding_id);
            target.episodes.push(ep);
        }
    }
    Ok(target)
}
fn add_binding(w: &mut Work, provider: &str, remote: &str, season: u32, title: &str) -> String {
    if let Some(b) = w
        .bindings
        .iter()
        .find(|b| b.provider == provider && b.remote_id == remote)
    {
        return b.id.clone();
    }
    let id = id();
    w.bindings.push(Binding {
        episode_group: None,
        id: id.clone(),
        provider: provider.into(),
        remote_id: remote.into(),
        source_kind: w.kind.clone(),
        source_title: title.into(),
        season,
    });
    id
}
fn local_art(store: &Store, source: &Path, dir: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let resolved = paths::resolved(source)?;
    if !paths::contained(&resolved, &paths::resolved(dir)?) {
        return Err("图片路径超出作品目录".into());
    }
    let signature = paths::signature(&resolved)?;
    if signature.size > 10 * 1024 * 1024 {
        return Err("图片超过 10 MB".into());
    }
    let key = format!(
        "{}:{}:{}",
        paths::key(source),
        signature.size,
        signature.modified
    );
    let target = store.data_dir.join("artwork").join(format!(
        "{}.{}",
        hex::encode(Sha256::digest(key.as_bytes())),
        paths::ext(source)
    ));
    if !target.is_file() {
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(&resolved, &target).map_err(|e| e.to_string())?;
    }
    Ok(paths::display(&target))
}
fn artwork(store: &Store, info: &mut MediaInfo, files: &[PathBuf], dir: &Path, owner: &Path) {
    for (names, target) in [
        (
            ["poster.jpg", "poster.png", "folder.jpg"],
            &mut info.local_poster,
        ),
        (
            ["fanart.jpg", "fanart.png", "backdrop.jpg"],
            &mut info.local_backdrop,
        ),
    ] {
        if target.is_empty() {
            if let Some(path) = names.iter().find_map(|name| file_named(files, dir, name)) {
                if let Ok(local) = local_art(store, path, owner) {
                    *target = local;
                }
            }
        }
    }
}
fn title_key(s: &str) -> String {
    s.split_whitespace().collect::<String>().to_lowercase()
}
fn filename_numbers(path: &Path) -> Option<(u32, u32, u32)> {
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)S(\d{1,3})E(\d{1,4})(?:-E?(\d{1,4}))?").unwrap()
    });
    let re = &*PATTERN;
    let name = path.file_name()?.to_string_lossy();
    let c = re.captures(&name)?;
    let n = c[2].parse().ok()?;
    Some((
        c[1].parse().ok()?,
        n,
        c.get(3).and_then(|v| v.as_str().parse().ok()).unwrap_or(n),
    ))
}
pub(crate) fn load(
    store: &Store,
    dir: &Path,
    kind: &str,
    category: &str,
    files: &[PathBuf],
    language: &str,
    fallback_title: Option<&str>,
) -> Result<Option<(Work, HashMap<String, (u32, u32, u32)>)>> {
    let roots: Vec<_> = if kind == "movie" {
        files
            .iter()
            .filter(|p| p.parent() == Some(dir) && paths::ext(p) == "nfo")
            .collect()
    } else {
        file_named(
            files,
            dir,
            if kind == "tv" {
                "tvshow.nfo"
            } else {
                "album.nfo"
            },
        )
        .into_iter()
        .collect()
    };
    if roots.is_empty() {
        return Ok(None);
    }
    let tag = if kind == "music" {
        "album"
    } else if kind == "tv" {
        "tvshow"
    } else {
        "movie"
    };
    let mut documents = Vec::new();
    for path in roots {
        documents.push(nfo::one(path, tag)?);
    }
    let main = &documents[0];
    let title = match main.value("title") {
        title if title.is_empty() => fallback_title.unwrap_or_default().to_string(),
        title => title,
    };
    if title.is_empty() {
        return Err("NFO 缺少作品名称".into());
    }
    let info = main.info(language)?;
    let mut w = Work {
        id: id(),
        kind: kind.into(),
        title: title.clone(),
        year: main.value("year"),
        folder_name: dir.file_name().unwrap_or_default().to_string_lossy().into(),
        category: category.into(),
        tmdb_id: info.external_ids.get("tmdb").cloned().unwrap_or_default(),
        overview: info.overview.clone(),
        poster: info.poster.clone(),
        info,
        ..Default::default()
    };
    if w.year.is_empty() {
        w.year = w.info.air_date.get(..4).unwrap_or("").into();
    }
    for doc in documents.iter().skip(1) {
        let info = doc.info(language)?;
        if w.info.external_ids.is_empty()
            && info.external_ids.is_empty()
            && title_key(&doc.value("title")) != title_key(&w.title)
        {
            return Err("同目录中的电影 NFO 名称不同，无法确认是否属于同一作品".into());
        }
        merge_info(&mut w.info, &info)?;
    }
    let provider = if w.info.external_ids.contains_key("tmdb") {
        "tmdb"
    } else if w.info.external_ids.contains_key("bangumi") {
        "bangumi"
    } else if kind == "music" && w.info.external_ids.contains_key("musicbrainz") {
        "musicbrainz"
    } else {
        "nfo"
    };
    let remote = w
        .info
        .external_ids
        .get(provider)
        .cloned()
        .unwrap_or_else(|| paths::key(dir));
    let main_binding = add_binding(&mut w, provider, &remote, 1, &title);
    artwork(store, &mut w.info, files, dir, dir);
    let mut season_dirs = HashMap::new();
    if kind != "movie" {
        for path in files.iter().filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("season.nfo"))
        }) {
            let doc = nfo::one(path, "season")?;
            let number = doc
                .number("seasonnumber")
                .or_else(|| doc.number("season"))
                .ok_or("season.nfo 缺少有效季号")?;
            if number > 999 {
                return Err("季度 NFO 季号超出有效范围".into());
            }
            let mut info = doc.info(language)?;
            let title = doc.value("title");
            let binding_id = if provider != "tmdb" {
                if let Some(id) = info.external_ids.get("bangumi") {
                    add_binding(&mut w, "bangumi", id, number, &title)
                } else {
                    main_binding.clone()
                }
            } else {
                main_binding.clone()
            };
            let parent = path.parent().unwrap();
            artwork(store, &mut info, files, parent, dir);
            season_dirs.insert(paths::key(parent), number);
            if let Some(old) = w.seasons.iter_mut().find(|s| s.number == number) {
                merge_info(&mut old.info, &info)?;
            } else {
                w.seasons.push(Season {
                    binding_id,
                    number,
                    remote_number: if provider == "bangumi" { 1 } else { number },
                    title: if title.is_empty() {
                        format!("第 {number} 季")
                    } else {
                        title.clone()
                    },
                    remote_title: title,
                    custom_title: true,
                    info,
                });
            }
        }
    }
    let mut numbers = HashMap::new();
    if kind == "movie" {
        w.episodes.push(Episode {
            id: id(),
            binding_id: main_binding.clone(),
            remote_id: remote.clone(),
            title: w.title.clone(),
            season: 1,
            number: 1,
            sort: 1.,
            enabled: true,
            info: w.info.clone(),
            ..Default::default()
        });
    }
    let mut nfos_by_path = HashMap::new();
    for path in files.iter().filter(|p| paths::ext(p) == "nfo") {
        nfos_by_path.entry(paths::key(path)).or_insert(path);
    }
    for media in files.iter().filter(|p| {
        if kind == "music" {
            paths::audio(p)
        } else {
            paths::video(p)
        }
    }) {
        if kind == "movie" {
            numbers.insert(paths::key(media), (1, 1, 1));
            continue;
        }
        let expected = media.with_extension("nfo");
        let document = nfos_by_path.get(&paths::key(&expected));
        let mut eps = Vec::new();
        if let Some(nfo) = document {
            for doc in nfo::read(nfo)? {
                if doc.name
                    != if kind == "music" {
                        "song"
                    } else {
                        "episodedetails"
                    }
                {
                    return Err(format!("{} 的单集 NFO 类型不正确", media.display()));
                }
                let season = doc
                    .number("season")
                    .or_else(|| {
                        season_dirs
                            .get(&paths::key(media.parent().unwrap()))
                            .copied()
                    })
                    .or(if kind == "music" { Some(1) } else { None })
                    .ok_or("单集 NFO 缺少季号")?;
                let number = doc
                    .number("episode")
                    .or_else(|| doc.number("track"))
                    .ok_or("单集 NFO 缺少集号")?;
                eps.push((season, number, doc));
            }
        } else if let Some((season, first, last)) = filename_numbers(media) {
            if last < first || last - first > 500 {
                return Err("文件名中的合并集数范围无效".into());
            }
            for number in first..=last {
                eps.push((season, number, Node::default()));
            }
        } else if kind == "music" {
            let name = media.file_stem().unwrap_or_default().to_string_lossy();
            if let Ok(number) = name
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("")
                .parse()
            {
                let season = season_dirs
                    .get(&paths::key(media.parent().unwrap()))
                    .copied()
                    .unwrap_or(1);
                eps.push((season, number, Node::default()));
            }
        }
        eps.sort_by_key(|(s, n, _)| (*s, *n));
        if eps.is_empty() {
            continue;
        }
        let (season, first, _) = eps[0];
        let last = eps.last().unwrap().1;
        if eps.iter().enumerate().any(|(i, (s, n, _))| {
            *s != season || *n != first + i as u32 || *n == 0 || *n > 9999 || *s > 999
        }) {
            return Err(format!("{} 的单集 NFO 集号不连续或无效", media.display()));
        }
        if let Some(filename) = filename_numbers(media) {
            if filename != (season, first, last) {
                return Err(format!("{} 的文件名与 NFO 季集号冲突", media.display()));
            }
        }
        if season_dirs
            .get(&paths::key(media.parent().unwrap()))
            .is_some_and(|n| *n != season)
        {
            return Err(format!("{} 的季号与 season.nfo 冲突", media.display()));
        }
        numbers.insert(paths::key(media), (season, first, last));
        if !w.seasons.iter().any(|s| s.number == season) {
            let named = main.children.iter().find(|n| {
                n.name == "namedseason"
                    && n.attrs.get("number").and_then(|v| v.parse::<u32>().ok()) == Some(season)
            });
            let title = named
                .map(|n| n.text.trim().to_string())
                .unwrap_or_else(|| format!("第 {season} 季"));
            w.seasons.push(Season {
                number: season,
                remote_number: season,
                binding_id: main_binding.clone(),
                title: title.clone(),
                remote_title: title,
                ..Default::default()
            });
        }
        let binding_id = w
            .seasons
            .iter()
            .find(|s| s.number == season)
            .unwrap()
            .binding_id
            .clone();
        let ep_provider = w
            .bindings
            .iter()
            .find(|b| b.id == binding_id)
            .map(|b| b.provider.as_str())
            .unwrap_or(provider);
        for (season, number, doc) in eps {
            let info = doc.info(language)?;
            let ep = Episode {
                id: id(),
                binding_id: binding_id.clone(),
                remote_id: info
                    .external_ids
                    .get(if ep_provider == "musicbrainz" { "musicbrainz_track" } else { ep_provider })
                    .cloned()
                    .unwrap_or_else(|| format!("nfo:{season}:{number}")),
                title: if doc.value("title").is_empty() {
                    format!("第 {number} 集")
                } else {
                    doc.value("title")
                },
                season,
                number,
                sort: number as f64,
                kind: if season == 0 { 1 } else { 0 },
                enabled: true,
                info,
            };
            if let Some(old) = w
                .episodes
                .iter_mut()
                .find(|e| e.season == season && e.number == number)
            {
                merge_info(&mut old.info, &ep.info)?;
            } else {
                w.episodes.push(ep);
            }
        }
    }
    if provider == "bangumi" {
        let seasons: Vec<_> = w
            .seasons
            .iter_mut()
            .filter(|s| s.binding_id == main_binding)
            .collect();
        if seasons.len() == 1 {
            let number = seasons[0].number;
            if let Some(binding) = w.bindings.iter_mut().find(|b| b.id == main_binding) {
                binding.season = number;
            }
        }
    }
    Ok(Some((w, numbers)))
}
pub fn import_folder(
    store: &Store,
    dir: &Path,
    kind: &str,
    category: &str,
    files: &[PathBuf],
    language: &str,
) -> Result<Option<ImportedFolder>> {
    inspect_folder(store, dir, kind, category, files, language, None)
}
pub(crate) fn inspect_folder(
    store: &Store,
    dir: &Path,
    kind: &str,
    category: &str,
    files: &[PathBuf],
    language: &str,
    expected_work: Option<&str>,
) -> Result<Option<ImportedFolder>> {
    store.diagnostics.measure(
        "library.import_folder",
        serde_json::json!({
            "kind": kind,
            "files": files.len(),
            "nfos": files.iter().filter(|p| paths::ext(p) == "nfo").count(),
        }),
        || import_folder_inner(store, dir, kind, category, files, language, expected_work),
    )
}
fn import_folder_inner(
    store: &Store,
    dir: &Path,
    kind: &str,
    category: &str,
    files: &[PathBuf],
    language: &str,
    expected_work: Option<&str>,
) -> Result<Option<ImportedFolder>> {
    let bound = store.get::<String>("folders", &paths::key(dir))?;
    if expected_work.is_some_and(|id| bound.as_deref().is_some_and(|owner| owner != id)) {
        return Err("目录关联与所选作品不一致，请重新扫描并确认关联".into());
    }
    let works = if let Some(id) = expected_work.or(bound.as_deref()) {
        store.get::<Work>("works", id)?.into_iter().collect()
    } else {
        store.list::<Work>("works")?
    };
    let loaded = store.diagnostics.measure(
        "library.import.read_metadata",
        serde_json::json!({"kind": kind}),
        || load(store, dir, kind, category, files, language, None),
    )?;
    let sidecar = file_named(files, dir, "bangumi-manager.json")
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Work>(&b).ok())
        .filter(|w| {
            (uuid::Uuid::parse_str(&w.id).is_ok() || bound.as_deref() == Some(&w.id))
                && w.kind == kind
                && crate::providers::validate_work(w).is_ok()
        });
    let (candidate, numbers) = match (loaded, sidecar) {
        (Some((w, mut numbers)), Some(sidecar)) => {
            check_identity(&sidecar, &w)?;
            if kind == "movie" {
                if let Some(ep) = sidecar.episodes.first() {
                    for value in numbers.values_mut() {
                        *value = (ep.season, ep.number, ep.number);
                    }
                }
            }
            (Some(sidecar), numbers)
        }
        (Some((w, numbers)), None) => (Some(w), numbers),
        (None, sidecar) => (sidecar, HashMap::new()),
    };
    let existing = if let Some(bound) = bound {
        Some(
            works
                .iter()
                .find(|w| w.id == bound)
                .ok_or("目录绑定的作品已不存在")?,
        )
    } else if let Some(w) = &candidate {
        let identities = ids(w);
        let matches: Vec<_> = works
            .iter()
            .filter(|old| {
                old.id == w.id
                    || old.kind == kind && identities.iter().any(|(p, id)| same_id(old, p, id))
            })
            .collect();
        if matches.len() > 1 {
            return Err("NFO ID 同时指向多个本地作品，请人工处理重复关联".into());
        }
        if matches.is_empty()
            && works.iter().any(|old| {
                old.kind == kind
                    && title_key(&old.title) == title_key(&w.title)
                    && old.year == w.year
            })
        {
            return Err("已存在同名同年的作品，但 ID 无法确认一致，请人工关联".into());
        }
        matches.first().copied()
    } else {
        None
    };
    for (path, numbers) in &numbers {
        if store
            .get::<(u32, u32, u32)>("asset_identity", path)?
            .is_some_and(|old| old != *numbers)
        {
            return Err("NFO 集号与已保存的资源映射冲突".into());
        }
    }
    let mut imported = false;
    let mut updated = false;
    let work = match (existing, candidate) {
        (Some(old), Some(w)) => {
            let merged = if expected_work.is_some()
                || store
                    .get::<bool>("episode_order", &old.id)?
                    .unwrap_or(false)
            {
                check_identity(old, &w)?;
                // Previews validate file identities without changing the selected catalog data.
                // Sidecars can also retain the previous physical order after a group switch.
                old.clone()
            } else {
                merge_work(old.clone(), &w)?
            };
            updated = serde_json::to_value(old).unwrap() != serde_json::to_value(&merged).unwrap();
            merged
        }
        (Some(old), None) => old.clone(),
        (None, Some(w)) => {
            imported = true;
            w
        }
        (None, None) => return Ok(None),
    };
    if work.kind != kind {
        return Err("目录类型与已关联作品不一致".into());
    }
    if expected_work.is_some_and(|id| work.id != id) {
        return Err("目录关联与所选作品不一致，请重新扫描并确认关联".into());
    }
    crate::providers::validate_work(&work)?;
    // Only a full metadata sync updates the work; previews can confirm a matching folder.
    if expected_work.is_some() {
        store.put("folders", &paths::key(dir), &work.id)?;
    } else {
        store.put_documents(vec![
            (
                "works",
                work.id.clone(),
                serde_json::to_string(&work).unwrap(),
            ),
            (
                "folders",
                paths::key(dir),
                serde_json::to_string(&work.id).unwrap(),
            ),
        ])?;
    }
    Ok(Some(ImportedFolder {
        work_id: work.id,
        imported,
        updated,
        numbers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use std::sync::Arc;
    struct Fixture {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        movie: PathBuf,
        tv: PathBuf,
    }
    fn put(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    impl Fixture {
        fn new() -> Self {
            let d = tempfile::tempdir().unwrap();
            let store = Store::open(d.path().join("state")).unwrap();
            let output = d.path().join("library");
            store
                .put(
                    "settings",
                    "main",
                    &Settings {
                        output_root: paths::display(&output),
                        language: "zh-CN".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            let movie = output.join("电影/动画电影/虚构星海旅人！ (2026)");
            let tv = output.join("电视剧/日番/虚构森林日记 (2017)");
            Self {
                _dir: d,
                store,
                movie,
                tv,
            }
        }
        fn movie(&self) {
            put(&self.movie.join("虚构星海旅人！ (2026).mp4"), "movie");
            put(
                &self.movie.join("虚构星海旅人！ (2026).nfo"),
                r#"<?xml version="1.0" encoding="utf-8"?><movie>
                <tmdbid>910000040</tmdbid><uniqueid default="false" type="tmdb">910000040</uniqueid>
                <uniqueid type="imdb" default="true">tt910000040</uniqueid>
                <actor><name>Actor</name><tmdbid>999</tmdbid><thumb>https://example.invalid/actor.jpg</thumb></actor>
                <title>虚构星海旅人！</title><originaltitle>架空の星海の旅人！</originaltitle>
                <year>2026</year><premiered>2026-01-22</premiered><plot><![CDATA[虚构相遇 & 离别 <星图>]]></plot><genre>动画</genre><country>Japan</country></movie>"#,
            );
            put(&self.movie.join("poster.jpg"), "local poster");
            put(&self.movie.join("fanart.jpg"), "local backdrop");
        }
        fn tv(&self) {
            put(
                &self.tv.join("tvshow.nfo"),
                r#"<tvshow><title>虚构森林日记</title><originaltitle>架空の森の日記</originaltitle><year>2017</year><tmdbid>910000050</tmdbid><uniqueid type='tmdb'>910000050</uniqueid><tvdbid>910000051</tvdbid><season>-1</season><episode>-1</episode><plot><![CDATA[虚构系列简介]]></plot></tvshow>"#,
            );
            put(&self.tv.join("Season 1/season.nfo"),"<season><title>虚构森林日记</title><seasonnumber>1</seasonnumber><premiered>2017-01-12</premiered></season>");
            put(
                &self.tv.join("Season 1/虚构森林日记 - S01E13.mp4"),
                "episode",
            );
            put(
                &self.tv.join("Season 1/虚构森林日记 - S01E13.nfo"),
                r#"<episodedetails><uniqueid type="tmdb">910000052</uniqueid><title>虚构章节：抵达！</title><originaltitle>架空の章、到着！</originaltitle><season>1</season><episode>13</episode><aired>2017-04-06</aired></episodedetails>"#,
            );
        }
    }
    #[test]
    fn mp_nfos_import_offline_with_seasons_episodes_art_and_idempotence() {
        let f = Fixture::new();
        f.movie();
        f.tv();
        let index = scan::index_library(&f.store).unwrap();
        assert!(index.errors.is_empty(), "{:?}", index.errors);
        assert_eq!(index.imported, 2);
        assert_eq!(index.assets.len(), 2);
        assert!(index.unbound.is_empty());
        let works = f.store.list::<Work>("works").unwrap();
        let movie = works.iter().find(|w| w.kind == "movie").unwrap();
        assert_eq!(movie.tmdb_id, "910000040");
        assert_eq!(movie.category, "动画电影");
        assert_eq!(movie.info.original_title, "架空の星海の旅人！");
        assert_eq!(movie.overview, "虚构相遇 & 离别 <星图>");
        assert_eq!(fs::read(&movie.info.local_poster).unwrap(), b"local poster");
        assert!(!f.movie.join("movie.nfo").exists());
        assert!(!f.movie.join("bangumi-manager.json").exists());
        let tv = works.iter().find(|w| w.kind == "tv").unwrap();
        assert_eq!(tv.tmdb_id, "910000050");
        assert_eq!(tv.info.external_ids.get("tvdb").unwrap(), "910000051");
        assert_eq!(tv.seasons[0].number, 1);
        assert_eq!(tv.seasons[0].title, "虚构森林日记");
        assert_eq!(tv.episodes[0].remote_id, "910000052");
        assert_eq!(tv.episodes[0].number, 13);
        assert_eq!(tv.episodes[0].info.original_title, "架空の章、到着！");
        let list_count = || {
            f.store.diagnostics.report().statistics.iter()
                .find(|s| s.operation == "db.list.works").unwrap().count
        };
        let before = list_count();
        let second = scan::index_library(&f.store).unwrap();
        assert_eq!(
            list_count(), before,
            "Bound folders should only load their associated work",
        );
        assert_eq!(second.imported, 0);
        assert_eq!(second.updated, 0);
        for resource in &second.resources {
            let asset = resource.asset.as_ref().unwrap();
            assert_eq!(asset.path, resource.path);
            assert_eq!(asset.signature, resource.signature);
            assert!(second.assets.iter().any(|a| {
                a.path == asset.path && a.work_id == asset.work_id
            }));
        }
        assert_eq!(f.store.list::<Work>("works").unwrap().len(), 2);
    }
    #[test]
    fn library_scan_records_phases_and_counts_without_media_paths() {
        let f = Fixture::new();
        f.tv();
        scan::index_library(&f.store).unwrap();
        let raw = fs::read_to_string(
            f.store.diagnostics.directory().join("performance.jsonl"),
        ).unwrap();
        assert!(!raw.contains("虚构森林日记"));
        assert!(!raw.contains(&paths::display(&f.tv)));
        let events: Vec<serde_json::Value> = raw.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let finished = |operation: &str| {
            events.iter().find(|e| {
                e["operation"] == operation && e["phase"] == "finish"
            }).unwrap()
        };
        let scan = finished("library.scan");
        assert_eq!(scan["fields"]["folders"], 1);
        assert_eq!(scan["fields"]["resources"], 1);
        assert_eq!(scan["fields"]["assets"], 1);
        assert_eq!(scan["fields"]["errors"], 0);
        for operation in [
            "library.scan.lock_wait",
            "library.scan.collect",
            "library.scan.walk_folder",
            "library.import_folder",
            "library.import.read_metadata",
            "library.scan.index_files",
            "library.scan.link_resources",
            "library.scan.persist",
        ] {
            let event = finished(operation);
            assert_eq!(event["trace_id"], scan["trace_id"]);
            assert_eq!(event["status"], "ok");
            assert!(event["elapsed_ms"].as_f64().unwrap() >= 0.0);
        }
        assert_eq!(finished("library.import_folder")["fields"]["nfos"], 3);
    }
    #[test]
    fn duplicate_legacy_roots_preserve_the_categorized_output_layout() {
        let f = Fixture::new();
        f.movie();
        f.tv();
        let mut settings = f.store.settings().unwrap();
        settings.movie_root = paths::display(&crate::naming::root(&settings, "movie").unwrap());
        settings.tv_root = paths::display(&crate::naming::root(&settings, "tv").unwrap());
        f.store.put("settings", "main", &settings).unwrap();
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.imported, 2, "{:?}", index.errors);
        assert_eq!(index.assets.len(), 2);
        assert!(index.unbound.is_empty());
    }
    #[test]
    fn malformed_or_conflicting_nfo_does_not_partially_import_a_folder() {
        let f = Fixture::new();
        f.movie();
        f.tv();
        let path = f.tv.join("Season 1/虚构森林日记 - S01E13.nfo");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("<episode>13</episode>", "<episode>6</episode>");
        put(&path, &text);
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.imported, 1);
        assert_eq!(index.assets.len(), 1);
        assert!(index.unbound[0].reason.contains("文件名与 NFO"));
        assert!(f
            .store
            .get::<String>("folders", &paths::key(&f.tv))
            .unwrap()
            .is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        let nfo = f.movie.join("虚构星海旅人！ (2026).nfo");
        put(
            &nfo,
            &fs::read_to_string(&nfo)
                .unwrap()
                .replace("<tmdbid>910000040</tmdbid>", "<tmdbid>7</tmdbid>"),
        );
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.assets.len(), 0);
        assert_eq!(index.unbound.len(), 2);
    }
    #[test]
    fn duplicate_local_ids_and_conflicting_movie_nfos_require_manual_resolution() {
        let f = Fixture::new();
        f.movie();
        for _ in 0..2 {
            let w = Work {
                id: id(),
                title: "duplicate".into(),
                folder_name: "duplicate".into(),
                kind: "movie".into(),
                tmdb_id: "910000040".into(),
                ..Default::default()
            };
            f.store.put("works", &w.id, &w).unwrap();
        }
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.imported, 0);
        assert!(index.unbound[0].reason.contains("多个本地作品"));
        f.store.clear("works").unwrap();
        put(
            &f.movie.join("movie.nfo"),
            "<movie><title>another</title><tmdbid>7</tmdbid></movie>",
        );
        let index = scan::index_library(&f.store).unwrap();
        assert_eq!(index.imported, 0);
        assert!(index.unbound[0].reason.contains("ID 冲突"));
    }
    #[test]
    fn rescanning_preserves_manual_episode_and_season_edits_and_adds_missing_episodes() {
        let f = Fixture::new();
        f.tv();
        scan::index_library(&f.store).unwrap();
        let mut w = f.store.list::<Work>("works").unwrap().remove(0);
        w.title = "自定义剧名".into();
        w.seasons[0].title = "自定义季名".into();
        w.seasons[0].number = 2;
        w.episodes[0].number = 14;
        w.episodes[0].season = 2;
        w.episodes[0].enabled = false;
        f.store.put("works", &w.id, &w).unwrap();
        put(&f.tv.join("Season 1/show S01E12.mkv"), "episode");
        let index = scan::index_library(&f.store).unwrap();
        assert!(index.errors.is_empty(), "{:?}", index.errors);
        let saved = f.store.get::<Work>("works", &w.id).unwrap().unwrap();
        assert_eq!(saved.title, w.title);
        assert_eq!(saved.seasons[0].title, w.seasons[0].title);
        assert_eq!(saved.seasons.len(), 1);
        assert_eq!(saved.seasons[0].number, 2);
        let ep = saved
            .episodes
            .iter()
            .find(|e| e.id == w.episodes[0].id)
            .unwrap();
        assert_eq!((ep.season, ep.number, ep.enabled), (2, 14, false));
        assert_eq!(saved.episodes.len(), 2);
    }
    #[test]
    fn metadata_without_remote_ids_or_media_can_be_imported_without_guessing_an_existing_identity()
    {
        let f = Fixture::new();
        put(
            &f.tv.join("tvshow.nfo"),
            "<tvshow><title>本地条目</title><year>2026</year></tvshow>",
        );
        let first = scan::index_library(&f.store).unwrap();
        assert_eq!(first.imported, 1);
        assert!(first.assets.is_empty());
        let second = scan::index_library(&f.store).unwrap();
        assert_eq!(second.imported, 0);
        assert_eq!(second.updated, 0);
        assert_eq!(
            f.store.list::<Work>("works").unwrap()[0].bindings[0].provider,
            "nfo"
        );
    }
    #[test]
    fn remote_sync_reuses_imported_episode_placeholders_and_preserves_external_ids() {
        let f = Fixture::new();
        f.tv();
        let path = f.tv.join("Season 1/虚构森林日记 - S01E13.nfo");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("<uniqueid type=\"tmdb\">910000052</uniqueid>", "");
        put(&path, &text);
        scan::index_library(&f.store).unwrap();
        let mut work = f.store.list::<Work>("works").unwrap().remove(0);
        let ep_id = work.episodes[0].id.clone();
        work.episodes[0].number = 14;
        f.store.put("works", &work.id, &work).unwrap();
        let subject = RemoteSubject {
            episode_group: None,
            provider: "tmdb".into(),
            id: "910000050".into(),
            kind: "tv".into(),
            title: work.title.clone(),
            seasons: vec![Season {
                number: 1,
                remote_number: 1,
                title: "远端季名".into(),
                ..Default::default()
            }],
            episodes: vec![RemoteEpisode {
                id: "910000052".into(),
                sort: 13.,
                season: 1,
                title: "远端集名".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let saved = crate::providers::bind(
            &f.store,
            BindRequest {
                group_change_confirmed: false,
                work_id: Some(work.id.clone()),
                subject,
                kind: "tv".into(),
                title: work.title,
                season: 1,
                tmdb_id: "910000050".into(),
                parent_id: String::new(),
            },
        )
        .unwrap();
        assert_eq!(saved.episodes.len(), 1);
        assert_eq!(saved.episodes[0].id, ep_id);
        assert_eq!(saved.episodes[0].number, 14);
        assert_eq!(saved.episodes[0].remote_id, "910000052");
        assert_eq!(saved.info.external_ids.get("tvdb").unwrap(), "910000051");
        assert_eq!(saved.seasons[0].title, "虚构森林日记");
        scan::index_library(&f.store).unwrap();
        let saved = f.store.get::<Work>("works", &work.id).unwrap().unwrap();
        assert_eq!(saved.episodes.len(), 1);
        assert_eq!(saved.episodes[0].number, 14);
    }
    #[test]
    fn legacy_movie_nfo_and_multi_episode_nfo_are_supported() {
        let f = Fixture::new();
        f.movie();
        f.tv();
        fs::rename(
            f.movie.join("虚构星海旅人！ (2026).nfo"),
            f.movie.join("movie.nfo"),
        )
        .unwrap();
        put(&f.tv.join("Season 1/double S01E01-E02.mkv"), "double");
        put(&f.tv.join("Season 1/double S01E01-E02.nfo"),"<?xml version=\"1.0\"?><episodedetails><title>A</title><season>1</season><episode>1</episode></episodedetails><episodedetails><title>B</title><season>1</season><episode>2</episode></episodedetails>");
        let index = scan::index_library(&f.store).unwrap();
        assert!(index.errors.is_empty(), "{:?}", index.errors);
        assert_eq!(index.assets.len(), 3);
        assert!(index.assets.iter().any(|a| a.number == 1 && a.end == 2));
    }
    #[test]
    #[ignore = "Read-only check of user-specified NFO directories; requires NFO_MOVIE_SAMPLE and NFO_TV_SAMPLE"]
    fn verify_user_nfo_samples_read_only() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().join("state")).unwrap();
        for (env, kind, category, expected) in [
            ("NFO_MOVIE_SAMPLE", "movie", "动画电影", "910000040"),
            ("NFO_TV_SAMPLE", "tv", "日番", "910000050"),
        ] {
            let folder = PathBuf::from(std::env::var(env).unwrap());
            let files: Vec<_> = walkdir::WalkDir::new(&folder)
                .follow_links(false)
                .into_iter()
                .map(|e| e.unwrap())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .collect();
            let result = import_folder(&store, &folder, kind, category, &files, "zh-CN")
                .unwrap()
                .unwrap();
            let w = store
                .get::<Work>("works", &result.work_id)
                .unwrap()
                .unwrap();
            assert_eq!(w.tmdb_id, expected);
            if kind == "tv" {
                assert!(w.episodes.iter().any(|e| e.season == 1 && e.number == 13));
            }
            println!(
                "{}: TMDB {}, {} seasons, {} episodes, {} media identities",
                w.title,
                w.tmdb_id,
                w.seasons.len(),
                w.episodes.len(),
                result.numbers.len()
            );
            assert!(
                !import_folder(&store, &folder, kind, category, &files, "zh-CN")
                    .unwrap()
                    .unwrap()
                    .imported
            );
        }
    }
}
