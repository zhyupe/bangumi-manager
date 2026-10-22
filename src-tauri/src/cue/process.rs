use crate::{db::Store, model::Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub struct Output {
    pub bytes: Vec<u8>,
    pub count: u64,
    pub hash: String,
}
// Drain both pipes even after the capture limit; a full pipe must never deadlock cancellation.
pub fn run(
    store: &Store,
    executable: &str,
    args: &[String],
    cancel: Option<&AtomicBool>,
    hash_only: bool,
    operation: &'static str,
) -> Result<Output> {
    store
        .diagnostics
        .measure_with(operation, serde_json::json!({}), |span| {
            let mut command = Command::new(executable);
            command
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command
                .spawn()
                .map_err(|e| format!("Cannot start audio tool: {e}"))?;
            let mut stdout = child.stdout.take().ok_or("Missing tool output")?;
            let mut stderr = child.stderr.take().ok_or("Missing tool diagnostics")?;
            let output = thread::spawn(move || -> std::io::Result<Output> {
                let mut bytes = Vec::new();
                let mut hash = Sha256::new();
                let mut count = 0;
                let mut buffer = [0u8; 65536];
                loop {
                    let n = stdout.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    hash.update(&buffer[..n]);
                    if !hash_only && bytes.len() < 128 * 1024 * 1024 {
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                }
                Ok(Output {
                    bytes,
                    count,
                    hash: hex::encode(hash.finalize()),
                })
            });
            let errors = thread::spawn(move || {
                let mut data = Vec::new();
                let mut buffer = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buffer[..n]);
                    if data.len() > 65536 {
                        data.drain(..data.len() - 65536);
                    }
                }
                String::from_utf8_lossy(&data).into_owned()
            });
            let started = Instant::now();
            let mut interrupted = false;
            let timeout = if operation == "cue.tool_check" {
                15
            } else if operation.contains("probe") {
                600
            } else {
                24 * 3600
            };
            let status = loop {
                if cancel.is_some_and(|c| c.load(Ordering::Relaxed))
                    || started.elapsed() > Duration::from_secs(timeout)
                {
                    interrupted = true;
                    let _ = child.kill();
                    break child.wait();
                }
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => thread::sleep(Duration::from_millis(50)),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(error);
                    }
                }
            };
            let data = output
                .join()
                .map_err(|_| "Audio output reader failed")?
                .map_err(|e| e.to_string())?;
            let stderr = errors
                .join()
                .map_err(|_| "Audio diagnostics reader failed")?;
            let status = status.map_err(|e| e.to_string())?;
            span.field("exit_code", status.code());
            if interrupted {
                return Err("Audio task stopped; source files were preserved".into());
            }
            if !status.success() {
                let mut redacted = stderr.clone();
                for arg in args
                    .iter()
                    .filter(|s| s.contains(['\\', '/']) && !s.starts_with('-'))
                {
                    redacted = redacted.replace(arg, "[file]");
                }
                span.field("stderr", &redacted);
                return Err(format!("Audio tool failed: {stderr}"));
            }
            if !hash_only && data.count > 128 * 1024 * 1024 {
                return Err("Audio analysis output exceeds its limit".into());
            }
            Ok(data)
        })
}
pub fn ffmpeg(store: &Store) -> Result<String> {
    let settings = store.settings()?;
    if !settings.ffmpeg.trim().is_empty() {
        return Ok(settings.ffmpeg);
    }
    let probe = std::path::Path::new(&settings.ffprobe);
    let sibling = probe.with_file_name(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    Ok(if sibling.is_file() {
        sibling.to_string_lossy().into_owned()
    } else {
        "ffmpeg".into()
    })
}
pub fn check(store: &Store, path: &str) -> Result<String> {
    let executable = if path.trim().is_empty() {
        ffmpeg(store)?
    } else {
        path.into()
    };
    let output = run(
        store,
        &executable,
        &["-version".into()],
        None,
        false,
        "cue.tool_check",
    )?;
    let line = String::from_utf8_lossy(&output.bytes)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    if !line.starts_with("ffmpeg version") {
        return Err("Selected executable is not FFmpeg".into());
    }
    Ok(line)
}
