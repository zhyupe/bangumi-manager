use crate::{
    db::Store,
    model::{Result, Signature, Work},
    paths, resources,
};
use serde::Serialize;
use std::{
    fs,
    path::{Component, Path},
};

#[derive(Serialize)]
pub struct ExtraFile {
    pub path: String,
    relative_path: String,
    signature: Signature,
}
#[derive(Serialize)]
pub struct ExtraFiles {
    pub path: String,
    pub files: Vec<ExtraFile>,
    errors: Vec<String>,
}
pub fn list(store: &Store, work_id: &str) -> Result<ExtraFiles> {
    let work = store
        .get::<Work>("works", work_id)?
        .ok_or("Work not found")?;
    let settings = store.settings()?;
    if settings.extras_root.trim().is_empty() {
        return Err("Extra files directory is not configured".into());
    }
    let root = paths::normalize(&settings.extras_root)?;
    if !fs::metadata(&root).map_err(|e| e.to_string())?.is_dir() {
        return Err("Extra files directory is unavailable".into());
    }
    read(&root, &work.folder_name)
}
fn read(root: &Path, folder: &str) -> Result<ExtraFiles> {
    let relative = Path::new(folder);
    if folder.is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Invalid work folder".into());
    }
    let directory = root.join(relative);
    let mut result = ExtraFiles {
        path: paths::display(&directory),
        files: vec![],
        errors: vec![],
    };
    match fs::symlink_metadata(&directory) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(e) => return Err(e.to_string()),
        Ok(meta) if !meta.is_dir() || linked(&meta) => {
            return Err("Invalid extra files directory".into())
        }
        _ => {}
    }
    let resolved_root = paths::resolved(root)?;
    if !paths::contained(&paths::resolved(&directory)?, &resolved_root) {
        return Err("Extra files directory is outside its root".into());
    }
    for entry in walkdir::WalkDir::new(&directory)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| fs::symlink_metadata(e.path()).is_ok_and(|m| !linked(&m)))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                result.errors.push(e.to_string());
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        match resources::regular(entry.path(), Some(&directory)) {
            Ok(signature) => result.files.push(ExtraFile {
                path: paths::display(entry.path()),
                relative_path: paths::display(
                    entry
                        .path()
                        .strip_prefix(&directory)
                        .map_err(|e| e.to_string())?,
                ),
                signature,
            }),
            Err(error) => result.errors.push(error),
        }
    }
    result
        .files
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(result)
}
fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_only_work_extras_and_keeps_nested_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sample/Book A")).unwrap();
        fs::create_dir_all(dir.path().join("sample/Book B")).unwrap();
        fs::create_dir_all(dir.path().join("other")).unwrap();
        for name in [
            "sample/Book A/page.jpg",
            "sample/Book B/page.jpg",
            "other/hidden.txt",
        ] {
            fs::write(dir.path().join(name), b"fixture").unwrap();
        }
        let result = read(dir.path(), "sample").unwrap();
        assert_eq!(result.files.len(), 2);
        assert_ne!(result.files[0].relative_path, result.files[1].relative_path);
        assert_eq!(result.files[0].signature.size, 7);
        assert!(read(dir.path(), "../other").is_err());
        assert!(read(dir.path(), "absent").unwrap().files.is_empty());
    }
}
