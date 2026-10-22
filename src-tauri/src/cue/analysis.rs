use super::{parser::*, process};
use crate::{copy, db::Store, model::*, paths, resources};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyzeRequest {
    pub path: String,
    pub encoding: String,
    pub files: BTreeMap<String, String>,
    pub selected_paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    pub file: VerifiedFile,
    pub codec: String,
    pub rate: u32,
    pub channels: u32,
    pub bits: u32,
    pub samples: u64,
    pub lossy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub source: usize,
    pub start: u64,
    pub end: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrack {
    pub number: u32,
    pub title: String,
    pub performer: String,
    pub isrc: String,
    pub segments: Vec<Segment>,
    pub duration: f64,
    pub extension: String,
    pub cut: Option<super::transfer::Cut>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub id: String,
    pub request: AnalyzeRequest,
    pub cue: VerifiedFile,
    pub sheet: CueSheet,
    pub sources: Vec<AudioSource>,
    pub tracks: Vec<AudioTrack>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CueReference {
    pub name: String,
    pub path: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CueInspection {
    #[serde(flatten)]
    pub sheet: CueSheet,
    pub references: Vec<CueReference>,
}
fn resolve_source(request: &AnalyzeRequest, parent: &Path, name: &str) -> Result<std::path::PathBuf> {
    if let Some(raw) = request.files.get(name) {
        let path = paths::normalize(raw)?;
        resources::regular(&path, None)?;
        return Ok(path);
    }
    let relative = Path::new(name);
    if relative.is_absolute() || name.contains(':') || name.starts_with(['/', '\\'])
        || relative.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(format!("Choose the referenced audio file explicitly: {name}"));
    }
    let path = parent.join(relative);
    if resources::regular(&path, Some(parent)).is_ok() { return Ok(path); }
    let mut matches = BTreeMap::new();
    for raw in &request.selected_paths {
        let path = paths::normalize(raw)?;
        if path.file_name().zip(relative.file_name()).is_some_and(|(a,b)| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())) {
            resources::regular(&path, None)?;
            matches.insert(paths::key(&path), path);
        }
    }
    if matches.len() == 1 { return Ok(matches.into_values().next().unwrap()); }
    Err(format!("Referenced audio missing or ambiguous; select manually: {name}"))
}
pub fn inspect(request: &AnalyzeRequest) -> Result<CueInspection> {
    let path = paths::normalize(&request.path)?;
    if resources::regular(&path, None)?.size > 4 * 1024 * 1024 {
        return Err("CUE exceeds 4 MiB".into());
    }
    let sheet = parse(&decode(&fs::read(&path).map_err(|e| e.to_string())?, &request.encoding)?)?;
    let parent = path.parent().ok_or("CUE parent missing")?;
    let references = sheet.files.iter().map(|name| {
        match resolve_source(request, parent, name) {
            Ok(path) => CueReference { name: name.clone(), path: Some(paths::display(&path)), error: None },
            Err(error) => CueReference { name: name.clone(), path: None, error: Some(error) },
        }
    }).collect();
    Ok(CueInspection { sheet, references })
}
fn effective_bits(stream: &serde_json::Value, codec: &str) -> Option<u32> {
    let positive = |value: &serde_json::Value| {
        value.as_u64().and_then(|v| u32::try_from(v).ok())
            .or_else(|| value.as_str().and_then(|v| v.parse().ok())).filter(|v| *v > 0)
    };
    positive(&stream["bits_per_raw_sample"])
        .or_else(|| positive(&stream["bits_per_sample"]))
        .or_else(|| match codec {
            "pcm_s16le" | "pcm_s16be" => Some(16),
            "pcm_s24le" | "pcm_s24be" => Some(24),
            "pcm_s32le" | "pcm_s32be" => Some(32),
            // A 32-bit decoder buffer may contain 24-bit audio; never infer 32 from it.
            "flac" | "ape" | "alac" if matches!(stream["sample_fmt"].as_str(), Some("s16" | "s16p")) => Some(16),
            _ => None,
        })
}
pub fn verify(store: &Store, file: &VerifiedFile) -> Result<()> {
    let path = Path::new(&file.path);
    if resources::regular(path, None)? != file.signature
        || copy::measured_digest(store, path)? != file.digest
    {
        return Err(format!("Source changed; analyze CUE again: {}", file.path));
    }
    Ok(())
}
pub(super) fn flac_header_bits(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut header = [0u8; 42];
    fs::File::open(path).ok()?.read_exact(&mut header).ok()?;
    if &header[..4] != b"fLaC" || header[4] & 0x7f != 0 || header[5..8] != [0, 0, 34] {
        return None;
    }
    Some((((header[20] & 1) as u32) << 4 | (header[21] >> 4) as u32) + 1)
}
pub fn audio(store: &Store, path: &Path) -> Result<AudioSource> {
    let _input = crate::remote_transfer::LockedFile::open(path, false)?;
    let signature = resources::regular(path, None)?;
    let args = ["-v", "error", "-show_streams", "-of", "json"]
        .map(String::from)
        .into_iter()
        .chain([paths::display(path)])
        .collect::<Vec<_>>();
    let output = process::run(
        store,
        &store.settings()?.ffprobe,
        &args,
        Some(&store.cue_cancel),
        false,
        "cue.probe",
    )?;
    let json: serde_json::Value =
        serde_json::from_slice(&output.bytes).map_err(|e| e.to_string())?;
    let streams = json["streams"].as_array().ok_or("Audio streams missing")?;
    let streams: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    if streams.len() != 1 {
        return Err("CUE requires exactly one audio stream per source".into());
    }
    let stream = streams[0];
    let codec = stream["codec_name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let lossy = matches!(codec.as_str(), "mp3" | "aac");
    if !lossy
        && !matches!(
            codec.as_str(),
            "flac"
                | "ape"
                | "alac"
                | "pcm_s16le"
                | "pcm_s16be"
                | "pcm_s24le"
                | "pcm_s24be"
                | "pcm_s32le"
                | "pcm_s32be"
        )
    {
        return Err(format!("Unsupported CUE audio codec: {codec}"));
    }
    let rate = stream["sample_rate"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .ok_or("Audio sample rate missing")?;
    let channels = stream["channels"]
        .as_u64()
        .filter(|v| (1..=8).contains(v))
        .ok_or("Unsupported audio channel count")? as u32;
    let bits = if codec == "flac" {
        flac_header_bits(path).or_else(|| effective_bits(stream, &codec))
    } else {
        effective_bits(stream, &codec)
    }.unwrap_or(0);
    if !lossy && ![16, 24, 32].contains(&bits) {
        return Err("Cannot preserve this source's effective bit depth".into());
    }
    let decoded = process::run(
        store,
        &process::ffmpeg(store)?,
        &[
            "-v",
            "error",
            "-xerror",
            "-i",
            &paths::display(path),
            "-map",
            "0:a:0",
            "-c:a",
            "pcm_s32le",
            "-f",
            "s32le",
            "-",
        ]
        .map(String::from),
        Some(&store.cue_cancel),
        true,
        "cue.sample_count",
    )?;
    if decoded.count == 0 || decoded.count % (channels as u64 * 4) != 0 {
        return Err("Invalid decoded audio sample count".into());
    }
    let file = VerifiedFile {
        path: paths::display(path),
        signature,
        digest: copy::measured_digest(store, path)?,
    };
    if resources::regular(path, None)? != file.signature {
        return Err("Audio source changed during analysis".into());
    }
    Ok(AudioSource {
        file,
        codec,
        rate,
        channels,
        bits,
        samples: decoded.count / (channels as u64 * 4),
        lossy,
    })
}
pub fn analyze(store: &Store, request: AnalyzeRequest) -> Result<Analysis> {
    use std::sync::atomic::Ordering;
    if store.cue_running.swap(true, Ordering::SeqCst) {
        return Err("A CUE analysis is already running".into());
    }
    struct Reset<'a>(&'a Store);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.cue_running.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(store);
    store.cue_cancel.store(false, Ordering::SeqCst);
    store.diagnostics.measure("cue.analyze", serde_json::json!({}), || {
        let cue_path = paths::normalize(&request.path)?;
        let _input=crate::remote_transfer::LockedFile::open(&cue_path,false)?;
        let signature = resources::regular(&cue_path, None)?;
        if signature.size > 4 * 1024 * 1024 { return Err("CUE exceeds 4 MiB".into()); }
        let sheet = parse(&decode(&fs::read(&cue_path).map_err(|e| e.to_string())?, &request.encoding)?)?;
        let cue = VerifiedFile { path: paths::display(&cue_path), signature, digest: copy::measured_digest(store, &cue_path)? };
        let parent = cue_path.parent().ok_or("CUE parent missing")?;
        let mut sources = vec![];
        for name in &sheet.files {
            if store.cue_cancel.load(Ordering::SeqCst) {return Err("CUE analysis cancelled".into());}
            let path = resolve_source(&request, parent, name)?;
            if sources.iter().any(|s: &AudioSource| paths::key(Path::new(&s.file.path))==paths::key(&path)) {return Err("Multiple CUE references resolve to the same audio file".into());}
            sources.push(audio(store, &path)?);
        }
        let first = &sources[0];
        if sources.iter().any(|s| s.rate != first.rate || s.channels != first.channels || s.bits != first.bits || s.lossy != first.lossy || (s.lossy && s.codec != first.codec)) {
            return Err("Multi-file CUE sources must use matching audio parameters".into());
        }
        let mut offsets = vec![0u64];
        for source in &sources { offsets.push(offsets.last().unwrap().checked_add(source.samples).ok_or("Audio length overflow")?); }
        let starts = sheet.tracks.iter().map(|track| {
            let offset = sample_at(track.index01.unwrap(), first.rate)?;
            if offset >= sources[track.file].samples { return Err("CUE index exceeds source length".to_string()); }
            Ok(offsets[track.file] + offset)
        }).collect::<Result<Vec<_>>>()?;
        let mut tracks = vec![];
        let mut warnings = sheet.warnings.clone();
        if starts[0] > 0 { warnings.push("Audio before the first track is retained at its beginning".into()); }
        for (i, track) in sheet.tracks.iter().enumerate() {
            let start = if i == 0 { 0 } else { starts[i] };
            let end = starts.get(i + 1).copied().unwrap_or(*offsets.last().unwrap());
            if end <= start { return Err("Empty or overlapping CUE track".into()); }
            let segments: Vec<_> = sources.iter().enumerate().filter_map(|(j, source)| {
                let a = start.max(offsets[j]); let b = end.min(offsets[j + 1]);
                (a < b).then_some(Segment { source: j, start: a.saturating_sub(offsets[j]), end: b.saturating_sub(offsets[j]).min(source.samples) })
            }).collect();
            if first.lossy && segments.len() != 1 { return Err("Lossy tracks spanning files cannot be safely copied".into()); }
            let cut = if first.lossy { Some(super::transfer::cut(store, &sources[segments[0].source], &segments[0], false)?) } else { None };
            tracks.push(AudioTrack { number: track.number, title: track.title.clone(), performer: if track.performer.is_empty() { sheet.performer.clone() } else { track.performer.clone() }, isrc: track.isrc.clone(), segments, duration: (end - start) as f64 / first.rate as f64, extension: if !first.lossy { "flac" } else if first.codec == "mp3" { "mp3" } else { "m4a" }.into(), cut });
        }
        if first.lossy { warnings.push("Lossy audio is copied at codec frame boundaries; gapless playback is not guaranteed".into()); }
        verify(store, &cue)?;
        let result = Analysis { id: id(), request, cue, sheet, sources, tracks, warnings };
        store.put("cue_analyses", &result.id, &result)?;
        Ok(result)
    })
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bit_depth_accepts_numeric_and_string_fields_and_safe_decoder_fallback() {
        assert_eq!(effective_bits(&json!({"bits_per_raw_sample":24}), "flac"), Some(24));
        assert_eq!(effective_bits(&json!({"bits_per_raw_sample":"24"}), "flac"), Some(24));
        assert_eq!(effective_bits(&json!({"bits_per_sample":"16"}), "ape"), Some(16));
        assert_eq!(effective_bits(&json!({"bits_per_sample":0,"sample_fmt":"s16p"}), "ape"), Some(16));
        assert_eq!(effective_bits(&json!({"sample_fmt":"s32"}), "flac"), None);
        assert_eq!(effective_bits(&json!({"sample_fmt":"s32"}), "pcm_s24le"), Some(24));
        assert_eq!(effective_bits(&json!({"bits_per_raw_sample":"20","sample_fmt":"s32"}), "flac"), Some(20));
    }
    #[test]
    fn flac_streaminfo_preserves_depth_when_probe_omits_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.flac");
        for bits in [16u8, 24, 32] {
            let mut header = [0u8; 42];
            header[..4].copy_from_slice(b"fLaC");
            header[4] = 0x80;
            header[7] = 34;
            header[20] = (bits - 1) >> 4;
            header[21] = (bits - 1) << 4;
            fs::write(&path, header).unwrap();
            assert_eq!(flac_header_bits(&path), Some(bits as u32));
        }
        fs::write(&path, b"not a FLAC file").unwrap();
        assert_eq!(flac_header_bits(&path), None);
    }
    #[test]
    fn resolves_directory_then_explicitly_selected_audio_without_guessing() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("cue");
        let other = dir.path().join("audio");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("source.flac"), b"audio").unwrap();
        let mut request = AnalyzeRequest {selected_paths: vec![paths::display(&other.join("source.flac"))], ..Default::default()};
        assert_eq!(resolve_source(&request,&parent,"source.flac").unwrap(),other.join("source.flac"));
        fs::write(parent.join("source.flac"), b"audio").unwrap();
        assert_eq!(resolve_source(&request,&parent,"source.flac").unwrap(),parent.join("source.flac"));
        assert!(resolve_source(&request,&parent,"../audio/source.flac").is_err());
        assert!(resolve_source(&request,&parent,"different.flac").is_err());
        request.selected_paths.push(paths::display(&parent.join("source.flac")));
        assert!(resolve_source(&request,&dir.path().join("missing"),"source.flac").is_err());
    }
}
