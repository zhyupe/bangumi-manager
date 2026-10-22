use super::{analysis::*, planning, process};
use crate::{copy, db::Store, model::*, paths, scan};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}
fn filter_inputs(a: &Analysis, track: &AudioTrack) -> (Vec<String>, String) {
    let mut command = args(&["-v", "error", "-xerror", "-nostdin"]);
    let mut filters = vec![];
    for (i, segment) in track.segments.iter().enumerate() {
        command.extend(["-i".into(), a.sources[segment.source].file.path.clone()]);
        filters.push(format!(
            "[{i}:a:0]atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS[a{i}]",
            segment.start, segment.end
        ));
    }
    if track.segments.len() == 1 {
        filters.push("[a0]anull[out]".into());
    } else {
        filters.push(format!(
            "{}concat=n={}:v=0:a=1[out]",
            (0..track.segments.len())
                .map(|i| format!("[a{i}]"))
                .collect::<String>(),
            track.segments.len()
        ));
    }
    (command, filters.join(";"))
}
fn metadata(store: &Store, plan: &Plan, index: usize) -> Result<Vec<String>> {
    let work = &plan.works[0];
    let row = &plan.rows[index];
    let ep = work
        .episodes
        .iter()
        .find(|e| row.episode_ids.contains(&e.id))
        .ok_or("Track metadata missing")?;
    let context = planning::context(store, &plan.id)?;
    let count = context.total_tracks;
    let discs = context.total_discs;
    let mut tags = vec![
        ("album", work.title.clone()),
        ("title", ep.title.clone()),
        ("artist", ep.info.artist.clone()),
        ("album_artist", work.info.artist.clone()),
        ("track", format!("{}/{}", ep.number, count)),
        ("disc", format!("{}/{}", ep.season, discs)),
        ("date", work.info.air_date.clone()),
        ("TRACKTOTAL", count.to_string()),
        ("DISCTOTAL", discs.to_string()),
    ];
    for (key, id) in [
        (
            "MUSICBRAINZ_ALBUMID",
            work.info.external_ids.get("musicbrainz"),
        ),
        (
            "MUSICBRAINZ_RELEASEGROUPID",
            work.info.external_ids.get("musicbrainz_releasegroup"),
        ),
        (
            "MUSICBRAINZ_TRACKID",
            ep.info.external_ids.get("musicbrainz"),
        ),
        (
            "MUSICBRAINZ_RELEASETRACKID",
            ep.info.external_ids.get("musicbrainz_track"),
        ),
        ("ISRC", ep.info.external_ids.get("isrc")),
    ] {
        if let Some(id) = id {
            tags.push((key, id.clone()));
        }
    }
    Ok(tags
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .flat_map(|(k, v)| ["-metadata".into(), format!("{k}={v}")])
        .collect())
}
#[derive(Deserialize)]
struct Packet {
    pts_time: Option<String>,
    duration_time: Option<String>,
    data_hash: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cut {
    pub begin: usize,
    pub finish: usize,
    pub start: f64,
    pub end: f64,
    pub start_offset: f64,
    pub end_offset: f64,
}
pub fn cut(store: &Store, source: &AudioSource, segment: &Segment, cancel: bool) -> Result<Cut> {
    let p = packets(store, &source.file.path, cancel)?;
    let times = p
        .iter()
        .map(|p| {
            p.pts_time
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .ok_or("Audio packet timestamp missing")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if times.is_empty() || times.windows(2).any(|p| p[0] >= p[1]) {
        return Err("Audio packet timestamps are not increasing".into());
    }
    let desired_start = segment.start as f64 / source.rate as f64;
    let desired_end = segment.end as f64 / source.rate as f64;
    let nearest = |t: f64| {
        times
            .iter()
            .enumerate()
            .filter(|(_, v)| **v >= 0.0)
            .min_by(|(_, a), (_, b)| (**a - t).abs().total_cmp(&(**b - t).abs()))
            .map(|(i, _)| i)
            .ok_or("No safe audio packet boundary")
    };
    let begin = if segment.start == 0 {
        0
    } else {
        nearest(desired_start)?
    };
    let finish = if segment.end == source.samples {
        p.len()
    } else {
        nearest(desired_end)?
    };
    if begin >= finish {
        return Err("Track is shorter than one encoded audio frame".into());
    }
    let start = if begin == 0 { 0.0 } else { times[begin] };
    let end = if finish == p.len() {
        desired_end
    } else {
        times[finish]
    };
    for (index, delta) in [
        (begin, start - desired_start),
        (finish.min(p.len() - 1), end - desired_end),
    ] {
        let frame = p[index]
            .duration_time
            .as_deref()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or("Audio frame duration missing")?;
        if delta.abs() > frame {
            return Err("Lossy cut exceeds one audio frame; use an external splitter".into());
        }
    }
    Ok(Cut {
        begin,
        finish,
        start,
        end,
        start_offset: start - desired_start,
        end_offset: end - desired_end,
    })
}
fn packets(store: &Store, path: &str, cancel: bool) -> Result<Vec<Packet>> {
    let mut command = args(&[
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_packets",
        "-show_entries",
        "packet=pts_time,duration_time,data_hash",
        "-show_data_hash",
        "sha256",
        "-of",
        "json",
    ]);
    command.push(path.into());
    let out = process::run(
        store,
        &store.settings()?.ffprobe,
        &command,
        Some(if cancel {
            &store.cancel
        } else {
            &store.cue_cancel
        }),
        false,
        "cue.packet_probe",
    )?;
    #[derive(Deserialize)]
    struct Packets {
        packets: Vec<Packet>,
    }
    serde_json::from_slice::<Packets>(&out.bytes)
        .map(|v| v.packets)
        .map_err(|e| e.to_string())
}
fn pcm(store: &Store, executable: &str, mut command: Vec<String>) -> Result<process::Output> {
    command.extend(args(&["-c:a", "pcm_s32le", "-f", "s32le", "-"]));
    process::run(
        store,
        executable,
        &command,
        Some(&store.cancel),
        true,
        "cue.audio_verify",
    )
}
fn render(
    store: &Store,
    plan: &Plan,
    index: usize,
    a: &Analysis,
    executable: &str,
    stage: &Path,
) -> Result<()> {
    let track = &a.tracks[index];
    let source = &a.sources[track.segments[0].source];
    if !source.lossy {
        let (mut input, filter) = filter_inputs(a, track);
        input.extend([
            "-filter_complex".into(),
            filter,
            "-map".into(),
            "[out]".into(),
        ]);
        let expected = pcm(store, executable, input.clone())?;
        let expected_samples: u64 = track.segments.iter().map(|s| s.end - s.start).sum();
        if expected.count != expected_samples * source.channels as u64 * 4 {
            return Err("Source decoding does not cover the planned sample range".into());
        }
        input.extend(args(&[
            "-map_metadata",
            "-1",
            "-c:a",
            "flac",
            "-compression_level",
            "8",
        ]));
        input.extend([
            "-sample_fmt".into(),
            if source.bits <= 16 { "s16" } else { "s32" }.into(),
            "-bits_per_raw_sample".into(),
            source.bits.to_string(),
        ]);
        input.extend(metadata(store, plan, index)?);
        input.extend(["-n".into(), paths::display(stage)]);
        process::run(
            store,
            executable,
            &input,
            Some(&store.cancel),
            false,
            "cue.encode",
        )?;
        let actual = pcm(
            store,
            executable,
            args(&[
                "-v",
                "error",
                "-xerror",
                "-i",
                &paths::display(stage),
                "-map",
                "0:a:0",
            ]),
        )?;
        if actual.count != expected.count || actual.hash != expected.hash {
            return Err("Lossless track sample verification failed".into());
        }
    } else {
        let segment = &track.segments[0];
        let input_packets = packets(store, &source.file.path, true)?;
        let cuts = cut(store, source, segment, true)?;
        let recorded = track.cut.as_ref().ok_or("Lossy cut confirmation missing")?;
        if recorded.begin != cuts.begin
            || recorded.finish != cuts.finish
            || (recorded.start - cuts.start).abs() > 1e-9
            || (recorded.end - cuts.end).abs() > 1e-9
        {
            return Err("Audio packet boundaries changed; preview again".into());
        }
        let begin = cuts.begin;
        let finish = cuts.finish;
        let mut command = args(&["-v", "error", "-xerror", "-i", &source.file.path]);
        if begin > 0 {
            command.extend(["-ss".into(), format!("{:.9}", cuts.start)]);
        }
        if finish < input_packets.len() {
            command.extend(["-t".into(), format!("{:.9}", cuts.end - cuts.start)]);
        }
        command.extend(args(&[
            "-map",
            "0:a:0",
            "-map_metadata",
            "-1",
            "-c:a",
            "copy",
        ]));
        command.extend(metadata(store, plan, index)?);
        if source.codec == "aac" {
            command.extend(args(&["-movflags", "use_metadata_tags"]));
        }
        command.extend(["-n".into(), paths::display(stage)]);
        process::run(
            store,
            executable,
            &command,
            Some(&store.cancel),
            false,
            "cue.stream_copy",
        )?;
        let output_packets = packets(store, &paths::display(stage), true)?;
        if output_packets.len() != finish - begin
            || output_packets
                .iter()
                .zip(&input_packets[begin..finish])
                .any(|(a, b)| a.data_hash.is_none() || a.data_hash != b.data_hash)
        {
            return Err(
                "Cannot verify lossless packet copying at this cut; use an external splitter"
                    .into(),
            );
        }
        // MP3 reservoirs and codec priming can make an isolated packet range undecodable.
        let actual = pcm(
            store,
            executable,
            args(&[
                "-v",
                "error",
                "-xerror",
                "-i",
                &paths::display(stage),
                "-map",
                "0:a:0",
            ]),
        )?;
        let start_sample = (cuts.start * source.rate as f64).round() as u64;
        let end_sample = (cuts.end * source.rate as f64).round() as u64;
        let filter = format!(
            "atrim=start_sample={start_sample}:end_sample={end_sample},asetpts=PTS-STARTPTS"
        );
        let expected = pcm(
            store,
            executable,
            args(&[
                "-v",
                "error",
                "-xerror",
                "-i",
                &source.file.path,
                "-map",
                "0:a:0",
                "-af",
                &filter,
            ]),
        )?;
        if actual.count == 0 || actual.count != expected.count || actual.hash != expected.hash {
            return Err("Independent decoding differs at the cut (codec delay or frame dependency); use an external splitter".into());
        }
    }
    let probe = scan::probe_controlled(store, &paths::display(stage), true, Some(&store.cancel))?;
    if probe.error.is_some()
        || probe
            .audio_tracks
            .first()
            .is_none_or(|t| t.codec != if source.lossy { &source.codec } else { "flac" })
        || probe.sample_rate != Some(source.rate)
        || probe.audio_tracks.first().and_then(|t| t.channels) != Some(source.channels as u64)
        || (!source.lossy && super::analysis::flac_header_bits(stage).or(probe.audio_bit_depth) != Some(source.bits))
    {
        return Err("Split audio format verification failed".into());
    }
    Ok(())
}
fn space(path: &Path, required: u64) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0u64;
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if available < required {
            return Err("Insufficient free space for CUE staging".into());
        }
    }
    #[cfg(not(windows))]
    let _ = (path, required);
    Ok(())
}
pub fn transfer(store: &Store, plan: &mut Plan, progress: &impl Fn(&Plan)) -> Result<()> {
    let ctx = planning::context(store, &plan.id)?;
    if ctx.cleanup.started {
        super::cleanup::verify_outputs(store, plan)?;
        return Ok(());
    }
    let _inputs = ctx
        .analysis
        .sources
        .iter()
        .map(|s| &s.file)
        .chain([&ctx.analysis.cue])
        .map(|f| crate::remote_transfer::LockedFile::open(Path::new(&f.path), false))
        .collect::<Result<Vec<_>>>()?;
    for source in &ctx.analysis.sources {
        verify(store, &source.file)?;
    }
    verify(store, &ctx.analysis.cue)?;
    let root = Path::new(&plan.music_root).to_path_buf();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let required = ctx
        .analysis
        .sources
        .iter()
        .map(|s| {
            s.samples
                .saturating_mul(s.channels as u64)
                .saturating_mul(4)
        })
        .sum::<u64>()
        .saturating_add(16 * 1024 * 1024);
    space(&root, required)?;
    // Generation is complete for the whole CUE before any destination is replaced.
    for i in 0..plan.rows.len() {
        if store.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if plan.rows[i].action == "skip" {
            plan.rows[i].state = "skipped".into();
            copy::save(store, plan)?;
            continue;
        }
        let row = plan.rows[i].clone();
        if row.state == "complete" {
            if copy::measured_digest(store, Path::new(&row.target))? != row.digest {
                return Err("Committed CUE track changed".into());
            }
            continue;
        }
        let target = PathBuf::from(&row.target);
        paths::safe_target_entry(&target, &root)?;
        let stage = target
            .parent()
            .ok_or("Target directory missing")?
            .join(format!(
                ".bm-cue-{}-{}.{}",
                plan.id, row.id, ctx.analysis.tracks[i].extension
            ));
        paths::safe_target_entry(&stage, &root)?;
        if !row.digest.is_empty() {
            if stage.is_file() && copy::measured_digest(store, &stage)? == row.digest {
                continue;
            }
            if !stage.exists()
                && target.is_file()
                && copy::measured_digest(store, &target)? == row.digest
            {
                copy::complete_row(store, plan, i)?;
                continue;
            }
            return Err("Verified CUE output changed; recovery stopped".into());
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| serde_json::json!({"code":"errors.cue.outputDirectory","params":{"path":paths::display(target.parent().unwrap()),"error":e.to_string()}}).to_string())?;
        paths::safe_target_entry(&stage, &root)?;
        if stage.exists() {
            crate::resources::regular(&stage, Some(&root))?;
            fs::remove_file(&stage).map_err(|e| e.to_string())?;
        }
        plan.rows[i].stage = paths::display(&stage);
        plan.rows[i].state = "staging".into();
        plan.rows[i].error.clear();
        copy::save(store, plan)?;
        progress(plan);
        if let Err(error) = render(store, plan, i, &ctx.analysis, &ctx.ffmpeg, &stage) {
            if store.cancel.load(Ordering::Relaxed) {
                copy::save(store, plan)?;
                return Ok(());
            }
            plan.rows[i].state = "failed".into();
            plan.rows[i].error = error.clone();
            copy::save(store, plan)?;
            progress(plan);
            return Err(error);
        }
        fs::OpenOptions::new()
            .write(true)
            .open(&stage)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        plan.rows[i].digest = copy::measured_digest(store, &stage)?;
        plan.rows[i].probe = Some(scan::probe(store, &paths::display(&stage))?);
        plan.rows[i].state = "staged".into();
        copy::save(store, plan)?;
        progress(plan);
    }
    for source in &ctx.analysis.sources {
        verify(store, &source.file)?;
    }
    verify(store, &ctx.analysis.cue)?;
    for i in 0..plan.rows.len() {
        if store.cancel.load(Ordering::Relaxed) {
            break;
        }
        if ["complete", "skipped"].contains(&plan.rows[i].state.as_str()) {
            continue;
        }
        let target = PathBuf::from(&plan.rows[i].target);
        let stage = PathBuf::from(&plan.rows[i].stage);
        paths::safe_target_entry(&target, &root)?;
        paths::safe_target_entry(&stage, &root)?;
        let output = crate::remote_transfer::LockedFile::open(&stage, true)?;
        if copy::measured_digest(store, &stage)? != plan.rows[i].digest {
            return Err("Staged CUE output changed before commit".into());
        }
        copy::check_destination(plan, i, &target, false)?;
        if plan.rows[i].action == "replace" {
            copy::backup_existing(store, plan, i)?;
        }
        if store.cancel.load(Ordering::Relaxed) {
            break;
        }
        plan.rows[i].state = "committing".into();
        copy::save(store, plan)?;
        store.diagnostics.measure(
            "cue.commit",
            serde_json::json!({"task_id":plan.id,"row_id":plan.rows[i].id}),
            || output.rename(&target).map_err(|e| e.to_string()),
        )?;
        drop(output);
        copy::complete_row(store, plan, i)?;
        progress(plan);
    }
    Ok(())
}
