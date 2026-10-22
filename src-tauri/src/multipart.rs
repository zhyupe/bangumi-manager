use crate::{model::Result, paths};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static PART: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?:^|[\s._~()\[\]-])(?:part|pt|cd|disc|disk|dvd)[ ._-]*0?([1-8])")
        .unwrap()
});

pub fn number(path: &Path) -> u32 {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    PART.captures_iter(&stem)
        .filter(|c| {
            stem[c.get(0).unwrap().end()..]
                .chars()
                .next()
                .is_none_or(|ch| ch.is_whitespace() || "._~()[]-".contains(ch))
        })
        .last()
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0)
}

pub fn validate(part: u32, kind: &str, extra: bool, episodes: usize) -> Result<()> {
    if part > 8 || (part > 0 && (extra || kind == "music" || episodes != 1)) {
        return Err("Part 编号须为 1–8，且只能用于单集或电影；完整文件请选择不分段".into());
    }
    Ok(())
}

pub fn target(path: PathBuf, part: u32) -> PathBuf {
    if part == 0 {
        return path;
    }
    path.with_file_name(format!(
        "{} - part{}.{}",
        path.file_stem().unwrap_or_default().to_string_lossy(),
        part,
        paths::ext(&path)
    ))
}

// Whole files occupy every part slot; different explicit parts may coexist.
pub fn slots(part: u32) -> std::ops::RangeInclusive<u32> {
    if part == 0 {
        0..=8
    } else {
        part..=part
    }
}
