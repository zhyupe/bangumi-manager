use rusqlite::{Connection, OpenFlags};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Keep the existing catalog in place when the application identifier changes.
/// In-place reuse also preserves SQLite WAL files and absolute artwork/cache paths.
pub(crate) fn resolve(preferred: PathBuf) -> io::Result<PathBuf> {
    if preferred.join("catalog.sqlite3").try_exists()? {
        return Ok(preferred);
    }
    let Some(parent) = preferred.parent() else {
        return Ok(preferred);
    };
    if !parent.try_exists()? {
        return Ok(preferred);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let path = entry.path();
        if path == preferred
            || !entry
                .file_name()
                .to_string_lossy()
                .ends_with(".bangumi-manager")
            || !entry.file_type()?.is_dir()
            || crate::paths::is_link(&fs::symlink_metadata(&path)?)
        {
            continue;
        }
        let catalog = path.join("catalog.sqlite3");
        if !catalog.try_exists()? {
            continue;
        }
        if !fs::symlink_metadata(&catalog)?.is_file()
            || crate::paths::is_link(&fs::symlink_metadata(&catalog)?)
        {
            return Err(io::Error::other("Existing application catalog is not a regular file. Set BANGUMI_MANAGER_DATA_DIR explicitly."));
        }
        validate_catalog(&catalog)?;
        candidates.push(path);
    }
    match candidates.len() {
        0 => Ok(preferred),
        1 => Ok(candidates.remove(0)),
        _ => Err(io::Error::other("Multiple application catalogs found. Set BANGUMI_MANAGER_DATA_DIR to the intended directory.")),
    }
}

fn validate_catalog(path: &Path) -> io::Result<()> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(io::Error::other)?;
    connection
        .prepare("SELECT collection, id, data FROM documents LIMIT 0")
        .map_err(io::Error::other)?;
    connection
        .prepare("SELECT key, data, updated FROM api_cache LIMIT 0")
        .map_err(io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog(path: &Path) {
        fs::create_dir_all(path).unwrap();
        Connection::open(path.join("catalog.sqlite3")).unwrap().execute_batch(
            "CREATE TABLE documents(collection TEXT, id TEXT, data TEXT); CREATE TABLE api_cache(key TEXT, data TEXT, updated INTEGER);"
        ).unwrap();
    }
    #[test]
    fn fresh_install_and_existing_preferred_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let preferred = dir.path().join("io.github.example.bangumi-manager");
        assert_eq!(resolve(preferred.clone()).unwrap(), preferred);
        assert!(!preferred.exists());
        catalog(&preferred);
        catalog(&dir.path().join("org.example.bangumi-manager"));
        assert_eq!(resolve(preferred.clone()).unwrap(), preferred);
    }
    #[test]
    fn reuses_catalog_without_copying_or_renaming_files() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("org.example.bangumi-manager");
        catalog(&old);
        fs::write(old.join("artwork-marker"), b"cached").unwrap();
        let preferred = dir.path().join("io.github.example.bangumi-manager");
        assert_eq!(resolve(preferred.clone()).unwrap(), old);
        assert!(!preferred.exists());
        assert_eq!(fs::read(old.join("artwork-marker")).unwrap(), b"cached");
    }
    #[test]
    fn ignores_unrelated_apps_and_refuses_ambiguous_or_invalid_catalogs() {
        let dir = tempfile::tempdir().unwrap();
        let preferred = dir.path().join("io.github.example.bangumi-manager");
        catalog(&dir.path().join("unrelated-app"));
        assert_eq!(resolve(preferred.clone()).unwrap(), preferred);
        catalog(&dir.path().join("org.example.bangumi-manager"));
        catalog(&dir.path().join("net.example.bangumi-manager"));
        assert!(resolve(preferred.clone()).is_err());
        let invalid = dir.path().join("invalid.bangumi-manager");
        fs::create_dir(&invalid).unwrap();
        fs::write(invalid.join("catalog.sqlite3"), b"not a catalog").unwrap();
        assert!(resolve(preferred).is_err());
    }
}
