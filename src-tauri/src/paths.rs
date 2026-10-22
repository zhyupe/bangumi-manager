use crate::model::*;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    }
}
pub fn normalize(raw: &str) -> Result<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("路径不能为空".into());
    }
    let path = if raw.to_lowercase().starts_with("smb://") {
        let u = url::Url::parse(raw).map_err(|e| e.to_string())?;
        if !u.username().is_empty() || u.password().is_some() || u.port().is_some() {
            return Err("SMB 地址请使用已登录的 Windows 共享，不在路径中填写账号或端口".into());
        }
        let host = u.host_str().ok_or("SMB 地址缺少服务器")?;
        let suffix = percent_encoding::percent_decode_str(u.path())
            .decode_utf8()
            .map_err(|e| e.to_string())?;
        if !cfg!(windows) {
            return Err("此系统请先挂载 SMB 共享，再填写本地挂载路径".into());
        }
        PathBuf::from(format!(r"\\{}{}", host, suffix.replace('/', r"\")))
    } else if raw.starts_with("file://") {
        url::Url::parse(raw)
            .map_err(|e| e.to_string())?
            .to_file_path()
            .map_err(|_| "无效的文件地址")?
    } else {
        PathBuf::from(raw)
    };
    if !path.is_absolute() {
        return Err("请输入绝对路径，例如 D:\\Anime 或 \\\\server\\share\\Anime".into());
    }
    if path
        .components()
        .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err("路径不能包含 ..".into());
    }
    Ok(path)
}
pub fn key(path: &Path) -> String {
    let s = display(path)
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}
pub fn contained(path: &Path, root: &Path) -> bool {
    let p = key(path);
    let r = key(root);
    p == r || p.starts_with(&(r + "/"))
}
pub fn resolved(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| format!("{}：{}", display(path), e));
    }
    let parent = path.parent().ok_or("找不到路径的父目录")?;
    Ok(resolved(parent)?.join(path.file_name().ok_or("无效的路径")?))
}
pub fn safe_target(path: &Path, root: &Path) -> Result<()> {
    if !contained(&resolved(path)?, &resolved(root)?) || key(path) == key(root) {
        return Err("目标路径越出资料库或指向根目录".into());
    }
    Ok(())
}
// Resolve parent directories but preserve the final entry, including a file symlink.
fn resolved_entry(path: &Path) -> Result<PathBuf> {
    Ok(resolved(path.parent().ok_or("无效父目录")?)?.join(path.file_name().ok_or("无效文件名")?))
}
pub fn safe_target_entry(path: &Path, root: &Path) -> Result<()> {
    if !contained(&resolved_entry(path)?, &resolved(root)?) || key(path) == key(root) {
        return Err("目标路径越出资料库或指向根目录".into());
    }
    Ok(())
}
pub fn symlink_file(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(source, target);
    #[cfg(not(windows))]
    let result = std::os::unix::fs::symlink(source, target);
    result.map_err(|e| {
        format!("无法创建软链接（Windows 需要开发者模式或相应权限，共享也需支持）：{e}")
    })
}
pub fn signature(path: &Path) -> Result<Signature> {
    let m = fs::metadata(path).map_err(|e| format!("{}：{}", display(path), e))?;
    if !m.is_file() {
        return Err(format!("不是普通文件：{}", display(path)));
    }
    Ok(Signature {
        size: m.len(),
        modified: m
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    })
}
pub fn optional_signature(path: &Path) -> Result<Option<Signature>> {
    match fs::symlink_metadata(path) {
        Ok(m) if is_link(&m) => Err(format!(
            "目标是链接或重解析点，需人工处理：{}",
            display(path)
        )),
        Ok(_) => signature(path).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("无法确认目标状态：{}：{}", display(path), e)),
    }
}
pub fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}
pub fn clean_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    out = out.trim().trim_end_matches([' ', '.']).to_string();
    let stem = out.split('.').next().unwrap_or("").to_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        out.insert(0, '_');
    }
    if out.is_empty() {
        return "未命名".into();
    }
    // SMB servers may store names on filesystems limited to 255 UTF-8 bytes.
    // Reserve room for backup prefixes and keep truncated names deterministic.
    const MAX_BYTES: usize = 240;
    const MAX_CHARS: usize = 140;
    if out.len() > MAX_BYTES || out.chars().count() > MAX_CHARS {
        use sha2::{Digest, Sha256};
        let suffix = format!("-{}", hex::encode(&Sha256::digest(out.as_bytes())[..5]));
        let extension = out.rsplit_once('.').map(|(_, ext)| ext)
            .filter(|ext| !ext.is_empty() && ext.len() <= 16 && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
            .map(|ext| format!(".{ext}")).unwrap_or_default();
        let stem = &out[..out.len() - extension.len()];
        let budget = MAX_BYTES - suffix.len() - extension.len();
        let count = MAX_CHARS - suffix.len() - extension.len();
        let mut prefix = String::new();
        for c in stem.chars().take(count) {
            if prefix.len() + c.len_utf8() > budget { break; }
            prefix.push(c);
        }
        out = format!("{}{}{}", prefix.trim_end_matches([' ', '.']), suffix, extension);
    }
    out
}
pub fn video(path: &Path) -> bool {
    matches!(
        ext(path).as_str(),
        "mkv" | "mp4" | "avi" | "mov" | "m4v" | "ts" | "m2ts" | "webm" | "wmv" | "mpg" | "mpeg"
    )
}
pub fn subtitle(path: &Path) -> bool {
    matches!(
        ext(path).as_str(),
        "srt" | "ass" | "ssa" | "sub" | "idx" | "vtt"
    )
}
pub fn audio(path: &Path) -> bool {
    matches!(
        ext(path).as_str(),
        "flac" | "mp3" | "m4a" | "aac" | "wav" | "ogg" | "opus" | "ape" | "alac" | "aiff" | "dsf"
    )
}
pub fn media(path: &Path) -> bool {
    video(path) || audio(path)
}
pub fn ext(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}
pub fn validate_settings(settings: &Settings) -> Result<()> {
    crate::subtitles::language_pattern(&settings.subtitle_language_pattern)?;
    if !settings.musicbrainz_contact.trim().is_empty() {
        crate::musicbrainz::user_agent(&settings.musicbrainz_contact)?;
    }
    if !["keep", "delete", "backup"].contains(&settings.cue_source_disposition.as_str()) {
        return Err("errors.cue.originals.invalidDefault".into());
    }
    if !["copy", "move", "hardlink", "symlink"].contains(&settings.default_operation.as_str()) {
        return Err("无效默认整理方式".into());
    }
    if settings.language.is_empty()
        || settings.language.len() > 35
        || !settings
            .language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("语言请填写 BCP 47 代码，例如 zh-CN、ja-JP".into());
    }
    crate::naming::previews(settings)?;
    let roots: Vec<_> = if settings.output_root.is_empty() {
        vec![
            &settings.tv_root,
            &settings.movie_root,
            &settings.music_root,
            &settings.extras_root,
            &settings.backup_root,
        ]
    } else {
        vec![
            &settings.output_root,
            &settings.extras_root,
            &settings.backup_root,
        ]
    }
    .into_iter()
    .filter(|s| !s.trim().is_empty())
    .map(|s| normalize(s))
    .collect::<Result<_>>()?;
    for (i, a) in roots.iter().enumerate() {
        for b in roots.iter().skip(i + 1) {
            if contained(a, b) || contained(b, a) {
                return Err("剧集、电影、附加文件和备份目录必须相互独立，不能包含彼此".into());
            }
            if let (Ok(ra), Ok(rb)) = (resolved(a), resolved(b)) {
                if contained(&ra, &rb) || contained(&rb, &ra) {
                    return Err("目录解析到相同或嵌套位置".into());
                }
            }
        }
    }
    Ok(())
}
pub fn commit_no_replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let source = resolved_entry(source)?;
        let target = resolved_entry(target)?;
        let a: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, target).map_err(|e| e.to_string())?;
        fs::remove_file(source).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_unicode_names_fit_smb_components_without_collisions_or_lost_extensions() {
        let title = "架空演奏者、".repeat(24);
        let first = clean_name(&format!("{title} Album One (2024)"));
        let second = clean_name(&format!("{title} Album Two (2024)"));
        assert!(first.len() <= 240);
        assert!(first.chars().count() <= 140);
        assert_ne!(first, second);
        assert_eq!(clean_name(&first), first);
        for extension in ["flac", "m4a", "mkv"] {
            let file = clean_name(&format!("01 - {}.{extension}", "架空音楽🎵".repeat(50)));
            assert!(file.len() <= 240);
            assert!(file.ends_with(&format!(".{extension}")));
            assert!(format!("000-{file}").len() <= 255);
        }
        assert_eq!(clean_name("Example Album (2024)"), "Example Album (2024)");
        assert_eq!(clean_name("01 - Fictional Track.flac"), "01 - Fictional Track.flac");
    }

    #[test]
    fn cue_original_default_is_compatible_and_validated() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.cue_source_disposition, "keep");
        for value in ["keep", "delete", "backup"] {
            settings.cue_source_disposition = value.into();
            validate_settings(&settings).unwrap();
            let restored: Settings = serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
            assert_eq!(restored.cue_source_disposition, value);
        }
        settings.cue_source_disposition = "move".into();
        assert_eq!(validate_settings(&settings).unwrap_err(), "errors.cue.originals.invalidDefault");
    }

    #[test]
    fn names_are_portable() {
        assert_eq!(clean_name("CON"), "_CON");
        assert_eq!(clean_name("架空の役名: X?"), "架空の役名_ X_");
        assert_eq!(clean_name("a. "), "a");
    }
    #[test]
    fn root_containment_has_boundary() {
        assert!(!contained(Path::new("/library2"), Path::new("/library")));
        assert!(contained(Path::new("/library/TV"), Path::new("/library")));
    }
    #[test]
    fn refuse_overwrite_on_commit() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        fs::write(&a, b"new").unwrap();
        fs::write(&b, b"old").unwrap();
        assert!(commit_no_replace(&a, &b).is_err());
        assert_eq!(fs::read(b).unwrap(), b"old");
    }
    #[cfg(windows)]
    #[test]
    fn smb_and_unc() {
        assert_eq!(
            display(&normalize("smb://nas/media/%E5%8A%A8%E6%BC%AB").unwrap()),
            r"\\nas\media\动漫"
        );
        assert!(normalize("smb://user:pass@nas/media").is_err());
    }
}
