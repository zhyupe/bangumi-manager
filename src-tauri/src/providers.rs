use crate::{db::Store, model::*};
use serde_json::{json, Value};

fn text(v: &Value, field: &str) -> String {
    v[field].as_str().unwrap_or("").to_string()
}
fn number(v: &Value, field: &str) -> String {
    v[field].as_u64().map(|n| n.to_string()).unwrap_or_default()
}
fn tmdb_image(v: &Value, key: &str) -> String {
    let p = text(v, key);
    if p.is_empty() {
        String::new()
    } else {
        format!("https://image.tmdb.org/t/p/w780{p}")
    }
}
pub(crate) fn tmdb_info(
    v: &Value,
    original: &Value,
    lang: &str,
    original_lang: &str,
    name: &str,
) -> MediaInfo {
    MediaInfo {
        localized_title: text(v, name),
        original_title: text(original, name),
        language: lang.into(),
        original_language: original_lang.into(),
        overview: text(v, "overview"),
        original_overview: text(original, "overview"),
        poster: tmdb_image(
            v,
            if v["still_path"].is_string() {
                "still_path"
            } else {
                "poster_path"
            },
        ),
        backdrop: tmdb_image(v, "backdrop_path"),
        air_date: text(v, "air_date"),
        genres: v["genres"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|x| x["id"].as_u64().map(|n| n as u32))
            .collect(),
        countries: v["origin_country"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                v["production_countries"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|x| text(x, "iso_3166_1"))
                    .collect()
            }),
        ..Default::default()
    }
}
pub fn sync(
    store: &Store,
    work_id: &str,
    binding_id: Option<&str>,
    season: Option<u32>,
) -> Result<Work> {
    store.diagnostics.measure(
        "provider.sync",
        serde_json::json!({"work_id":work_id,"season":season}),
        || {
            let mut work = store.get::<Work>("works", work_id)?.ok_or("作品不存在")?;
            let bindings: Vec<_> = work
                .bindings
                .iter()
                .filter(|b| {
                    ["tmdb", "bangumi", "musicbrainz"].contains(&b.provider.as_str())
                        && binding_id.is_none_or(|id| id == b.id)
                })
                .cloned()
                .collect();
            if bindings.is_empty() {
                return Err("此作品只有本地 NFO 资料，请先关联远程数据源".into());
            }
            for b in &bindings {
                let remote_season = if b.provider == "musicbrainz" {
                    season.and_then(|n| work.seasons.iter().find(|s| s.binding_id == b.id && s.number == n).map(|s| s.remote_number))
                } else { season };
                if b.provider == "musicbrainz" && season.is_some() && remote_season.is_none() { continue; }
                let subject = detail_ordered(
                    store,
                    &b.provider,
                    &b.remote_id,
                    &b.source_kind,
                    true,
                    remote_season,
                    b.episode_group.as_ref().map(|g| g.id.as_str()),
                )?;
                work = bind(
                    store,
                    BindRequest {
                        group_change_confirmed: false,
                        work_id: Some(work.id.clone()),
                        subject,
                        title: work.title.clone(),
                        kind: work.kind.clone(),
                        season: b.season,
                        tmdb_id: work.tmdb_id.clone(),
                        parent_id: work.parent_id.clone(),
                    },
                )?;
            }
            crate::metadata::cache_work(store, &mut work);
            store.put("works", &work.id, &work)?;
            crate::metadata::refresh_existing(store, &work)?;
            Ok(work)
        },
    )
}
pub(crate) fn request(
    store: &Store,
    url: &str,
    body: Option<Value>,
    refresh: bool,
) -> Result<(Value, bool)> {
    let details = crate::diagnostics::endpoint(url);
    store
        .diagnostics
        .measure_with("http.request", details.clone(), |timing| {
            timing.field("refresh", refresh);
            let cache_key = format!(
                "{}:{}",
                url,
                body.as_ref().map(|v| v.to_string()).unwrap_or_default()
            );
            if !refresh {
                if let Some(v) = store.cache(&cache_key, false)? {
                    timing.field("cache", "fresh_hit");
                    return Ok((v, false));
                }
            }
            timing.field("cache", if refresh { "refresh" } else { "miss" });
            let client = store.http_client()?;
            let mut request = if let Some(b) = body {
                client.post(url).json(&b)
            } else {
                client.get(url)
            };
            if url.starts_with("https://api.themoviedb.org/") {
                let token = store.settings()?.tmdb_token;
                if token.trim().is_empty() {
                    timing.field("error_kind", "missing_token");
                    return Err("请先在设置中填写 TMDB API Read Access Token".into());
                }
                request = request.bearer_auth(token.trim());
            }
            let is_musicbrainz = url.starts_with("https://musicbrainz.org/ws/2/");
            if is_musicbrainz {
                request = request.header(reqwest::header::USER_AGENT,
                    crate::musicbrainz::user_agent(&store.settings()?.musicbrainz_contact)?);
            }
            let musicbrainz_permit = if is_musicbrainz { Some(crate::musicbrainz::throttle(store)?) } else { None };
            let response =
                store
                    .diagnostics
                    .measure_with("http.headers", details.clone(), |phase| {
                        let result = request.send();
                        if let Ok(response) = &result {
                            phase.field("http_status", response.status().as_u16());
                            timing.field("http_status", response.status().as_u16());
                        }
                        result.and_then(|r| r.error_for_status())
                    });
            drop(musicbrainz_permit);
            let result = response.and_then(|response| {
                store
                    .diagnostics
                    .measure("http.body_json", details, || response.json::<Value>())
            });
            match result {
                Ok(v) => {
                    store.cache_put(&cache_key, &v)?;
                    Ok((v, false))
                }
                Err(e) => {
                    timing.field(
                        "error_kind",
                        if e.is_timeout() {
                            "timeout"
                        } else if e.is_connect() {
                            "connect"
                        } else if e.is_status() {
                            "http_status"
                        } else if e.is_decode() {
                            "decode"
                        } else {
                            "network"
                        },
                    );
                    match store.cache(&cache_key, true)? {
                        Some(v) => {
                            timing.field("cache", "stale_fallback");
                            Ok((v, true))
                        }
                        None if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => {
                            Err(if is_musicbrainz { "HTTP 404：MusicBrainz 发行版本不存在，请核对 MBID" } else { "HTTP 404：远程条目不存在，请核对电影 / 剧集类型和 ID" }.into())
                        }
                        None if is_musicbrainz && matches!(e.status().map(|s| s.as_u16()), Some(429 | 503)) =>
                            Err("MusicBrainz 暂时限流或服务繁忙，请稍后重试".into()),
                        None => Err(format!(
                            "远程请求失败，已保留其他缓存，可重试：{}",
                            e.without_url()
                        )),
                    }
                }
            }
        })
}

fn project_bangumi(v: &Value) -> RemoteSubject {
    let original = text(v, "name");
    let chinese = text(v, "name_cn");
    let infobox = |keys: &[&str]| -> String {
        v["infobox"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .find(|item| keys.contains(&text(item, "key").as_str()))
            .map(|item| {
                item["value"].as_str().map(String::from).unwrap_or_else(|| {
                    item["value"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|v| text(v, "v"))
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                        .join(" / ")
                })
            })
            .unwrap_or_default()
    };
    RemoteSubject {
        episode_group: None,
        info: MediaInfo {
            localized_title: chinese.clone(),
            original_title: original.clone(),
            overview: text(v, "summary"),
            poster: text(&v["images"], "large"),
            genres: if v["type"].as_u64() == Some(2) {
                vec![16]
            } else {
                vec![]
            },
            language: "zh-CN".into(),
            air_date: text(v, "date"),
            artist: infobox(&["艺术家", "艺术家名", "表演者"]),
            ..Default::default()
        },
        seasons: vec![],
        provider: "bangumi".into(),
        id: number(v, "id"),
        kind: if v["type"].as_u64() == Some(3) {
            "music"
        } else if ["剧场版", "Movie", "movie"].contains(&text(v, "platform").as_str()) {
            "movie"
        } else {
            "tv"
        }
        .into(),
        title: if chinese.is_empty() {
            original.clone()
        } else {
            chinese
        },
        original_title: original,
        year: text(v, "date").chars().take(4).collect(),
        overview: text(v, "summary"),
        poster: text(&v["images"], "large"),
        episodes: vec![],
        relations: vec![],
        cached: false,
    }
}
fn project_tmdb(v: &Value, kind: &str) -> RemoteSubject {
    let movie = kind == "movie";
    RemoteSubject {
        episode_group: None,
        info: MediaInfo::default(),
        seasons: vec![],
        provider: "tmdb".into(),
        id: number(v, "id"),
        kind: kind.into(),
        title: text(v, if movie { "title" } else { "name" }),
        original_title: text(
            v,
            if movie {
                "original_title"
            } else {
                "original_name"
            },
        ),
        year: text(
            v,
            if movie {
                "release_date"
            } else {
                "first_air_date"
            },
        )
        .chars()
        .take(4)
        .collect(),
        overview: text(v, "overview"),
        poster: if text(v, "poster_path").is_empty() {
            String::new()
        } else {
            format!("https://image.tmdb.org/t/p/w500{}", text(v, "poster_path"))
        },
        episodes: vec![],
        relations: vec![],
        cached: false,
    }
}
pub fn search(
    store: &Store,
    provider: &str,
    query: &str,
    kind: &str,
) -> Result<Vec<RemoteSubject>> {
    store.diagnostics.measure(
        "provider.search",
        serde_json::json!({"provider":provider,"kind":kind}),
        || {

    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }
    match provider {
        "musicbrainz" => {
            if kind != "music" { return Err("MusicBrainz 仅用于音乐专辑".into()); }
            Ok(crate::musicbrainz::releases(store, query, 0, false)?.items)
        }
        "bangumi" => {
            let (v, stale) = request(
                store,
                "https://api.bgm.tv/v0/search/subjects?limit=30&offset=0",
                Some(
                    json!({"keyword":query,"filter":{"type":if kind == "music" { vec![3] } else { vec![2,6] }},"sort":"match"}),
                ),
                false,
            )?;
            Ok(v["data"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| {
                    let mut s = project_bangumi(v);
                    s.cached = stale;
                    s
                })
                .collect())
        }
        "tmdb" => {
            if kind == "music" {
                return Err("音乐专辑请使用 Bangumi 或 MusicBrainz 数据源".into());
            }
            let mut url =
                url::Url::parse(&"https://api.themoviedb.org/3/search/multi").unwrap();
            url.query_pairs_mut()
                .append_pair("query", query)
                .append_pair("language", &store.settings()?.language);
            let (v, stale) = request(store, url.as_str(), None, false)?;
            Ok(v["results"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter(|v| matches!(v["media_type"].as_str(), Some("movie" | "tv")))
                .map(|v| {
                    let mut s = project_tmdb(v, v["media_type"].as_str().unwrap_or(kind));
                    s.cached = stale;
                    s
                })
                .collect())
        }
        _ => Err("不支持的数据源".into()),
    }

    })
}
pub fn detail(
    store: &Store,
    provider: &str,
    id: &str,
    kind: &str,
    refresh: bool,
) -> Result<RemoteSubject> {
    let id = id.trim();
    let numeric = !id.is_empty() && id.chars().all(|c| c.is_ascii_digit());
    if provider == "musicbrainz" {
        return detail_ordered(store, provider, id, kind, refresh, None, None);
    }
    if provider != "tmdb" && !numeric {
        return Err("Bangumi 条目 ID 必须为数字".into());
    }
    let (id, resolved_kind) = parse_reference(id, kind)?;
    match detail_ordered(store, provider, &id, &resolved_kind, refresh, None, None) {
        Err(e) if provider == "tmdb" && numeric && e.starts_with("TMDB 条目 HTTP 404：") => {
            let alternate = if resolved_kind == "movie" {
                "tv"
            } else {
                "movie"
            };
            detail_ordered(store, provider, &id, alternate, refresh, None, None)
        }
        result => result,
    }
}
pub fn parse_reference(raw: &str, kind: &str) -> Result<(String, String)> {
    let raw = raw.trim();
    if raw.chars().all(|c| c.is_ascii_digit()) && !raw.is_empty() {
        return Ok((raw.into(), kind.into()));
    }
    if let Ok(url) = url::Url::parse(raw) {
        if matches!(
            url.host_str(),
            Some("www.themoviedb.org" | "themoviedb.org")
        ) {
            let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
            if parts.len() >= 2 && ["movie", "tv"].contains(&parts[0]) {
                let id = parts[1].split('-').next().unwrap_or("");
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                    return Ok((id.into(), parts[0].into()));
                }
            }
        }
    }
    Err("请输入数字 ID 或 TMDB 电影 / 剧集链接".into())
}
#[cfg(test)]
pub fn detail_season(
    store: &Store,
    provider: &str,
    id: &str,
    kind: &str,
    refresh: bool,
    only_season: Option<u32>,
) -> Result<RemoteSubject> {
    detail_ordered(store, provider, id, kind, refresh, only_season, None)
}
pub fn detail_ordered(
    store: &Store,
    provider: &str,
    id: &str,
    kind: &str,
    refresh: bool,
    only_season: Option<u32>,
    group_id: Option<&str>,
) -> Result<RemoteSubject> {
    store.diagnostics.measure(
        "provider.detail",
        serde_json::json!({"provider":provider,"remote_id":id,"kind":kind,"refresh":refresh,"season":only_season}),
        || {

    if provider == "musicbrainz" {
        if kind != "music" { return Err("MusicBrainz 仅用于音乐专辑".into()); }
        return crate::musicbrainz::detail(store, id, refresh, only_season);
    }
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("资料库 ID 必须为数字".into());
    }
    match provider {
        "bangumi" => {
            let (v, stale) = request(
                store,
                &format!("https://api.bgm.tv/v0/subjects/{id}"),
                None,
                refresh,
            )?;
            if ![Some(2), Some(3), Some(6)].contains(&v["type"].as_u64()) {
                return Err("请选择 Bangumi 动画、音乐或三次元条目".into());
            }
            let mut s = project_bangumi(&v);
            s.cached = stale;
            let mut offset = 0;
            loop {
                let (page, cached) = request(
                    store,
                    &format!(
                        "https://api.bgm.tv/v0/episodes?subject_id={id}&limit=100&offset={offset}"
                    ),
                    None,
                    refresh,
                )?;
                s.cached |= cached;
                let items = page["data"]
                    .as_array()
                    .ok_or("Bangumi 返回了无效的集数列表")?;
                for ep in items {
                    s.episodes.push(RemoteEpisode {
                        info: MediaInfo {
                            localized_title: text(ep, "name_cn"),
                            original_title: text(ep, "name"),
                            overview: text(ep, "desc"),
                            air_date: text(ep, "airdate"),
                            language: "zh-CN".into(),
                            ..Default::default()
                        },
                        id: number(ep, "id"),
                        sort: ep["sort"].as_f64().unwrap_or(0.0),
                        title: {
                            let n = text(ep, "name_cn");
                            if n.is_empty() {
                                text(ep, "name")
                            } else {
                                n
                            }
                        },
                        kind: if s.kind == "music" {
                            0
                        } else {
                            ep["type"].as_u64().unwrap_or(6) as u8
                        },
                        season: if s.kind == "music" {
                            ep["disc"].as_u64().unwrap_or(1).max(1) as u32
                        } else {
                            1
                        },
                    });
                }
                offset += items.len();
                if items.is_empty()
                    || offset >= page["total"].as_u64().unwrap_or(offset as u64) as usize
                {
                    break;
                }
                if offset > 10000 {
                    return Err("集数分页异常，请重试".into());
                }
            }
            if let Ok((related, cached)) = request(
                store,
                &format!("https://api.bgm.tv/v0/subjects/{id}/subjects"),
                None,
                refresh,
            ) {
                s.cached |= cached;
                s.relations = related
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter(|x| x["type"].as_u64() == Some(2))
                    .map(|r| Relation {
                        id: number(r, "id"),
                        title: text(r, "name"),
                        relation: text(r, "relation"),
                    })
                    .collect();
            }
            Ok(s)
        }
        "tmdb" => {
            if kind == "music" {
                return Err("TMDB 不支持音乐专辑".into());
            }
            let kind = if kind == "movie" { "movie" } else { "tv" };
            let lang = store.settings()?.language;
            let (v, stale) = request(
                store,
                &format!("https://api.themoviedb.org/3/{kind}/{id}?language={lang}"),
                None,
                refresh,
            ).map_err(|e| if e.starts_with("HTTP 404：") { format!("TMDB 条目 {e}") } else { e })?;
            let mut s = project_tmdb(&v, kind);
            s.cached = stale;
            let original_lang = text(&v, "original_language");
            let (original, cached) = if !original_lang.is_empty() && original_lang != lang {
                request(
                    store,
                    &format!("https://api.themoviedb.org/3/{kind}/{id}?language={original_lang}"),
                    None,
                    refresh,
                )?
            } else {
                (v.clone(), false)
            };
            s.cached |= cached;
            s.info = tmdb_info(
                &v,
                &original,
                &lang,
                &original_lang,
                if kind == "movie" { "title" } else { "name" },
            );
            s.info.original_title = s.original_title.clone();
            s.info.air_date = text(
                &v,
                if kind == "movie" {
                    "release_date"
                } else {
                    "first_air_date"
                },
            );
            if kind == "tv" {
                if let Some(group_id) = group_id.filter(|g| !g.is_empty()) {
                    crate::tmdb_groups::apply(store, &mut s, group_id, &lang, &original_lang, refresh, only_season)?;
                    return Ok(s);
                }
                for season in v["seasons"].as_array().unwrap_or(&vec![]) {
                    let n = season["season_number"].as_u64().unwrap_or(0);
                    if only_season.is_some_and(|only| only != n as u32) {
                        continue;
                    }
                    let (page, cached) = request(
                        store,
                        &format!("https://api.themoviedb.org/3/tv/{id}/season/{n}?language={lang}"),
                        None,
                        refresh,
                    )?;
                    s.cached |= cached;
                    let (original_page, cached) = if !original_lang.is_empty()
                        && original_lang != lang
                    {
                        request(store, &format!("https://api.themoviedb.org/3/tv/{id}/season/{n}?language={original_lang}"), None, refresh)?
                    } else {
                        (page.clone(), false)
                    };
                    s.cached |= cached;
                    s.seasons.push(Season {
                        number: n as u32,
                        remote_number: n as u32,
                        title: text(&page, "name"),
                        remote_title: text(&page, "name"),
                        info: tmdb_info(&page, &original_page, &lang, &original_lang, "name"),
                        ..Default::default()
                    });
                    for ep in page["episodes"].as_array().unwrap_or(&vec![]) {
                        let original_ep = original_page["episodes"]
                            .as_array()
                            .and_then(|items| items.iter().find(|x| x["id"] == ep["id"]))
                            .unwrap_or(ep);
                        s.episodes.push(RemoteEpisode {
                            info: tmdb_info(ep, original_ep, &lang, &original_lang, "name"),
                            id: number(ep, "id"),
                            sort: ep["episode_number"].as_f64().unwrap_or(0.),
                            title: text(ep, "name"),
                            kind: if n == 0 { 1 } else { 0 },
                            season: n as u32,
                        });
                    }
                }
            }
            Ok(s)
        }
        _ => Err("不支持的数据源".into()),
    }

    })
}

pub fn bind(store: &Store, req: BindRequest) -> Result<Work> {
    store.diagnostics.measure(
        "provider.bind",
        serde_json::json!({"provider":req.subject.provider,"remote_id":req.subject.id,"episodes":req.subject.episodes.len(),"seasons":req.subject.seasons.len()}),
        || {

    if !["tv", "movie", "music"].contains(&req.kind.as_str()) {
        return Err("媒体类型错误".into());
    }
    let s = &req.subject;
    if s.provider == "musicbrainz" && (s.kind != "music" || req.kind != "music" || s.episodes.is_empty()) {
        return Err("请选择包含音轨的 MusicBrainz 发行版本，并以音乐专辑入库".into());
    }
    for existing in store.list::<Work>("works")? {
        if Some(existing.id.as_str()) != req.work_id.as_deref()
            && existing
                .bindings
                .iter()
                .any(|b| b.provider == s.provider && b.remote_id == s.id && b.source_kind == s.kind)
        {
            return Err(format!(
                "此远程条目已关联到 {}，请选择该本地条目；需要改季号时在剧集目录编辑",
                existing.title
            ));
        }
    }
    let new = req.work_id.as_deref().unwrap_or("").is_empty();
    let mut work = if new {
        let title = if req.title.trim().is_empty() {
            s.original_title.clone()
        } else {
            req.title.trim().into()
        };
        if title.is_empty() {
            return Err("请输入主剧名".into());
        }
        let folder = if req.kind == "movie" && !s.year.is_empty() {
            format!("{} ({})", title, s.year)
        } else {
            title.clone()
        };
        Work {
            info: s.info.clone(),
            seasons: vec![],
            category: String::new(),
            metadata_errors: vec![],
            metadata_notices: vec![],
            id: crate::model::id(),
            title,
            kind: req.kind.clone(),
            year: s.year.clone(),
            folder_name: crate::paths::clean_name(&folder),
            tmdb_id: req.tmdb_id.clone(),
            parent_id: req.parent_id.clone(),
            overview: s.overview.clone(),
            poster: s.poster.clone(),
            bindings: vec![],
            episodes: vec![],
        }
    } else {
        store
            .get::<Work>("works", req.work_id.as_deref().unwrap())?
            .ok_or("本地条目不存在")?
    };
    if work.kind != req.kind {
        return Err("电影与剧集不能绑定到同一个本地条目；请新建电影并设置所属系列".into());
    }
    let mut binding = work
        .bindings
        .iter()
        .find(|b| b.provider == s.provider && b.remote_id == s.id && b.source_kind == s.kind)
        .cloned()
        .unwrap_or(Binding {
        episode_group: None,
            id: crate::model::id(),
            provider: s.provider.clone(),
            remote_id: s.id.clone(),
            source_kind: s.kind.clone(),
            source_title: s.original_title.clone(),
            season: req.season,
        });
    let group_changed = binding.episode_group.as_ref().map(|g| &g.id) != s.episode_group.as_ref().map(|g| &g.id);
    if group_changed && !new && !req.group_change_confirmed {
        return Err("切换剧集组会更改季集编号，请先核对并确认映射".into());
    }
    if group_changed && !new {
        if store.list::<Plan>("plans")?.iter().any(|p| !["complete", "cancelled"].contains(&p.state.as_str()) && p.rows.iter().any(|r| r.work_id == work.id)) {
            return Err("此作品还有未结束的整理任务，请先取消或完成，再切换剧集组并重新生成计划".into());
        }
        // Retain local IDs for file associations. Unlisted episodes remain visible but disabled.
        work.seasons.retain(|season| season.binding_id != binding.id);
        for ep in work.episodes.iter_mut().filter(|e| e.binding_id == binding.id) {
            ep.enabled = false;
        }
    }
    binding.episode_group = s.episode_group.clone();
    if let Some(old) = work.bindings.iter_mut().find(|b| b.id == binding.id) { *old = binding.clone(); }
    if !work.bindings.iter().any(|b| b.id == binding.id) {
        work.bindings.push(binding.clone());
    }
    if s.provider == "tmdb" {
        work.tmdb_id = s.id.clone();
    }
    if work.kind == "movie" && work.episodes.is_empty() {
        work.episodes.push(Episode {
            info: s.info.clone(),
            id: crate::model::id(),
            binding_id: binding.id.clone(),
            remote_id: s.id.clone(),
            sort: 1.,
            title: work.title.clone(),
            kind: 0,
            season: 1,
            number: 1,
            enabled: true,
        });
    }
    let local_bindings: std::collections::HashSet<_> = work
        .bindings
        .iter()
        .filter(|b| b.provider == "nfo" || b.provider == "cue")
        .map(|b| b.id.clone())
        .collect();
    if work.kind != "movie" {
        for remote in &s.episodes {
            let source_season = if s.provider == "musicbrainz" {
                work.seasons.iter().find(|d| d.binding_id == binding.id && d.remote_number == remote.season)
                    .map(|d| d.number).unwrap_or(remote.season)
            } else if s.provider == "tmdb" || s.kind == "music" {
                remote.season
            } else if remote.kind == 0 {
                binding.season
            } else {
                0
            };
            let placeholder = format!("nfo:{source_season}:{}", remote.sort);
            if let Some(ep) = work.episodes.iter_mut().find(|ep| {
                (ep.binding_id == binding.id && ep.remote_id == remote.id)
                    || ((ep.binding_id == binding.id || local_bindings.contains(&ep.binding_id))
                        && ep.remote_id == placeholder)
            }) {
                ep.title = remote.title.clone();
                let mut ids = ep.info.external_ids.clone();
                ids.extend(remote.info.external_ids.clone());
                ep.info = remote.info.clone();
                ep.info.external_ids = ids;
                ep.remote_id = remote.id.clone();
                ep.binding_id = binding.id.clone();
                if group_changed {
                    ep.season = remote.season;
                    ep.number = remote.sort as u32;
                    ep.sort = remote.sort;
                    ep.kind = remote.kind;
                    ep.enabled = remote.sort > 0. && remote.sort.fract() == 0.;
                }
                continue;
            }
            // Provider numbering is kept separately. Specials remain pending until explicitly mapped.
            let season = source_season;
            let integer = remote.sort.fract() == 0. && remote.sort > 0. && remote.sort < 10000.;
            let enabled = integer && (s.provider == "tmdb" || remote.kind == 0);
            work.episodes.push(Episode {
                info: remote.info.clone(),
                id: crate::model::id(),
                binding_id: binding.id.clone(),
                remote_id: remote.id.clone(),
                sort: remote.sort,
                title: remote.title.clone(),
                kind: remote.kind,
                season,
                number: if integer { remote.sort as u32 } else { 0 },
                enabled,
            });
        }
    }
    let seasons = if s.provider == "tmdb" || s.provider == "musicbrainz" {
        s.seasons.clone()
    } else if s.kind == "music" {
        s.episodes
            .iter()
            .map(|ep| ep.season)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|n| Season {
                number: n,
                remote_number: n,
                title: format!("Disc {n:02}"),
                ..Default::default()
            })
            .collect()
    } else {
        vec![Season {
            number: binding.season,
            remote_number: 1,
            title: s.title.clone(),
            remote_title: s.title.clone(),
            info: s.info.clone(),
            ..Default::default()
        }]
    };
    for mut season in seasons {
        season.binding_id = binding.id.clone();
        if let Some(old) = work.seasons.iter_mut().find(|x| {
            (x.binding_id == binding.id && x.remote_number == season.remote_number)
                || (local_bindings.contains(&x.binding_id) && x.number == season.number)
        }) {
            if old.custom_title {
                season.title = old.title.clone();
                season.custom_title = true;
            }
            season.number = old.number;
            let mut ids = old.info.external_ids.clone();
            ids.extend(season.info.external_ids.clone());
            season.info.external_ids = ids;
            *old = season;
        } else {
            work.seasons.push(season);
        }
    }
    if s.provider == "tmdb"
        || (!work.bindings.iter().any(|b| b.provider == "tmdb")
            && work.bindings.first().is_some_and(|b| b.id == binding.id))
    {
        let mut ids = work.info.external_ids.clone();
        ids.extend(s.info.external_ids.clone());
        work.info = s.info.clone();
        work.info.external_ids = ids;
        work.overview = s.overview.clone();
        work.poster = s.poster.clone();
    }
    validate_work(&work)?;
    if group_changed && !new { crate::tmdb_groups::save_reordered_work(store, &work, &binding.id)?; } else { store.put("works", &work.id, &work)?; }
    Ok(work)

    })
}
pub fn validate_work(w: &Work) -> Result<()> {
    if w.title.trim().is_empty() || !["tv", "movie", "music"].contains(&w.kind.as_str()) {
        return Err("请输入有效剧名和类型".into());
    }
    if crate::paths::clean_name(&w.folder_name) != w.folder_name {
        return Err("目录名含有不支持的字符".into());
    }
    if !w.category.is_empty() && crate::paths::clean_name(&w.category) != w.category {
        return Err("分类必须是单个有效目录名".into());
    }
    if !w.tmdb_id.is_empty() && !w.tmdb_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("TMDB ID 必须为数字".into());
    }
    for ep in &w.episodes {
        if ep.enabled && (ep.number == 0 || ep.season > 999 || ep.number > 9999) {
            return Err("已启用集数需要有效的本地季号和集号".into());
        }
    }
    // Multiple metadata sources may describe the same episode. Copy planning detects duplicate files.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires BANGUMI_TMDB_TEST_* environment variables; uses a temporary database and read-only TMDB requests"]
    fn live_tmdb_movie_and_episode_group() {
        let token = std::env::var("BANGUMI_TMDB_TEST_TOKEN").expect("test token is required");
        let query =
            std::env::var("BANGUMI_TMDB_TEST_QUERY").expect("movie search query is required");
        let movie_id = std::env::var("BANGUMI_TMDB_TEST_MOVIE_ID").expect("movie ID is required");
        let series_id =
            std::env::var("BANGUMI_TMDB_TEST_SERIES_ID").expect("series ID is required");
        let group_id =
            std::env::var("BANGUMI_TMDB_TEST_GROUP_ID").expect("episode group ID is required");
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        db.put(
            "settings",
            "main",
            &Settings {
                language: "zh-CN".into(),
                tmdb_token: token,
                ..Default::default()
            },
        )
        .unwrap();
        let results = search(&db, "tmdb", &query, "tv").unwrap();
        assert!(results
            .iter()
            .any(|s| s.id == movie_id && s.kind == "movie"));
        let movie = detail(&db, "tmdb", &movie_id, "tv", false).unwrap();
        assert_eq!(movie.kind, "movie");
        assert_eq!(movie.id, movie_id);
        let group =
            detail_ordered(&db, "tmdb", &series_id, "tv", false, None, Some(&group_id)).unwrap();
        assert_eq!(
            group.episodes.len(),
            group.episode_group.as_ref().unwrap().episode_count as usize
        );
        println!("live TMDB: movie {} ({}) loaded from TV entry; selected group has {} seasons and {} episodes",movie.id,movie.kind,group.seasons.len(),group.episodes.len());
        for stat in db.diagnostics.report().statistics.iter().filter(|s| {
            [
                "provider.detail",
                "provider.search",
                "http.headers",
                "http.body_json",
                "db.lock_wait",
            ]
            .contains(&s.operation.as_str())
        }) {
            println!(
                "{}: calls={}, total_ms={:.1}, max_ms={:.1}",
                stat.operation, stat.count, stat.total_ms, stat.max_ms
            );
        }
    }
    #[test]
    fn movie_search_and_tmdb_links_work_from_the_tv_browser() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        db.put(
            "settings",
            "main",
            &Settings {
                language: "zh-CN".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let mut url = url::Url::parse("https://api.themoviedb.org/3/search/multi").unwrap();
        url.query_pairs_mut()
            .append_pair("query", "虚构河岸之行")
            .append_pair("language", "zh-CN");
        db.cache_put(&format!("{url}:"), &json!({"results":[{"id":910000020,"media_type":"movie","title":"虚构河岸之行","original_title":"虚构河岸之行"},{"id":1,"media_type":"person","name":"unrelated"}]})).unwrap();
        let results = search(&db, "tmdb", "虚构河岸之行", "tv").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, "movie");
        db.cache_put("https://api.themoviedb.org/3/movie/910000020?language=zh-CN:",&json!({"id":910000020,"title":"虚构河岸之行","original_title":"虚构河岸之行","original_language":"zh-CN"})).unwrap();
        let movie = detail(
            &db,
            "tmdb",
            "https://www.themoviedb.org/movie/910000020?language=zh-CN",
            "tv",
            false,
        )
        .unwrap();
        assert_eq!(movie.id, "910000020");
        assert_eq!(movie.kind, "movie");
        assert_eq!(movie.title, "虚构河岸之行");
        assert_eq!(
            parse_reference(" 910000020 ", "movie").unwrap(),
            ("910000020".into(), "movie".into())
        );
        assert!(parse_reference("https://example.com/movie/910000020", "tv").is_err());
    }
    #[test]
    fn metadata_queries_do_not_wait_for_the_organizer_queue_lock() {
        use std::sync::{atomic::Ordering, mpsc};
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        db.put(
            "settings",
            "main",
            &Settings {
                language: "zh-CN".into(),
                ..Default::default()
            },
        )
        .unwrap();
        db.cache_put(
            "https://api.themoviedb.org/3/movie/910000020?language=zh-CN:",
            &json!({"id":910000020,"title":"虚构河岸之行","original_language":"zh-CN"}),
        )
        .unwrap();
        let _queue = db.queue_lock.lock().unwrap();
        db.running.store(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        let cloned = db.clone();
        let thread = std::thread::spawn(move || {
            tx.send(detail(&cloned, "tmdb", "910000020", "movie", false))
                .unwrap()
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(2));
        drop(_queue);
        thread.join().unwrap();
        assert_eq!(result.unwrap().unwrap().id, "910000020");
    }
    #[test]
    fn slow_network_request_does_not_hold_database_connection() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::mpsc,
            time::Duration,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/slow", listener.local_addr().unwrap());
        let (started, rx) = mpsc::channel();
        let (finish, release) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buffer = [0; 2048];
            let _ = s.read(&mut buffer);
            started.send(()).unwrap();
            let _ = release.recv_timeout(Duration::from_secs(4));
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        });
        let client = db.clone();
        let worker = std::thread::spawn(move || request(&client, &url, None, true));
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let (tx, result) = mpsc::channel();
        let local = db.clone();
        let reader = std::thread::spawn(move || tx.send(local.settings()).unwrap());
        let settings = result.recv_timeout(Duration::from_secs(1));
        finish.send(()).unwrap();
        server.join().unwrap();
        worker.join().unwrap().unwrap();
        reader.join().unwrap();
        assert!(settings.unwrap().is_ok());
    }
    fn subject(id: &str, title: &str) -> RemoteSubject {
        RemoteSubject {
            episode_group: None,
            info: MediaInfo::default(),
            seasons: vec![],
            provider: "bangumi".into(),
            id: id.into(),
            kind: "tv".into(),
            title: title.into(),
            original_title: title.into(),
            year: "2012".into(),
            overview: "".into(),
            poster: "".into(),
            episodes: vec![
                RemoteEpisode {
                    info: MediaInfo::default(),
                    id: format!("{id}01"),
                    sort: 1.,
                    title: "第1話".into(),
                    kind: 0,
                    season: 1,
                },
                RemoteEpisode {
                    info: MediaInfo::default(),
                    id: format!("{id}02"),
                    sort: 1.,
                    title: "OVA".into(),
                    kind: 1,
                    season: 1,
                },
            ],
            relations: vec![],
            cached: false,
        }
    }
    #[test]
    fn sequel_mapping_and_refresh_preserve_user_numbering() {
        let d = tempfile::tempdir().unwrap();
        let db = Store::open(d.path().into()).unwrap();
        let request = |work_id, sub, season| BindRequest {
            group_change_confirmed: false,
            work_id,
            subject: sub,
            title: "架空の冒険".into(),
            kind: "tv".into(),
            season,
            tmdb_id: "".into(),
            parent_id: "".into(),
        };
        let w = bind(&db, request(None, subject("1", "架空の冒険"), 1)).unwrap();
        let mut w = bind(&db, request(Some(w.id), subject("2", "架空の冒険W"), 2)).unwrap();
        assert_eq!(w.bindings.len(), 2);
        assert_eq!(w.episodes[2].season, 2);
        assert!(!w.episodes[3].enabled);
        w.episodes[2].number = 9;
        db.put("works", &w.id, &w).unwrap();
        let w = bind(&db, request(Some(w.id), subject("2", "架空の冒険W"), 3)).unwrap();
        assert_eq!(w.episodes[2].season, 2);
        assert_eq!(w.episodes[2].number, 9);
    }
    #[test]
    fn bangumi_pages_and_relation_cache_load_without_network() {
        let d = tempfile::tempdir().unwrap();
        let db = Store::open(d.path().into()).unwrap();
        db.cache_put(
            "https://api.bgm.tv/v0/subjects/42:",
            &json!({"id":42,"type":2,"name":"架空の旅路X","date":"2022-07-05","platform":"TV"}),
        )
        .unwrap();
        for offset in 0..2 {
            db.cache_put(&format!("https://api.bgm.tv/v0/episodes?subject_id=42&limit=100&offset={offset}:"),&json!({"total":2,"data":[{"id":100+offset,"sort":offset+1,"type":0,"name":format!("episode {offset}")}]})).unwrap();
        }
        db.cache_put(
            "https://api.bgm.tv/v0/subjects/42/subjects:",
            &json!([{ "id":12,"type":2,"name":"架空の旅路","relation":"前传" }]),
        )
        .unwrap();
        let s = detail(&db, "bangumi", "42", "tv", false).unwrap();
        assert_eq!(s.episodes.len(), 2);
        assert_eq!(s.relations[0].id, "12");
        let w = bind(
            &db,
            BindRequest {
                group_change_confirmed: false,
                work_id: None,
                subject: s.clone(),
                title: "架空の旅路".into(),
                kind: "tv".into(),
                season: 3,
                tmdb_id: "".into(),
                parent_id: "".into(),
            },
        )
        .unwrap();
        assert!(w.episodes.iter().all(|e| e.season == 3));
        assert!(bind(
            &db,
            BindRequest {
                group_change_confirmed: false,
                work_id: None,
                subject: s,
                title: "duplicate".into(),
                kind: "tv".into(),
                season: 2,
                tmdb_id: "".into(),
                parent_id: "".into()
            }
        )
        .is_err());
    }
    #[test]
    fn network_failure_falls_back_to_persisted_cache() {
        let d = tempfile::tempdir().unwrap();
        let db = Store::open(d.path().into()).unwrap();
        let url = "http://127.0.0.1:0/offline";
        db.cache_put(&format!("{url}:"), &json!({"id":1})).unwrap();
        let (value, stale) = request(&db, url, None, true).unwrap();
        assert!(stale);
        assert_eq!(value["id"], 1);
    }
    #[test]
    fn tmdb_keeps_both_languages_and_refreshes_one_season_without_losing_edits() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        let mut settings = db.settings().unwrap();
        settings.language = "zh-CN".into();
        db.put("settings", "main", &settings).unwrap();
        for (lang, title, ep_title) in [
            ("zh-CN", "中文剧名", "新的旅程"),
            ("ja", "日本語の題", "新しい旅"),
        ] {
            db.cache_put(&format!("https://api.themoviedb.org/3/tv/42?language={lang}:"), &json!({"id":42,"name":title,"original_name":"日本語の題","original_language":"ja","overview":title,"seasons":[{"season_number":1},{"season_number":2}],"genres":[{"id":16}],"origin_country":["JP"]})).unwrap();
            db.cache_put(&format!("https://api.themoviedb.org/3/tv/42/season/2?language={lang}:"), &json!({"name":title,"episodes":[{"id":4201,"episode_number":1,"name":ep_title,"overview":ep_title}]})).unwrap();
        }
        // Season 1 has no cached response: this also proves a season refresh does not fetch it.
        let subject = detail_season(&db, "tmdb", "42", "tv", false, Some(2)).unwrap();
        assert_eq!(subject.seasons.len(), 1);
        assert_eq!(subject.episodes[0].info.original_title, "新しい旅");
        let req = |id| BindRequest {
            group_change_confirmed: false,
            work_id: id,
            subject: subject.clone(),
            kind: "tv".into(),
            season: 99,
            title: "日本語の題".into(),
            tmdb_id: "42".into(),
            parent_id: String::new(),
        };
        let mut work = bind(&db, req(None)).unwrap();
        assert_eq!(work.episodes[0].season, 2);
        assert_eq!(work.info.language, "zh-CN");
        work.seasons[0].title = "第二章：帰還".into();
        work.seasons[0].custom_title = true;
        work.episodes[0].number = 9;
        db.put("works", &work.id, &work).unwrap();
        let work = bind(&db, req(Some(work.id))).unwrap();
        assert_eq!(work.seasons[0].title, "第二章：帰還");
        assert_eq!(work.episodes[0].number, 9);
    }
    #[test]
    fn music_discs_keep_repeated_track_numbers_separate() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        let mut s = subject("99", "Album");
        s.kind = "music".into();
        s.episodes[1].kind = 0;
        s.episodes[1].season = 2;
        let work = bind(
            &db,
            BindRequest {
                group_change_confirmed: false,
                work_id: None,
                subject: s,
                title: "Album".into(),
                kind: "music".into(),
                season: 1,
                tmdb_id: String::new(),
                parent_id: String::new(),
            },
        )
        .unwrap();
        assert_eq!(work.seasons.len(), 2);
        assert_eq!(work.episodes[0].number, 1);
        assert_eq!(work.episodes[1].number, 1);
        assert_eq!(work.episodes[1].season, 2);
    }
    #[test]
    fn tmdb_localized_series_metadata_wins_when_added_after_bangumi() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().into()).unwrap();
        let bgm = subject("44", "Bangumi season");
        let req = |work_id, subject| BindRequest {
            group_change_confirmed: false,
            work_id,
            subject,
            title: "Series".into(),
            kind: "tv".into(),
            season: 1,
            tmdb_id: String::new(),
            parent_id: String::new(),
        };
        let w = bind(&db, req(None, bgm.clone())).unwrap();
        let mut tmdb = subject("123", "Series");
        tmdb.provider = "tmdb".into();
        tmdb.info = MediaInfo {
            language: "zh-TW".into(),
            original_language: "ja".into(),
            localized_title: "中文系列名".into(),
            original_title: "日本語".into(),
            ..Default::default()
        };
        let w = bind(&db, req(Some(w.id), tmdb)).unwrap();
        assert_eq!(w.info.localized_title, "中文系列名");
        let w = bind(&db, req(Some(w.id), bgm)).unwrap();
        assert_eq!(w.info.language, "zh-TW");
        assert_eq!(w.info.original_language, "ja");
    }
}
