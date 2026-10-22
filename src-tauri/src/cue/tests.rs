use super::*;
use crate::{copy, db::Store, model::*, paths};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

struct Fixture {
    root: PathBuf,
    store: Arc<Store>,
    ffmpeg: String,
    _dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Option<Self> {
        let ffmpeg = std::env::var("CUE_TEST_FFMPEG").ok()?;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = Store::open(root.join("db")).unwrap();
        let settings = Settings {
            output_root: paths::display(&root.join("library")),
            backup_root: paths::display(&root.join("backups")),
            extras_root: paths::display(&root.join("extras")),
            ffmpeg: ffmpeg.clone(),
            ffprobe: paths::display(&Path::new(&ffmpeg).with_file_name("ffprobe.exe")),
            default_operation: "move".into(),
            ..Default::default()
        };
        store.put("settings", "main", &settings).unwrap();
        Some(Self {
            root,
            store,
            ffmpeg,
            _dir: dir,
        })
    }
    fn source(&self, filename: &str, codec: &str, extra: &[&str]) {
        let path = self.root.join(filename);
        let mut args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=613:sample_rate=44100:duration=3",
            "-ac",
            "2",
            "-c:a",
            codec,
        ]
        .map(str::to_string)
        .to_vec();
        args.extend(extra.iter().map(|s| s.to_string()));
        args.push(paths::display(&path));
        process::run(
            &self.store,
            &self.ffmpeg,
            &args,
            None,
            false,
            "cue.test_generate",
        )
        .unwrap();
    }
    fn analyze(&self, text: &str) -> Result<Analysis> {
        let path = self.root.join("album.cue");
        fs::write(&path, text).unwrap();
        analyze(
            &self.store,
            AnalyzeRequest {
                path: paths::display(&path),
                ..Default::default()
            },
        )
    }
    fn plan(&self, a: &Analysis) -> Plan {
        self.plan_with_disposition(a, SourceDisposition::Keep)
    }
    fn plan_with_disposition(&self, a: &Analysis, disposition: SourceDisposition) -> Plan {
        let p = preview(
            &self.store,
            SplitRequest {
                analysis_id: a.id.clone(),
                release_id: String::new(),
                work_id: None,
                disc: 1,
                title: "Fictional Album".into(),
                artist: "Example Ensemble".into(),
                date: String::new(),
                tracks: a
                    .tracks
                    .iter()
                    .map(|t| TrackChoice {
                        number: t.number,
                        remote_id: String::new(),
                        title: format!("Track {}", t.number),
                        performer: "Example Ensemble".into(),
                    })
                    .collect(),
            },
        )
        .unwrap();
        assert!(self.store.list::<Work>("works").unwrap().is_empty());
        assert!(self.store.list::<Plan>("plans").unwrap().is_empty());
        if disposition != SourceDisposition::Keep {
            assert!(configure_cleanup(&self.store, &p.id, disposition, false).is_err());
        }
        let mut legacy = serde_json::to_value(context(&self.store, &p.id).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("cleanup");
        assert_eq!(serde_json::from_value::<Context>(legacy).unwrap().cleanup.disposition, SourceDisposition::Keep);
        configure_cleanup(&self.store, &p.id, disposition, true).unwrap();
        submit(
            &self.store,
            &p.id,
            p.rows
                .iter()
                .map(|r| Decision {
                    id: r.id.clone(),
                    action: "copy".into(),
                    operation: "split".into(),
                    reviewed: true,
                })
                .collect(),
        )
        .unwrap()
    }
}

#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn lossless_hidden_audio_gaps_and_recovery_preserve_every_sample() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.wav", "pcm_s24le", &[]);
    let a=f.analyze("TITLE \"Fictional Album\"\nFILE \"source.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:15\nTRACK 02 AUDIO\nINDEX 00 00:00:60\nINDEX 01 00:01:00\n").unwrap();
    assert_eq!(a.tracks[0].segments[0].start, 0);
    assert_eq!(a.tracks[0].segments[0].end, 44100);
    assert_eq!(a.tracks[1].segments[0].start, 44100);
    let mut p = f.plan(&a);
    let mut count = 0;
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    for row in &p.rows {
        assert_eq!(row.state, "complete");
        let s = audio(&f.store, Path::new(&row.target)).unwrap();
        assert_eq!(s.bits, 24);
        count += s.samples;
    }
    assert_eq!(count, a.sources[0].samples);
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    assert!(f.root.join("source.wav").exists());
    assert!(f.root.join("album.cue").exists());
    assert_eq!(submit(&f.store, &p.id, vec![]).unwrap().id, p.id);
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn changed_inputs_and_existing_destinations_are_not_overwritten() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f
        .analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00")
        .unwrap();
    let p = f.plan(&a);
    fs::create_dir_all(Path::new(&p.rows[0].target).parent().unwrap()).unwrap();
    fs::write(&p.rows[0].target, b"unrelated").unwrap();
    let mut p = p;
    assert!(transfer(&f.store, &mut p, &|_| {}).is_err());
    assert_eq!(fs::read(&p.rows[0].target).unwrap(), b"unrelated");
    fs::write(&a.cue.path, b"changed").unwrap();
    assert!(preflight(&f.store, &p, false).is_err());
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn multifile_boundaries_cover_both_sources_and_reject_float() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("first.wav", "pcm_s16le", &[]);
    f.source("second.wav", "pcm_s16le", &[]);
    let a=f.analyze("FILE \"first.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nFILE \"second.wav\" WAVE\nTRACK 02 AUDIO\nINDEX 00 00:00:00\nINDEX 01 00:00:15").unwrap();
    assert_eq!(a.tracks[0].segments.len(), 2);
    let mut p = f.plan(&a);
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    f.source("float.wav", "pcm_f32le", &[]);
    assert!(f
        .analyze("FILE \"float.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00")
        .is_err());
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn full_queue_execution_registers_tracks_and_retains_sources() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f
        .analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00")
        .unwrap();
    crate::scan::scan_sources(&f.store, vec![paths::display(&f.root.join("album.cue")), paths::display(&f.root.join("source.flac"))], |_| {}).unwrap();
    let p = f.plan(&a);
    let key = paths::key(&f.root.join("source.flac"));
    for state in ["failed", "cancelled", "queued"] {
        let mut incomplete = p.clone();
        incomplete.state = state.into();
        clear_completed_sources(&f.store, &incomplete).unwrap();
        assert!(f.store.get::<SourceFile>("sources", &key).unwrap().is_some());
    }
    let mut skipped = p.clone();
    skipped.state = "complete".into();
    skipped.rows[0].state = "skipped".into();
    skipped.rows[0].action = "skip".into();
    clear_completed_sources(&f.store, &skipped).unwrap();
    assert!(f.store.get::<SourceFile>("sources", &key).unwrap().is_some());
    copy::start_plan(&f.store, &p.id).unwrap();
    let result = copy::execute(f.store.clone(), p.id.clone(), |_| {});
    let done =
        copy::finish_plan(&f.store, &p.id, result.as_ref().err().map(String::as_str)).unwrap();
    assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
    assert!(f.root.join("source.flac").exists());
    assert!(f.root.join("album.cue").exists());
    for name in ["source.flac", "album.cue"] {
        assert!(f.store.get::<SourceFile>("sources", &paths::key(&f.root.join(name))).unwrap().is_none());
    }
    clear_completed_sources(&f.store, &done).unwrap();
    fs::write(f.root.join("source.flac"), b"changed input").unwrap();
    crate::scan::scan_sources(&f.store, vec![paths::display(&f.root.join("source.flac"))], |_| {}).unwrap();
    clear_completed_sources(&f.store, &done).unwrap();
    assert!(f.store.get::<SourceFile>("sources", &key).unwrap().is_some());
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn lossy_frames_are_previewed_and_unsafe_decoding_never_commits() {
    for (filename, codec, extra) in [
        ("source.mp3", "libmp3lame", vec!["-reservoir", "1"]),
        ("source.m4a", "aac", vec![]),
    ] {
        let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
        f.source(filename, codec, &extra);
        let a=f.analyze(&format!("FILE \"{filename}\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:01:00")).unwrap();
        assert!(a.tracks.iter().all(|t| t.cut.is_some()));
        assert!(a.tracks[1].cut.as_ref().unwrap().start_offset.abs() < 0.04);
        let mut p = f.plan(&a);
        let result = transfer(&f.store, &mut p, &|_| {});
        let error = result.expect_err("dependent lossy cut must stop");
        assert!(
            error.contains("cut") || error.contains("Audio tool failed"),
            "{error}"
        );
        assert!(p.rows.iter().all(|r| !Path::new(&r.target).exists()));
        assert!(f.root.join(filename).exists());
    }
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn cancelled_generation_resumes_and_partial_commit_is_recognized() {
    use std::sync::atomic::Ordering;
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.wav", "pcm_s16le", &[]);
    let a=f.analyze("FILE \"source.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:01:00").unwrap();
    let mut p = f.plan(&a);
    transfer(&f.store, &mut p, &|p| {
        if p.rows[0].state == "staged" {
            f.store.cancel.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert!(p.rows.iter().all(|r| !Path::new(&r.target).exists()));
    assert_eq!(p.rows[0].state, "staged");
    let stage = p.rows[0].stage.clone();
    let digest = p.rows[0].digest.clone();
    paths::commit_no_replace(Path::new(&stage), Path::new(&p.rows[0].target)).unwrap();
    f.store.cancel.store(false, Ordering::SeqCst);
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    assert_eq!(p.rows[0].digest, digest);
    assert!(p.rows.iter().all(|r| r.state == "complete"));
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn whole_lossy_tracks_keep_their_encoded_packets() {
    for (filename, codec) in [("source.mp3", "libmp3lame"), ("source.m4a", "aac")] {
        let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
        f.source(filename, codec, &[]);
        let a = f
            .analyze(&format!(
                "FILE \"{filename}\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00"
            ))
            .unwrap();
        let mut p = f.plan(&a);
        transfer(&f.store, &mut p, &|_| {}).unwrap();
        assert_eq!(p.rows[0].state, "complete");
        assert!(f.root.join(filename).exists());
    }
}
#[test]
fn linking_local_cue_album_preserves_episode_identity() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(d.path().into()).unwrap();
    let w = Work {
        id: "local-album".into(),
        title: "Example Album".into(),
        folder_name: "Example Album".into(),
        kind: "music".into(),
        bindings: vec![Binding {
            id: "local".into(),
            provider: "cue".into(),
            remote_id: String::new(),
            source_kind: "music".into(),
            source_title: "Example Album".into(),
            season: 1,
            episode_group: None,
        }],
        episodes: vec![Episode {
            id: "stable-track".into(),
            binding_id: "local".into(),
            remote_id: "nfo:1:1".into(),
            number: 1,
            season: 1,
            sort: 1.0,
            title: "Local Track".into(),
            enabled: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    store.put("works", &w.id, &w).unwrap();
    let subject = RemoteSubject {
        id: "00000000-0000-4000-8000-000000000001".into(),
        provider: "musicbrainz".into(),
        kind: "music".into(),
        title: w.title.clone(),
        episodes: vec![RemoteEpisode {
            id: "00000000-0000-4000-8000-000000000002".into(),
            sort: 1.0,
            season: 1,
            title: "Updated Track".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let updated = crate::providers::bind(
        &store,
        BindRequest {
            subject,
            work_id: Some(w.id.clone()),
            title: w.title,
            kind: "music".into(),
            season: 1,
            tmdb_id: String::new(),
            parent_id: String::new(),
            group_change_confirmed: false,
        },
    )
    .unwrap();
    assert_eq!(updated.episodes.len(), 1);
    assert_eq!(updated.episodes[0].id, "stable-track");
    assert_eq!(updated.episodes[0].title, "Updated Track");
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn cancellation_stops_a_running_audio_process() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    let worker = f.store.clone();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        worker
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let started = std::time::Instant::now();
    let result = process::run(
        &f.store,
        &f.ffmpeg,
        &[
            "-v",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-t",
            "30",
            "-f",
            "null",
            "-",
        ]
        .map(String::from),
        Some(&f.store.cancel),
        false,
        "cue.test_cancel",
    );
    cancel.join().unwrap();
    assert!(result.err().unwrap().contains("stopped"));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}
#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn replacement_backup_failure_retains_staged_tracks_and_recovers() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f
        .analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00")
        .unwrap();
    let mut old = f.plan(&a);
    transfer(&f.store, &mut old, &|_| {}).unwrap();
    old.state = "complete".into();
    copy::save(&f.store, &mut old).unwrap();
    let old_hash = copy::digest(Path::new(&old.rows[0].target)).unwrap();
    let mut req = context(&f.store, &old.id).unwrap().request;
    req.work_id = Some(old.works[0].id.clone());
    let p = preview(&f.store, req).unwrap();
    assert!(!p.rows[0].backups[0].backup.is_empty());
    let mut p = submit(
        &f.store,
        &p.id,
        p.rows
            .iter()
            .map(|r| Decision {
                id: r.id.clone(),
                action: "replace".into(),
                operation: "split".into(),
                reviewed: true,
            })
            .collect(),
    )
    .unwrap();
    fs::write(&p.backup_root, b"blocked directory").unwrap();
    assert!(transfer(&f.store, &mut p, &|_| {}).is_err());
    assert_eq!(
        copy::digest(Path::new(&p.rows[0].target)).unwrap(),
        old_hash
    );
    assert!(Path::new(&p.rows[0].stage).exists());
    fs::remove_file(&p.backup_root).unwrap();
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    assert_eq!(
        copy::digest(Path::new(&p.rows[0].backups[0].backup)).unwrap(),
        old_hash
    );
    p.state = "complete".into();
    copy::save(&f.store, &mut p).unwrap();
    copy::delete_plan(&f.store, &p.id).unwrap();
    assert!(!f.store.list::<BackupRecord>("backups").unwrap().is_empty());
    assert!(f.root.join("source.flac").exists());
}

#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn original_disposition_executes_only_after_verified_import_and_is_recoverable() {
    for disposition in [SourceDisposition::Keep, SourceDisposition::Delete, SourceDisposition::Backup] {
        let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
        f.source("source.flac", "flac", &[]);
        let a = f.analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00").unwrap();
        let p = f.plan_with_disposition(&a, disposition);
        let mut pending = p.clone();
        finish_cleanup(&f.store, &mut pending).unwrap();
        assert!(Path::new(&a.cue.path).exists());
        let mut skipped = p.clone();
        skipped.state = "complete".into();
        skipped.rows[0].state = "skipped".into();
        skipped.rows[0].action = "skip".into();
        finish_cleanup(&f.store, &mut skipped).unwrap();
        assert!(Path::new(&a.sources[0].file.path).exists());
        copy::start_plan(&f.store, &p.id).unwrap();
        let result = copy::execute(f.store.clone(), p.id.clone(), |_| {});
        let mut done = copy::finish_plan(&f.store, &p.id, result.as_ref().err().map(String::as_str)).unwrap();
        assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
        for file in [&a.cue, &a.sources[0].file] {
            assert_eq!(Path::new(&file.path).exists(), disposition == SourceDisposition::Keep);
        }
        if disposition != SourceDisposition::Keep {
            // Replay after an interruption: transfer must validate outputs without requiring removed inputs.
            preflight(&f.store, &done, false).unwrap();
            transfer(&f.store, &mut done, &|_| {}).unwrap();
            finish_cleanup(&f.store, &mut done).unwrap();
            fs::write(&a.cue.path, b"new unrelated cue").unwrap();
            finish_cleanup(&f.store, &mut done).unwrap();
            assert_eq!(fs::read(&a.cue.path).unwrap(), b"new unrelated cue");
        }
        if disposition == SourceDisposition::Backup {
            let records = crate::backups::list(&f.store).unwrap();
            assert_eq!(records.len(), 2);
            assert!(records.iter().all(|r| r.complete && Path::new(&r.entry.backup).is_file()));
            copy::delete_plan(&f.store, &p.id).unwrap();
            assert_eq!(crate::backups::list(&f.store).unwrap().len(), 2);
        }
    }
}

#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn original_disposition_protects_changed_inputs_outputs_and_shared_sources() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f.analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00").unwrap();
    let mut p = f.plan_with_disposition(&a, SourceDisposition::Delete);
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    p.state = "complete".into();
    let mut other = p.clone();
    other.id = "other-task".into();
    other.kind = "organize".into();
    other.state = "draft".into();
    f.store.put("plans", &other.id, &other).unwrap();
    assert!(finish_cleanup(&f.store, &mut p).is_err());
    assert!(Path::new(&a.cue.path).exists());
    f.store.remove("plans", &other.id).unwrap();
    let original = fs::read(&a.cue.path).unwrap();
    fs::write(&a.cue.path, b"changed").unwrap();
    assert!(finish_cleanup(&f.store, &mut p).is_err());
    assert!(Path::new(&a.sources[0].file.path).exists());
    fs::write(&a.cue.path, original).unwrap();
    fs::write(&p.rows[0].target, b"damaged output").unwrap();
    assert!(finish_cleanup(&f.store, &mut p).is_err());
    assert!(Path::new(&a.sources[0].file.path).exists());
}

#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn original_backup_partial_move_resumes_without_requiring_missing_inputs() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f.analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00").unwrap();
    let mut p = f.plan_with_disposition(&a, SourceDisposition::Backup);
    transfer(&f.store, &mut p, &|_| {}).unwrap();
    p.state = "complete".into();
    let ctx = serde_json::to_value(context(&f.store, &p.id).unwrap()).unwrap();
    let second_backup = PathBuf::from(ctx["cleanup"]["entries"][1]["backup"].as_str().unwrap());
    fs::create_dir_all(second_backup.parent().unwrap()).unwrap();
    fs::write(&second_backup, b"unrelated backup").unwrap();
    assert!(finish_cleanup(&f.store, &mut p).is_err());
    assert!(!Path::new(&a.cue.path).exists());
    assert!(Path::new(&a.sources[0].file.path).exists());
    assert_eq!(fs::read(&second_backup).unwrap(), b"unrelated backup");
    assert!(copy::cancel_plan(&f.store, &p.id).is_err());
    assert!(copy::delete_plan(&f.store, &p.id).is_err());
    fs::remove_file(&second_backup).unwrap();
    // Simulate a crash between moving the CUE and recording completion.
    let mut ctx = serde_json::to_value(context(&f.store, &p.id).unwrap()).unwrap();
    ctx["cleanup"]["entries"][0]["done"] = false.into();
    f.store.put("cue_tasks", &p.id, &ctx).unwrap();
    copy::start_plan(&f.store, &p.id).unwrap();
    let result = copy::execute(f.store.clone(), p.id.clone(), |_| {});
    let done = copy::finish_plan(&f.store, &p.id, result.as_ref().err().map(String::as_str)).unwrap();
    assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
    assert!(!Path::new(&a.sources[0].file.path).exists());
    assert_eq!(crate::backups::list(&f.store).unwrap().len(), 2);
}

#[test]
#[ignore = "requires CUE_TEST_FFMPEG and adjacent ffprobe executable"]
fn long_unicode_album_paths_split_and_commit_with_portable_components() {
    let f = Fixture::new().expect("Set CUE_TEST_FFMPEG");
    f.source("source.flac", "flac", &[]);
    let a = f.analyze("FILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00").unwrap();
    let title = format!("{} Fictional Album", "架空作品集、".repeat(22));
    let p = preview(&f.store, SplitRequest {
        analysis_id: a.id.clone(), release_id: String::new(), work_id: None, disc: 1,
        title: title.clone(), artist: "架空演奏者、".repeat(24), date: "2024".into(),
        tracks: vec![TrackChoice { number: 1, remote_id: String::new(), title: "架空音楽🎵".repeat(40), performer: "Example Ensemble".into() }],
    }).unwrap();
    assert_eq!(p.works[0].title, title);
    for component in Path::new(&p.rows[0].target).components() {
        assert!(component.as_os_str().to_string_lossy().len() <= 240);
    }
    assert_eq!(paths::ext(Path::new(&p.rows[0].target)), "flac");
    let p = submit(&f.store, &p.id, p.rows.iter().map(|r| Decision {
        id: r.id.clone(), action: "copy".into(), operation: "split".into(), reviewed: true,
    }).collect()).unwrap();
    copy::start_plan(&f.store, &p.id).unwrap();
    let result = copy::execute(f.store.clone(), p.id.clone(), |_| {});
    let done = copy::finish_plan(&f.store, &p.id, result.as_ref().err().map(String::as_str)).unwrap();
    assert_eq!(done.state, "complete", "{:?}", done.metadata_errors);
    assert!(Path::new(&done.rows[0].target).is_file());
    assert!(Path::new(&a.sources[0].file.path).is_file());
}
