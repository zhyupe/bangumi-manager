use crate::{db::Store, model::*, providers};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

const API: &str = "https://musicbrainz.org/ws/2";
const PAGE_SIZE: u32 = 25;
pub const CONTACT: &str = "https://github.com/zhyupe/bangumi-manager";
static REQUEST_GATE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn user_agent(contact: &str) -> Result<String> {
    let contact = if contact.trim().is_empty() {
        CONTACT
    } else {
        contact.trim()
    };
    let web = url::Url::parse(contact).is_ok_and(|u| {
        matches!(u.scheme(), "https" | "http")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    });
    let email = contact
        .split_once('@')
        .is_some_and(|(a, b)| !a.is_empty() && b.contains('.'));
    if contact.len() > 512
        || !contact.is_ascii()
        || contact
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>'))
        || (!web && !email)
    {
        return Err("请在设置中填写 MusicBrainz 联系地址（项目网址或邮箱）".into());
    }
    Ok(format!(
        "BangumiManager/{} ({contact})",
        env!("CARGO_PKG_VERSION")
    ))
}

fn enter_gate(
    gate: &Mutex<Option<Instant>>,
    interval: Duration,
) -> Result<MutexGuard<'_, Option<Instant>>> {
    let mut last = gate.lock().map_err(|e| e.to_string())?;
    if let Some(time) = *last {
        if let Some(wait) = interval.checked_sub(time.elapsed()) {
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
    Ok(last)
}

// Only MusicBrainz HTTP requests share this gate. SQLite, TMDB and cache hits do not wait.
pub fn throttle(store: &Store) -> Result<MutexGuard<'static, Option<Instant>>> {
    store
        .diagnostics
        .measure("musicbrainz.rate_wait", serde_json::json!({}), || {
            enter_gate(&REQUEST_GATE, Duration::from_millis(1100))
        })
}

pub fn mbid(raw: &str) -> Result<String> {
    let id = uuid::Uuid::parse_str(raw).map_err(|_| "MusicBrainz ID 必须是有效的 UUID")?;
    if raw.len() != 36 || id.is_nil() {
        return Err("MusicBrainz ID 必须是有效的 UUID".into());
    }
    Ok(id.hyphenated().to_string())
}

pub fn reference(raw: &str) -> Result<(&'static str, String)> {
    let raw = raw.trim();
    if let Ok(id) = mbid(raw) {
        return Ok(("release", id));
    }
    if let Ok(url) = url::Url::parse(raw) {
        if matches!(url.scheme(), "https" | "http")
            && matches!(
                url.host_str(),
                Some("musicbrainz.org" | "www.musicbrainz.org")
            )
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
        {
            let parts: Vec<_> = url
                .path()
                .trim_end_matches('/')
                .split('/')
                .filter(|p| !p.is_empty())
                .collect();
            if let [entity, id] = parts.as_slice() {
                let entity = match *entity {
                    "release" => "release",
                    "release-group" => "release-group",
                    _ => return Err("请选择发行版本或专辑组链接，不能导入艺术家或单条录音".into()),
                };
                return Ok((entity, mbid(id)?));
            }
        }
    }
    Err("请输入发行版本 MBID，或 MusicBrainz 的 release / release-group 链接".into())
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}
fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or_default()
}
fn artist(v: &Value) -> String {
    array(v, "artist-credit")
        .iter()
        .map(|c| {
            if let Some(s) = c.as_str() {
                return s.to_string();
            }
            let name = text(c, "name");
            format!(
                "{}{}",
                if name.is_empty() {
                    text(&c["artist"], "name")
                } else {
                    name
                },
                text(c, "joinphrase")
            )
        })
        .collect()
}
fn url(entity: &str, id: Option<&str>, pairs: &[(&str, String)]) -> String {
    let path = id.map(|v| format!("/{v}")).unwrap_or_default();
    let mut url = url::Url::parse(&format!("{API}/{entity}{path}")).unwrap();
    url.query_pairs_mut()
        .append_pair("fmt", "json")
        .extend_pairs(pairs.iter().map(|(k, v)| (*k, v)));
    url.to_string()
}
fn detail_url(id: &str) -> String {
    url(
        "release",
        Some(id),
        &[(
            "inc",
            "recordings+artist-credits+release-groups+labels".into(),
        )],
    )
}

pub(crate) fn project(v: &Value, stale: bool) -> Result<RemoteSubject> {
    let id = mbid(&text(v, "id"))?;
    let title = text(v, "title");
    if title.is_empty() {
        return Err("MusicBrainz 发行版本缺少标题".into());
    }
    let date = text(v, "date");
    let country = text(v, "country");
    let mut overview = vec![
        text(v, "disambiguation"),
        text(v, "status"),
        date.clone(),
        country.clone(),
    ];
    let media = array(v, "media");
    if !media.is_empty() {
        overview.push(
            media
                .iter()
                .map(|m| {
                    let position = m["position"].as_u64().unwrap_or(1);
                    let count = m["track-count"].as_u64().or(m["trackCount"].as_u64());
                    format!(
                        "Disc {position:02} · {}{}",
                        text(m, "format"),
                        count.map(|n| format!(" · {n} 轨")).unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join(" / "),
        );
    }
    for label in array(v, "label-info") {
        overview.push(
            [text(&label["label"], "name"), text(label, "catalog-number")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    let barcode = text(v, "barcode");
    if !barcode.is_empty() {
        overview.push(format!("条码 {barcode}"));
    }
    let overview = overview
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let poster = if v["cover-art-archive"]["front"].as_bool() == Some(true) {
        format!("https://coverartarchive.org/release/{id}/front-500")
    } else {
        String::new()
    };
    let mut info = MediaInfo {
        original_title: title.clone(),
        artist: artist(v),
        air_date: date.clone(),
        original_language: text(&v["text-representation"], "language"),
        countries: if country.is_empty() {
            vec![]
        } else {
            vec![country]
        },
        overview: overview.clone(),
        poster: poster.clone(),
        ..Default::default()
    };
    info.external_ids.insert("musicbrainz".into(), id.clone());
    if let Ok(group) = mbid(&text(&v["release-group"], "id")) {
        info.external_ids
            .insert("musicbrainz_releasegroup".into(), group);
    }
    Ok(RemoteSubject {
        provider: "musicbrainz".into(),
        id,
        kind: "music".into(),
        title: title.clone(),
        original_title: title,
        year: date.chars().take(4).collect(),
        overview,
        poster,
        info,
        cached: stale,
        ..Default::default()
    })
}

#[derive(Serialize)]
pub struct ReleasePage {
    pub items: Vec<RemoteSubject>,
    pub total: u64,
    pub next_offset: Option<u32>,
}
pub fn releases(store: &Store, query: &str, offset: u32, refresh: bool) -> Result<ReleasePage> {
    store.diagnostics.measure(
        "musicbrainz.releases",
        serde_json::json!({"offset":offset,"refresh":refresh}),
        || {
            let query = query.trim();
            if query.is_empty() {
                return Ok(ReleasePage {
                    items: vec![],
                    total: 0,
                    next_offset: None,
                });
            }
            if query.len() > 2048 || offset > 10000 {
                return Err("搜索范围过大，请缩小关键词范围".into());
            }
            let is_reference = query.contains("://") || mbid(query).is_ok();
            let mut pairs = vec![
                ("limit", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ];
            let browse = if is_reference {
                let (entity, id) = reference(query)?;
                if entity == "release" {
                    return Ok(ReleasePage {
                        items: vec![detail(store, &id, refresh, None)?],
                        total: 1,
                        next_offset: None,
                    });
                }
                pairs.push(("release-group", id));
                pairs.push(("inc", "artist-credits+labels+media".into()));
                true
            } else {
                pairs.push(("query", query.to_string()));
                false
            };
            let (v, stale) =
                providers::request(store, &url("release", None, &pairs), None, refresh)?;
            let entries = v["releases"]
                .as_array()
                .ok_or("MusicBrainz 返回了无效的发行版本列表")?;
            let total = v[if browse { "release-count" } else { "count" }]
                .as_u64()
                .ok_or("MusicBrainz 返回的版本数量无效")?;
            let items = entries
                .iter()
                .map(|r| project(r, stale))
                .collect::<Result<Vec<_>>>()?;
            let next = offset.saturating_add(entries.len() as u32);
            Ok(ReleasePage {
                items,
                total,
                next_offset: if !entries.is_empty() && u64::from(next) < total {
                    Some(next)
                } else {
                    None
                },
            })
        },
    )
}

pub fn detail(
    store: &Store,
    raw: &str,
    refresh: bool,
    only_disc: Option<u32>,
) -> Result<RemoteSubject> {
    let (entity, id) = reference(raw)?;
    if entity != "release" {
        return Err("请先从专辑组中选择具体发行版本，再导入音轨".into());
    }
    let (v, stale) = providers::request(store, &detail_url(&id), None, refresh)?;
    let mut subject = project(&v, stale)?;
    let media = v["media"]
        .as_array()
        .ok_or("发行版本缺少碟片信息，请刷新后重试")?;
    let mut discs = HashSet::new();
    let mut ids = HashSet::new();
    for m in media {
        let disc = m["position"]
            .as_u64()
            .filter(|n| (1..=999).contains(n))
            .ok_or("发行版本的碟片编号无效")? as u32;
        if !discs.insert(disc) {
            return Err("发行版本包含重复碟片编号".into());
        }
        if only_disc.is_some_and(|n| n != disc) {
            continue;
        }
        let tracks = m["tracks"]
            .as_array()
            .ok_or("发行版本的音轨列表不完整，请刷新后重试")?;
        if m["track-count"].as_u64() != Some(tracks.len() as u64) {
            return Err("发行版本的音轨数量不一致，请刷新后重试".into());
        }
        let name = text(m, "title");
        subject.seasons.push(Season {
            number: disc,
            remote_number: disc,
            title: if name.is_empty() {
                format!("Disc {disc:02}")
            } else {
                name.clone()
            },
            remote_title: name,
            ..Default::default()
        });
        let mut positions = HashSet::new();
        for t in tracks
            .iter()
            .chain(m.get("pregap").filter(|p| p.is_object()))
        {
            let position = t["position"]
                .as_u64()
                .filter(|n| *n < 10000)
                .ok_or("音轨序号无效")? as u32;
            if !positions.insert(position) {
                return Err("同一碟片中存在重复音轨序号".into());
            }
            let track = mbid(&text(t, "id"))?;
            if !ids.insert(track.clone()) {
                return Err("发行版本包含重复的音轨 ID".into());
            }
            let recording = mbid(&text(&t["recording"], "id"))?;
            let title = text(t, "title");
            let title = if title.is_empty() {
                text(&t["recording"], "title")
            } else {
                title
            };
            if title.is_empty() {
                return Err("发行版本中存在缺少标题的音轨".into());
            }
            let mut performer = artist(t);
            if performer.is_empty() {
                performer = artist(&t["recording"]);
            }
            if performer.is_empty() {
                performer = subject.info.artist.clone();
            }
            subject.episodes.push(RemoteEpisode {
                id: track.clone(),
                sort: position as f64,
                title: title.clone(),
                kind: if position == 0 { 1 } else { 0 },
                season: disc,
                info: MediaInfo {
                    duration_ms: t["length"].as_u64().or(t["recording"]["length"].as_u64()),
                    original_title: title,
                    artist: performer,
                    external_ids: [
                        ("musicbrainz".into(), recording),
                        ("musicbrainz_track".into(), track),
                    ]
                    .into(),
                    ..Default::default()
                },
            });
        }
    }
    if subject.episodes.is_empty() {
        return Err("此发行版本没有可导入的音轨，请核对碟片或选择其他版本".into());
    }
    subject
        .episodes
        .sort_by(|a, b| (a.season, a.sort as u32).cmp(&(b.season, b.sort as u32)));
    Ok(subject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::Path, sync::atomic::Ordering};

    const RELEASE: &str = "00000000-0000-4000-8000-000000000001";
    const GROUP: &str = "00000000-0000-4000-8000-000000000002";
    const RECORDING: &str = "00000000-0000-4000-8000-000000000003";

    #[test]
    #[ignore = "Read-only MusicBrainz requests; requires BANGUMI_MUSICBRAINZ_TEST=1 and uses a temporary cache"]
    fn live_musicbrainz_search_release_and_group() {
        assert_eq!(
            std::env::var("BANGUMI_MUSICBRAINZ_TEST").as_deref(),
            Ok("1")
        );
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        let page = releases(&store, "status:official AND tracks:2", 0, false).unwrap();
        let first = page
            .items
            .first()
            .expect("the live search must return a release");
        let subject = detail(&store, &first.id, false, None).unwrap();
        assert!(!subject.episodes.is_empty());
        let group = subject
            .info
            .external_ids
            .get("musicbrainz_releasegroup")
            .unwrap();
        let versions = releases(
            &store,
            &format!("https://musicbrainz.org/release-group/{group}"),
            0,
            false,
        )
        .unwrap();
        assert!(!versions.items.is_empty());
        let before = store
            .diagnostics
            .report()
            .statistics
            .iter()
            .find(|s| s.operation == "http.headers")
            .unwrap()
            .count;
        let cached = detail(&store, &first.id, false, None).unwrap();
        let after = store
            .diagnostics
            .report()
            .statistics
            .iter()
            .find(|s| s.operation == "http.headers")
            .unwrap()
            .count;
        assert_eq!(before, after);
        assert_eq!(subject.episodes.len(), cached.episodes.len());
        println!("MusicBrainz live API: search, release tracks, group editions and cache verified");
    }
    fn track(id: u32, position: u32) -> Value {
        json!({"id":format!("00000000-0000-4000-8000-{id:012}"), "position":position,
            "number":format!("A{position}"), "title":format!("虚构音轨 {id}"),
            "recording":{"id":RECORDING, "title":"虚构录音"}})
    }
    fn release() -> Value {
        json!({"id":RELEASE, "title":"虚构回声档案", "date":"2024-03-01", "country":"JP", "status":"Official",
            "disambiguation":"虚构双碟版本", "barcode":"0000000000000", "release-group":{"id":GROUP},
            "artist-credit":[{"name":"虚构演奏者甲", "joinphrase":" & "},{"artist":{"name":"虚构演奏者乙"}}],
            "cover-art-archive":{"front":false}, "label-info":[{"label":{"name":"虚构厂牌"},"catalog-number":"TEST-001"}],
            "media":[{"position":1,"title":"主碟","format":"CD","track-count":2,"tracks":[track(11,1),track(12,2)],"pregap":track(14,0)},
                {"position":2,"title":"附碟","format":"CD","track-count":1,"tracks":[track(13,1)]}]})
    }
    fn cache(store: &Store, value: &Value) {
        store
            .cache_put(&format!("{}:", detail_url(RELEASE)), value)
            .unwrap();
    }
    fn bind(store: &Store, subject: RemoteSubject, work: Option<&Work>) -> Work {
        providers::bind(
            store,
            BindRequest {
                subject,
                work_id: work.map(|w| w.id.clone()),
                title: "虚构回声档案".into(),
                kind: "music".into(),
                season: 1,
                tmdb_id: String::new(),
                parent_id: String::new(),
                group_change_confirmed: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn references_only_accept_release_ids_and_trusted_links() {
        assert_eq!(reference(RELEASE).unwrap(), ("release", RELEASE.into()));
        assert_eq!(
            reference(&format!(
                "https://musicbrainz.org/release-group/{GROUP}/?x=y"
            ))
            .unwrap(),
            ("release-group", GROUP.into())
        );
        for raw in [
            "1234",
            "00000000-0000-0000-0000-000000000000",
            "https://example.invalid/release/",
            "https://musicbrainz.org/artist/00000000-0000-4000-8000-000000000001",
            "https://musicbrainz.org@evil.invalid/release/00000000-0000-4000-8000-000000000001",
        ] {
            assert!(reference(raw).is_err(), "{raw}");
        }
        assert!(user_agent("").unwrap().contains(CONTACT));
        assert!(user_agent("a@example.invalid\r\nX: bad").is_err());
        assert!(user_agent("https://example.invalid/app")
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION")));
        assert!(user_agent("app@example.invalid").is_ok());
        let id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
        let nfo = crate::nfo::parse(&format!(
            "<album><uniqueid type=\"musicbrainz\">{id}</uniqueid><musicbrainzalbumid>{}</musicbrainzalbumid></album>",
            id.to_uppercase()
        )).unwrap();
        assert_eq!(nfo[0].ids().unwrap()["musicbrainz"], id);
    }

    #[test]
    fn searches_and_group_browsing_page_by_actual_result_count() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        for group in [false, true] {
            let query = if group {
                format!("https://musicbrainz.org/release-group/{GROUP}")
            } else {
                "虚构回声 artist:虚构演奏者".into()
            };
            let mut pairs = vec![("limit", "25".into()), ("offset", "0".into())];
            if group {
                pairs.extend([
                    ("release-group", GROUP.into()),
                    ("inc", "artist-credits+labels+media".into()),
                ]);
            } else {
                pairs.push(("query", query.clone()));
            }
            store
                .cache_put(
                    &format!("{}:", url("release", None, &pairs)),
                    &json!({"releases":[release()], "count":26,"release-count":26}),
                )
                .unwrap();
            let page = releases(&store, &query, 0, false).unwrap();
            assert_eq!(page.total, 26);
            assert_eq!(page.next_offset, Some(1));
            assert_eq!(page.items[0].info.artist, "虚构演奏者甲 & 虚构演奏者乙");
            assert!(page.items[0].overview.contains("TEST-001"));
            pairs[1].1 = "1".into();
            store
                .cache_put(
                    &format!("{}:", url("release", None, &pairs)),
                    &json!({"releases":[], "count":26,"release-count":26}),
                )
                .unwrap();
            assert!(releases(&store, &query, 1, false)
                .unwrap()
                .next_offset
                .is_none());
        }
        assert!(providers::search(&store, "musicbrainz", "query", "tv").is_err());
        assert!(releases(&store, "https://example.invalid/release/x", 0, false).is_err());
    }

    #[test]
    fn multi_disc_tracks_use_track_identity_and_pregap_stays_disabled() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        cache(&store, &release());
        let s = providers::detail(&store, "musicbrainz", RELEASE, "music", false).unwrap();
        assert_eq!(s.seasons.len(), 2);
        assert_eq!(s.seasons[1].title, "附碟");
        assert_eq!(s.episodes.len(), 4);
        assert_eq!(
            s.episodes
                .iter()
                .map(|t| (t.season, t.sort as u32))
                .collect::<Vec<_>>(),
            vec![(1, 0), (1, 1), (1, 2), (2, 1)]
        );
        assert!(s
            .episodes
            .iter()
            .all(|t| t.info.external_ids["musicbrainz"] == RECORDING));
        let w = bind(&store, s, None);
        assert_eq!(w.episodes.iter().filter(|t| t.enabled).count(), 3);
        assert_eq!(
            w.episodes
                .iter()
                .map(|t| &t.remote_id)
                .collect::<HashSet<_>>()
                .len(),
            4
        );
        assert_eq!(w.info.external_ids["musicbrainz"], RELEASE);
        let scoped =
            providers::detail_season(&store, "musicbrainz", RELEASE, "music", false, Some(2))
                .unwrap();
        assert_eq!(scoped.episodes.len(), 1);
        assert_eq!(scoped.seasons.len(), 1);
        let cover = project(
            &json!({"id":RELEASE,"title":"虚构封面样例","cover-art-archive":{"front":true}}),
            false,
        )
        .unwrap();
        assert_eq!(
            cover.poster,
            format!("https://coverartarchive.org/release/{RELEASE}/front-500")
        );
    }

    #[test]
    fn refresh_preserves_manual_disc_track_numbers_and_adds_tracks_to_the_same_disc() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        cache(&store, &release());
        let mut w = bind(&store, detail(&store, RELEASE, false, None).unwrap(), None);
        let old_ids: Vec<_> = w.episodes.iter().map(|e| e.id.clone()).collect();
        for ep in w.episodes.iter_mut().filter(|e| e.season == 2) {
            ep.season = 5;
            ep.number = 9;
        }
        w.seasons[1].number = 5;
        w.seasons[1].title = "手动碟片名".into();
        w.seasons[1].custom_title = true;
        store.put("works", &w.id, &w).unwrap();
        let mut v = release();
        v["media"][1]["tracks"]
            .as_array_mut()
            .unwrap()
            .push(track(15, 2));
        v["media"][1]["track-count"] = json!(2);
        cache(&store, &v);
        let updated = bind(
            &store,
            detail(&store, RELEASE, false, Some(2)).unwrap(),
            Some(&w),
        );
        assert_eq!(updated.bindings.len(), 1);
        assert_eq!(updated.episodes.len(), 5);
        assert!(old_ids
            .iter()
            .all(|id| updated.episodes.iter().any(|e| &e.id == id)));
        assert!(updated
            .episodes
            .iter()
            .any(|e| e.remote_id.ends_with("000000000013") && (e.season, e.number) == (5, 9)));
        assert!(updated
            .episodes
            .iter()
            .any(|e| e.remote_id.ends_with("000000000015") && (e.season, e.number) == (5, 2)));
        assert_eq!(updated.seasons[1].title, "手动碟片名");
    }

    #[test]
    fn incomplete_or_ambiguous_tracklists_cannot_be_imported() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        for case in 0..5 {
            let mut v = release();
            match case {
                0 => v["media"][0]["track-count"] = json!(10),
                1 => v["media"][1]["position"] = json!(1),
                2 => v["media"][0]["tracks"][1]["id"] = v["media"][0]["tracks"][0]["id"].clone(),
                3 => v["media"][0]["tracks"][0]["recording"]["id"] = json!("bad"),
                _ => v["media"][0]["tracks"][1]["position"] = json!(1),
            }
            cache(&store, &v);
            assert!(detail(&store, RELEASE, false, None).is_err(), "case {case}");
        }
        assert!(store.list::<Work>("works").unwrap().is_empty());
    }

    #[test]
    fn rate_gate_serializes_only_musicbrainz_and_cache_hits_bypass_it() {
        let gate = Mutex::new(None);
        let first = enter_gate(&gate, Duration::from_millis(30)).unwrap();
        let began = Instant::now();
        drop(first);
        let second = enter_gate(&gate, Duration::from_millis(30)).unwrap();
        assert!(began.elapsed() >= Duration::from_millis(30));
        drop(second);
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().into()).unwrap();
        cache(&store, &release());
        let _gate = REQUEST_GATE.lock().unwrap();
        store.running.store(true, Ordering::SeqCst);
        assert_eq!(detail(&store, RELEASE, false, None).unwrap().id, RELEASE);
        store.put("sample", "id", &true).unwrap();
        assert!(!store
            .diagnostics
            .report()
            .statistics
            .iter()
            .any(|s| s.operation == "musicbrainz.rate_wait"));
    }

    #[test]
    fn organized_music_nfo_round_trips_release_and_track_identity() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(d.path().join("db")).unwrap();
        let settings = Settings {
            output_root: crate::paths::display(&d.path().join("output")),
            extras_root: crate::paths::display(&d.path().join("extras")),
            backup_root: crate::paths::display(&d.path().join("backups")),
            ..Default::default()
        };
        for kind in ["tv", "movie", "music"] {
            fs::create_dir_all(crate::naming::root(&settings, kind).unwrap()).unwrap();
        }
        for root in [&settings.extras_root, &settings.backup_root] {
            fs::create_dir_all(root).unwrap();
        }
        store.put("settings", "main", &settings).unwrap();
        cache(&store, &release());
        let work = bind(&store, detail(&store, RELEASE, false, None).unwrap(), None);
        let source = d.path().join("input.flac");
        fs::write(&source, b"temporary audio fixture").unwrap();
        let ep = work.episodes.iter().find(|e| e.season == 2).unwrap();
        let plan = crate::copy::create_plan(
            &store,
            vec![Mapping {
                part: 0,
                source: crate::paths::display(&source),
                work_id: work.id.clone(),
                episode_ids: vec![ep.id.clone()],
                extra: false,
            }],
            |_, _| {},
        )
        .unwrap();
        let done = crate::copy::execute(store.clone(), plan.id, |_| {}).unwrap();
        assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
        let target = Path::new(&done.rows[0].target);
        assert_eq!(fs::read(target).unwrap(), b"temporary audio fixture");
        let song = crate::nfo::one(&target.with_extension("nfo"), "song").unwrap();
        assert_eq!(song.ids().unwrap()["musicbrainz"], RECORDING);
        assert_eq!(song.ids().unwrap()["musicbrainz_track"], ep.remote_id);
        let folder = Path::new(&done.rows[0].media_folder);
        let album = crate::nfo::one(&folder.join("album.nfo"), "album").unwrap();
        assert_eq!(album.ids().unwrap()["musicbrainz"], RELEASE);
        let files = vec![
            folder.join("album.nfo"),
            target.to_path_buf(),
            target.with_extension("nfo"),
        ];
        let (imported, _) =
            crate::library_import::load(&store, folder, "music", "", &files, "en", None)
                .unwrap()
                .unwrap();
        assert_eq!(imported.bindings[0].provider, "musicbrainz");
        assert_eq!(imported.episodes[0].remote_id, ep.remote_id);
        assert!(!crate::library_import::same_id(
            &work,
            "musicbrainz_releasegroup",
            GROUP
        ));
        let again = bind(
            &store,
            detail(&store, RELEASE, false, None).unwrap(),
            Some(&work),
        );
        assert_eq!(again.episodes.len(), work.episodes.len());
    }
}
