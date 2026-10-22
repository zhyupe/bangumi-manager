use crate::{db::Store, model::*, paths};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use walkdir::WalkDir;

pub fn scan_sources(
    store: &Store,
    inputs: Vec<String>,
    progress: impl Fn(usize),
) -> Result<ScanResult> {
    store.diagnostics.measure(
        "files.scan",
        serde_json::json!({"roots":inputs.len()}),
        || {
            let settings = store.settings()?;
            let excluded: Vec<_> = [
                settings.output_root,
                settings.music_root,
                settings.tv_root,
                settings.movie_root,
                settings.extras_root,
                settings.backup_root,
            ]
            .iter()
            .filter(|s| !s.is_empty())
            .filter_map(|s| paths::normalize(s).ok())
            .collect();
            let extra = regex::Regex::new(
                r"(?i)(?:^|[\[\]\s_.-])(NCOP|NCED|OP|ED|MENU|PV|CM)\d*(?:[\[\]\s_.-]|$)",
            )
            .unwrap();
            let special =
                regex::Regex::new(r"(?i)(?:^|[\[\]\s_.-])(OVA|OAD|SP|SPECIAL)\d*(?:[\[\]\s_.-]|$)")
                    .unwrap();
            let mut files = BTreeMap::new();
            let mut errors = vec![];
            for input in inputs {
                let root = match paths::normalize(&input)
                    .and_then(|p| fs::canonicalize(p).map_err(|e| e.to_string()))
                {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("{input}：{e}"));
                        continue;
                    }
                };
                let iter = WalkDir::new(&root)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| {
                        if excluded.iter().any(|p| paths::contained(e.path(), p)) {
                            return false;
                        }
                        fs::symlink_metadata(e.path())
                            .map(|m| !paths::is_link(&m))
                            .unwrap_or(true)
                    });
                for entry in iter {
                    let entry = match entry {
                        Ok(v) => v,
                        Err(e) => {
                            errors.push(e.to_string());
                            continue;
                        }
                    };
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let path = entry.path();
                    let signature = match paths::signature(path) {
                        Ok(v) => v,
                        Err(e) => {
                            errors.push(e);
                            continue;
                        }
                    };
                    let name = entry.file_name().to_string_lossy().to_string();
                    let extra_relative_path = crate::extras::relative_path(path).map(|p| paths::display(&p));
                    let mut file = SourceFile {
                        path: paths::display(path),
                        name: name.clone(),
                        parent: paths::display(path.parent().unwrap_or(path)),
                        extension: paths::ext(path),
                        signature,
                        media: paths::media(path),
                        hint: if extra_relative_path.is_some() {
                            "extra"
                        } else if special.is_match(&name) {
                            "special"
                        } else if extra.is_match(&name) {
                            "extra"
                        } else {
                            "episode"
                        }
                        .into(),
                        extra_relative_path,
                        probe: None,
                    };
                    if let Some(old) = store.get::<SourceFile>("sources", &paths::key(path))? {
                        if old.signature == file.signature {
                            file.probe = old.probe.filter(|p| p.version == PROBE_VERSION);
                        }
                    }
                    store.put("sources", &paths::key(path), &file)?;
                    files.insert(paths::key(path), file);
                    if files.len() % 100 == 0 {
                        progress(files.len());
                    }
                }
            }
            progress(files.len());
            Ok(ScanResult {
                files: files.into_values().collect(),
                errors,
            })
        },
    )
}
pub const PROBE_VERSION: u32 = 3;
pub fn probe_key(store: &Store, path: &Path, sig: &Signature) -> Result<String> {
    Ok(format!(
        "{}:{}:{}:{}:v{}",
        paths::key(path),
        sig.size,
        sig.modified,
        store.settings()?.ffprobe,
        PROBE_VERSION
    ))
}
pub fn cached_probe(store: &Store, raw: &str) -> Result<Option<Probe>> {
    let path = paths::normalize(raw)?;
    let key = probe_key(store, &path, &paths::signature(&path)?)?;
    Ok(store
        .get("probe_errors", &key)?
        .or(store.get("probes", &key)?))
}
pub fn probe(store: &Store, raw: &str) -> Result<Probe> {
    probe_controlled(store, raw, false, None)
}
pub fn probe_controlled(
    store: &Store,
    raw: &str,
    force: bool,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Probe> {
    store.diagnostics.measure_with(
        "media.probe",
        serde_json::json!({"force":force}),
        |timing| {
            let path = paths::normalize(raw)?;
            let sig = paths::signature(&path)?;
            let cachekey = probe_key(store, &path, &sig)?;
            if !force {
                if let Some(p) = store.get("probes", &cachekey)? {
                    timing.field("cache", "fresh_hit");
                    return Ok(p);
                }
            }
            timing.field("cache", "miss");
            let mut cmd = Command::new(store.settings()?.ffprobe);
            cmd.args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let result = (|| -> Result<Probe> {
                let mut child = cmd.spawn().map_err(|e| format!("ffprobe 无法启动：{e}"))?;
                let mut stdout = child.stdout.take().ok_or("没有 ffprobe 输出")?;
                let reader = thread::spawn(move || {
                    let mut data = Vec::new();
                    let _ = (&mut stdout).take(1024 * 1024).read_to_end(&mut data);
                    data
                });
                let start = Instant::now();
                loop {
                    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        || start.elapsed() > Duration::from_secs(20)
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = reader.join();
                        return Err("媒体探测已取消或超时，可重试".into());
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            if !status.success() {
                                let _ = reader.join();
                                return Err("ffprobe 无法读取此文件".into());
                            }
                            break;
                        }
                        Ok(None) => thread::sleep(Duration::from_millis(50)),
                        Err(e) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            let _ = reader.join();
                            return Err(e.to_string());
                        }
                    }
                }
                let output = reader.join().map_err(|_| "ffprobe 读取失败")?;
                let value = serde_json::from_slice(&output).map_err(|e| e.to_string())?;
                if paths::signature(&path)? != sig {
                    return Err("文件在探测期间发生变化，请重试".into());
                }
                parse_probe(&value, sig.size)
            })();
            let p = result.unwrap_or_else(|e| Probe {
                version: PROBE_VERSION,
                error: Some(e),
                ..Default::default()
            });
            if p.error.is_none() {
                store.put("probes", &cachekey, &p)?;
                store.remove("probe_errors", &cachekey)?;
            } else if !cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
                store.put("probe_errors", &cachekey, &p)?;
            }
            if let Some(mut source) = store.get::<SourceFile>("sources", &paths::key(&path))? {
                source.probe = Some(p.clone());
                store.put("sources", &paths::key(&path), &source)?;
            }
            timing.finish(
                if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
                    "cancelled"
                } else if p.error.is_some() {
                    "error"
                } else {
                    "ok"
                },
            );
            Ok(p)
        },
    )
}
fn number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .filter(|n| n.is_finite() && *n > 0.0)
}
fn string(v: &serde_json::Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(String::from)
}
pub fn parse_probe(v: &serde_json::Value, size: u64) -> Result<Probe> {
    let mut p = Probe {
        version: PROBE_VERSION,
        container: string(&v["format"]["format_name"]),
        duration: number(&v["format"]["duration"]),
        bitrate: number(&v["format"]["bit_rate"]).map(|n| n as u64),
        ..Default::default()
    };
    for stream in v["streams"].as_array().unwrap_or(&vec![]) {
        let track = MediaTrack {
            codec: stream["codec_name"].as_str().unwrap_or("?").into(),
            language: stream["tags"]["language"].as_str().unwrap_or("und").into(),
            title: stream["tags"]["title"].as_str().unwrap_or("").into(),
            channels: stream["channels"].as_u64(),
        };
        match stream["codec_type"].as_str() {
            Some("video")
                if p.width.is_none()
                    && stream["disposition"]["attached_pic"].as_u64() != Some(1) =>
            {
                p.width = stream["width"].as_u64();
                p.height = stream["height"].as_u64();
                p.video_codec = string(&stream["codec_name"]);
                p.video_bitrate = number(&stream["bit_rate"]).map(|n| n as u64);
                p.profile = string(&stream["profile"]);
                p.pixel_format = string(&stream["pix_fmt"]);
                p.bit_depth = number(&stream["bits_per_raw_sample"])
                    .map(|n| n as u64)
                    .or_else(|| {
                        let fmt = p.pixel_format.as_deref()?;
                        for depth in [16, 14, 12, 10, 9] {
                            if fmt.contains(&format!("p{depth}")) {
                                return Some(depth);
                            }
                        }
                        if fmt.starts_with("yuv") || fmt == "nv12" {
                            Some(8)
                        } else {
                            None
                        }
                    });
                p.frame_rate = stream["avg_frame_rate"].as_str().and_then(|s| {
                    let (a, b) = s.split_once('/')?;
                    let a: f64 = a.parse().ok()?;
                    let b: f64 = b.parse().ok()?;
                    (a > 0.0 && b > 0.0).then_some(a / b)
                });
                p.color_space = string(&stream["color_space"]);
                p.color_transfer = string(&stream["color_transfer"]);
                p.hdr = match p.color_transfer.as_deref() {
                    Some("smpte2084") => Some("HDR / PQ".into()),
                    Some("arib-std-b67") => Some("HLG".into()),
                    _ => None,
                };
                if stream["side_data_list"].as_array().is_some_and(|items| {
                    items.iter().any(|x| {
                        x["side_data_type"]
                            .as_str()
                            .is_some_and(|s| s.contains("DOVI"))
                    })
                }) {
                    p.hdr = Some("Dolby Vision".into());
                }
            }
            Some("audio") => {
                if p.audio_tracks.is_empty() {
                    p.sample_rate = stream["sample_rate"].as_str().and_then(|v| v.parse().ok());
                    p.audio_bit_depth = number(&stream["bits_per_raw_sample"]).or_else(|| number(&stream["bits_per_sample"])).map(|v| v as u32);
                    p.sample_count = stream["duration_ts"].as_u64().zip(stream["time_base"].as_str()).zip(p.sample_rate).and_then(|((duration, timebase), rate)| {
                        let (a,b) = timebase.split_once('/')?;
                        let (a,b) = (a.parse::<u64>().ok()?, b.parse::<u64>().ok()?);
                        if b == 0 { return None; }
                        u64::try_from((duration as u128).checked_mul(a as u128)?.checked_mul(rate as u128)? / b as u128).ok()
                    });
                }
                p.audio
                    .push(format!("{} / {}", track.codec, track.language));
                p.audio_tracks.push(track);
            }
            Some("subtitle") => p.subtitles.push(track),
            _ => (),
        }
    }
    if p.bitrate.is_none() {
        if let Some(d) = p.duration {
            p.bitrate = Some((size as f64 * 8.0 / d) as u64);
            p.bitrate_estimated = true;
        }
    }
    if p.video_codec.is_none() && p.audio_tracks.is_empty() {
        return Err("文件没有可识别的音视频轨道".into());
    }
    Ok(p)
}

fn collect_resource(out: &mut LibraryIndex, path: &Path, kind: &str) -> bool {
    if !paths::media(path) {
        return true;
    }
    match paths::signature(path) {
        Ok(signature) => {
            out.resources.push(ResourceFile {
                unavailable: false,
                path: paths::display(path),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                extension: paths::ext(path),
                kind: if paths::audio(path) {
                    "music"
                } else if kind == "movie" {
                    "movie"
                } else {
                    "tv"
                }
                .into(),
                signature,
                asset: None,
                probe: None,
                available: true,
            });
            true
        }
        Err(e) => {
            out.errors.push(e);
            false
        }
    }
}

mod related;
pub(crate) use related::index_works;

fn index_folder(
    store: &Store,
    settings: &Settings,
    dir: &Path,
    kind: &str,
    nested: bool,
    expected_work: Option<&str>,
    out: &mut LibraryIndex,
    root_assets: &mut Vec<Asset>,
) -> Result<bool> {
    let pattern = regex::Regex::new(r"(?i)S(\d{1,3})E(\d{1,4})(?:-E?(\d{1,4}))?").unwrap();
    let mut root_failed = false;
    let mut walk = store.diagnostics.span(
        "library.scan.walk_folder",
        serde_json::json!({"kind": kind}),
    );
    let mut files = vec![];
    let mut folder_failed = false;
    for entry in WalkDir::new(&dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            fs::symlink_metadata(e.path())
                .map(|m| !paths::is_link(&m) || m.file_type().is_symlink())
                .unwrap_or(true)
        })
    {
        match entry {
            Ok(entry) if entry.file_type().is_file() || entry.file_type().is_symlink() => {
                files.push(entry.into_path())
            }
            Err(e) => {
                folder_failed = true;
                root_failed = true;
                out.errors.push(e.to_string());
            }
            _ => {}
        }
    }
    files.sort();
    walk.field("files", files.len());
    walk.finish(if folder_failed { "error" } else { "ok" });
    let category = if nested {
        dir.parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let mut reason = String::new();
    let imported = if folder_failed {
        None
    } else {
        let result = if expected_work.is_some() {
            crate::library_import::inspect_folder(
                store,
                dir,
                kind,
                &category,
                &files,
                &settings.language,
                expected_work,
            )
        } else {
            crate::library_import::import_folder(
                store,
                dir,
                kind,
                &category,
                &files,
                &settings.language,
            )
        };
        match result {
            Ok(result) => result,
            Err(e) => {
                reason = e;
                out.errors.push(format!("{}：{}", dir.display(), reason));
                None
            }
        }
    };
    let binding = imported.as_ref().map(|i| &i.work_id);
    if let Some(imported) = &imported {
        out.imported += usize::from(imported.imported);
        out.updated += usize::from(imported.updated);
    }
    let mut indexing = store.diagnostics.span(
        "library.scan.index_files",
        serde_json::json!({"kind": kind, "files": files.len()}),
    );
    let indexing_scope = indexing.enter();
    let previous_errors = out.errors.len();
    let mut count = 0;
    for path in &files {
        root_failed |= !collect_resource(out, path, kind);
        if !(if kind == "music" {
            paths::audio(path)
        } else {
            paths::video(path)
        }) {
            continue;
        }
        count += 1;
        if let Some(work_id) = binding {
            let signature = match paths::signature(path) {
                Ok(s) => s,
                Err(e) => {
                    root_failed = true;
                    out.errors.push(e);
                    continue;
                }
            };
            let saved: Option<(u32, u32, u32)> = store.get("asset_identity", &paths::key(path))?;
            let from_nfo = imported
                .as_ref()
                .and_then(|i| i.numbers.get(&paths::key(path)).copied());
            let numbers = saved.or(from_nfo).or_else(|| {
                if kind == "movie" {
                    Some((1, 1, 1))
                } else if kind == "music" {
                    regex::Regex::new(r"^(\d+)")
                        .unwrap()
                        .captures(&path.file_name()?.to_string_lossy())
                        .and_then(|c| c[1].parse().ok())
                        .map(|n| (1, n, n))
                } else {
                    pattern
                        .captures(&path.file_name()?.to_string_lossy())
                        .map(|c| {
                            let n = c[2].parse().unwrap_or(0);
                            (
                                c[1].parse().unwrap_or(0),
                                n,
                                c.get(3).and_then(|v| v.as_str().parse().ok()).unwrap_or(n),
                            )
                        })
                }
            });
            let mut part = if kind == "music" {
                0
            } else {
                crate::multipart::number(path)
            };
            let numbers = match crate::tmdb_groups::asset_numbers(store, path, work_id, &signature)
            {
                Ok(Some((season, number, end, saved_part))) => {
                    part = saved_part;
                    Some((season, number, end))
                }
                Ok(None) => numbers,
                Err(e) => {
                    out.errors.push(format!("{}：{e}", path.display()));
                    None
                }
            };
            if let Some((season, number, end)) = numbers {
                root_assets.push(Asset {
                    part,
                    path: paths::display(path),
                    work_id: work_id.clone(),
                    season,
                    number,
                    end,
                    signature,
                });
            }
        }
    }
    if binding.is_none() && (count > 0 || !reason.is_empty()) {
        out.unbound.push(UnboundFolder {
            path: paths::display(&dir),
            name: dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            kind: kind.into(),
            files: count,
            reason,
        });
    }
    indexing.field("media", count);
    indexing.finish(if out.errors.len() == previous_errors {
        "ok"
    } else {
        "error"
    });
    drop(indexing_scope);
    Ok(!root_failed)
}

pub fn index_library(store: &Store) -> Result<LibraryIndex> {
    if store.list::<Plan>("plans")?.iter().any(|p| crate::renaming::in_progress(p) && p.rows.iter().any(|r| r.state != "complete")) {
        return Err("重命名尚未完成，请继续任务后再扫描资料库".into());
    }
    store
        .diagnostics
        .measure_with("library.scan", serde_json::json!({}), |span| {
            let _scan = store.diagnostics.measure(
                "library.scan.lock_wait",
                serde_json::json!({}),
                || store.library_scan.lock().map_err(|e| e.to_string()),
            )?;
            let settings = store.settings()?;
            if settings.output_root.is_empty()
                && settings.music_root.is_empty()
                && settings.tv_root.is_empty()
                && settings.movie_root.is_empty()
            {
                return Err("请先在设置中注册至少一个媒体库目录".into());
            }
            let mut out = LibraryIndex {
                scanned_at: now(),
                available: true,
                ..Default::default()
            };
            let previous = store
                .get::<LibraryIndex>("index", "main")?
                .unwrap_or_default();
            let mut seen = HashSet::new();
            let mut uncertain_roots = Vec::new();
            let mut roots: Vec<(String, &str, bool)> = vec![];
            for (raw, kind) in [
                (&settings.tv_root, "tv"),
                (&settings.movie_root, "movie"),
                (&settings.music_root, "music"),
            ] {
                if !raw.is_empty() {
                    roots.push((raw.clone(), kind, kind == "music"));
                }
                if !settings.output_root.is_empty() {
                    let root = paths::display(&crate::naming::root(&settings, kind)?);
                    let key = paths::key(Path::new(&root));
                    if let Some(existing) = roots.iter_mut().find(|(r, k, _)| {
                        *k == kind && paths::normalize(r).is_ok_and(|p| paths::key(&p) == key)
                    }) {
                        // The categorized output layout takes precedence over a duplicate legacy root.
                        existing.2 = true;
                    } else {
                        roots.push((root, kind, true));
                    }
                }
            }
            span.field("roots", roots.len());
            let mut collection = store.diagnostics.span(
                "library.scan.collect",
                serde_json::json!({"roots": roots.len()}),
            );
            let collection_scope = collection.enter();
            let mut folders = 0usize;
            for (raw, kind, nested) in &roots {
                let kind = *kind;
                if raw.is_empty() {
                    continue;
                }
                let root = paths::normalize(raw)?;
                let entries = match fs::read_dir(&root) {
                    Ok(v) => v,
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::NotFound
                            && !previous
                                .resources
                                .iter()
                                .any(|r| paths::contained(Path::new(&r.path), &root))
                            && !previous
                                .assets
                                .iter()
                                .any(|a| paths::contained(Path::new(&a.path), &root))
                        {
                            continue;
                        }
                        out.available = false;
                        uncertain_roots.push(root.clone());
                        out.errors.push(format!("{}：{}", raw, e));
                        out.assets.extend(
                            previous
                                .assets
                                .iter()
                                .filter(|a| paths::contained(Path::new(&a.path), &root))
                                .cloned(),
                        );
                        continue;
                    }
                };
                let mut root_assets = vec![];
                let mut root_failed = false;
                drop(entries);
                let depth = if *nested { 2 } else { 1 };
                for folder in WalkDir::new(&root)
                    .min_depth(1)
                    .max_depth(depth)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| {
                        fs::symlink_metadata(e.path())
                            .map(|m| !paths::is_link(&m))
                            .unwrap_or(true)
                    })
                {
                    let folder = match folder {
                        Ok(f) => f,
                        Err(e) => {
                            root_failed = true;
                            out.errors.push(e.to_string());
                            continue;
                        }
                    };
                    let dir = folder.path().to_path_buf();
                    if folder.file_type().is_file() {
                        root_failed |= !collect_resource(&mut out, &dir, kind);
                        continue;
                    }
                    if folder.depth() != depth {
                        continue;
                    }
                    let m = match fs::symlink_metadata(&dir) {
                        Ok(m) => m,
                        Err(e) => {
                            root_failed = true;
                            out.errors.push(e.to_string());
                            continue;
                        }
                    };
                    if !m.is_dir() || paths::is_link(&m) {
                        continue;
                    }
                    folders += 1;
                    root_failed |= !index_folder(
                        store, &settings, &dir, kind, *nested, None, &mut out, &mut root_assets,
                    )?;
                }
                if root_failed {
                    out.available = false;
                    uncertain_roots.push(root.clone());
                    root_assets.extend(
                        previous
                            .assets
                            .iter()
                            .filter(|a| paths::contained(Path::new(&a.path), &root))
                            .cloned(),
                    );
                }
                for asset in root_assets {
                    if seen.insert(paths::key(Path::new(&asset.path))) {
                        out.assets.push(asset);
                    }
                }
            }
            collection.field("folders", folders);
            collection.field("resources", out.resources.len());
            collection.field("assets", out.assets.len());
            collection.finish(if out.available { "ok" } else { "error" });
            drop(collection_scope);
            let mut association = store.diagnostics.span(
                "library.scan.link_resources",
                serde_json::json!({"resources": out.resources.len(), "assets": out.assets.len()}),
            );
            let mut resource_keys = HashSet::new();
            out.resources
                .retain(|r| resource_keys.insert(paths::key(Path::new(&r.path))));
            if !out.available {
                for resource in &previous.resources {
                    if uncertain_roots
                        .iter()
                        .any(|root| paths::contained(Path::new(&resource.path), root))
                        && resource_keys.insert(paths::key(Path::new(&resource.path)))
                    {
                        let mut old = resource.clone();
                        old.available = false;
                        out.resources.push(old);
                    }
                }
            }
            let mut assets_by_path = HashMap::with_capacity(out.assets.len());
            for asset in &out.assets {
                assets_by_path
                    .entry(paths::key(Path::new(&asset.path)))
                    .or_insert(asset);
            }
            for resource in &mut out.resources {
                resource.asset = assets_by_path
                    .get(&paths::key(Path::new(&resource.path)))
                    .map(|asset| (*asset).clone());
            }
            association.finish("ok");
            store.diagnostics.measure(
                "library.scan.persist",
                serde_json::json!({}),
                || store.put("index", "main", &out),
            )?;
            span.field("folders", folders);
            span.field("resources", out.resources.len());
            span.field("assets", out.assets.len());
            span.field("errors", out.errors.len());
            span.field("available", out.available);
            Ok(out)
        })
}

pub fn bind_folder(store: &Store, raw: &str, id: &str) -> Result<()> {
    store.diagnostics.measure(
        "library.bind_folder",
        serde_json::json!({"work_id":id}),
        || {
            let mut work = store.get::<Work>("works", id)?.ok_or("剧集不存在")?;
            let s = store.settings()?;
            let folder = paths::normalize(raw)?;
            let new_root = crate::naming::root(&s, &work.kind)?;
            let old_root = paths::normalize(match work.kind.as_str() {
                "movie" => &s.movie_root,
                "music" => &s.music_root,
                _ => &s.tv_root,
            })
            .ok();
            let root = if paths::contained(&folder, &new_root) {
                new_root.clone()
            } else {
                old_root
                    .filter(|r| paths::contained(&folder, r))
                    .ok_or("目录不在已注册的资料库内")?
            };
            paths::safe_target(&folder, &root)?;
            let depth = if !s.output_root.is_empty() && paths::key(&root) == paths::key(&new_root)
                || work.kind == "music"
            {
                2
            } else {
                1
            };
            if folder
                .strip_prefix(&root)
                .map(|p| p.components().count())
                .unwrap_or(0)
                != depth
            {
                return Err("请选择分类下的作品目录".into());
            }
            let name = folder
                .file_name()
                .ok_or("无效目录")?
                .to_string_lossy()
                .to_string();
            if paths::clean_name(&name) != name {
                return Err("此目录名不符合安全命名要求，请先在文件管理器修正".into());
            }
            work.folder_name = name;
            store.put("works", id, &work)?;
            store.put("folders", &paths::key(&folder), &id)
        },
    )
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    #[ignore = "Requires SCAN_PROFILE_DATABASE pointing to a consistent SQLite snapshot; reads its library roots"]
    fn profile_library_snapshot() {
        let snapshot = std::env::var_os("SCAN_PROFILE_DATABASE").expect("SCAN_PROFILE_DATABASE");
        let dir = tempfile::tempdir().unwrap();
        fs::copy(snapshot, dir.path().join("catalog.sqlite3")).unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let before: BTreeMap<_, _> = store.diagnostics.report().statistics.into_iter()
            .map(|s| (s.operation.clone(), s))
            .collect();
        let index = index_library(&store).unwrap();
        let mut statistics = store.diagnostics.report().statistics;
        for stat in &mut statistics {
            if let Some(old) = before.get(&stat.operation) {
                stat.count -= old.count;
                stat.total_ms -= old.total_ms;
            }
        }
        statistics.retain(|s| s.count > 0);
        statistics.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
        println!("{}", serde_json::json!({
            "available": index.available, "resources": index.resources.len(),
            "assets": index.assets.len(), "errors": index.errors.len(),
            "statistics": statistics,
        }));
        assert!(
            index.available && index.errors.is_empty(),
            "Library profile requires all roots to be available and readable",
        );
    }
}
