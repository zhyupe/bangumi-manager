use crate::{model::Result, paths};
use std::{fs, io::ErrorKind, path::PathBuf};

#[derive(Debug, PartialEq)]
pub struct Target {
    pub path: PathBuf,
    pub directory: bool,
    pub fallback: bool,
}

pub fn resolve(raw: &str) -> Result<Target> {
    let requested = paths::normalize(raw)?;
    for path in requested.ancestors() {
        match fs::metadata(path) {
            Ok(metadata) => {
                let fallback = path != requested;
                if !metadata.is_dir() && (fallback || !metadata.is_file()) {
                    return Err("路径的父级不是目录，无法在文件管理器中打开".into());
                }
                return Ok(Target {
                    path: path.to_path_buf(),
                    directory: metadata.is_dir(),
                    fallback,
                });
            }
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("无法访问路径：{error}")),
        }
    }
    Err("路径及其父目录均不可用，请检查磁盘或网络共享连接".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_files_and_opens_directories_without_modifying_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("虚构片段 [01], & sample.mkv");
        fs::write(&file, b"sample").unwrap();
        assert_eq!(
            resolve(&paths::display(&file)).unwrap(),
            Target {
                path: file.clone(),
                directory: false,
                fallback: false,
            }
        );
        assert_eq!(
            resolve(&paths::display(dir.path())).unwrap(),
            Target {
                path: dir.path().to_path_buf(),
                directory: true,
                fallback: false,
            }
        );
        assert_eq!(fs::read(file).unwrap(), b"sample");
    }

    #[test]
    fn missing_targets_open_nearest_existing_directory_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("future").join("season").join("EP01.mkv");
        assert_eq!(
            resolve(&paths::display(&target)).unwrap(),
            Target {
                path: dir.path().to_path_buf(),
                directory: true,
                fallback: true,
            }
        );
        assert!(!target.parent().unwrap().exists());
    }

    #[test]
    fn rejects_non_paths_and_file_parents() {
        for raw in ["", "EP01.mkv", "https://example.invalid/file.mkv"] {
            assert!(resolve(raw).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve(&paths::display(&dir.path().join("..").join("other"))).is_err());
        let file = dir.path().join("file.mkv");
        fs::write(&file, b"sample").unwrap();
        assert!(resolve(&paths::display(&file.join("child.mkv"))).is_err());
    }

    #[test]
    fn accepts_file_urls() {
        let dir = tempfile::tempdir().unwrap();
        let url = url::Url::from_directory_path(dir.path()).unwrap();
        let target = resolve(url.as_str()).unwrap();
        assert!(target.directory);
        assert!(!target.fallback);
        assert_eq!(target.path, dir.path());
    }
}
