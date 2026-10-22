use std::path::{Path, PathBuf};

/// Keep the tree below the outermost extras directory. Only the packaging
/// directory (bonus/extra/extras) is omitted; CD and scan names carry meaning.
pub fn relative_path(source: &Path) -> Option<PathBuf> {
    let directory = source
        .parent()?
        .ancestors()
        .filter(|parent| {
            parent.file_name().is_some_and(|name| {
                matches!(
                    name.to_string_lossy().to_ascii_lowercase().as_str(),
                    "bonus" | "extra" | "extras" | "cd" | "cds" | "scan" | "scans"
                )
            })
        })
        .last()?;
    let name = directory
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let root = if matches!(name.as_str(), "bonus" | "extra" | "extras") {
        directory
    } else {
        directory.parent()?
    };
    source.strip_prefix(root).ok().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Store, model::SourceFile, paths, scan};
    use std::fs;

    #[test]
    fn scans_all_extra_contents_and_recomputes_the_hint_on_rescan() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("incoming");
        let store = Store::open(temp.path().join("state")).unwrap();
        let expected = [
            ("Bonus/Disc 1/cover.jpg", Some("Disc 1/cover.jpg")),
            ("Bonus/Disc 2/cover.jpg", Some("Disc 2/cover.jpg")),
            ("Extras/Interview/OVA01.mkv", Some("Interview/OVA01.mkv")),
            ("Extras/Interview/OVA01.srt", Some("Interview/OVA01.srt")),
            ("Scans/Book/01.jpg", Some("Scans/Book/01.jpg")),
            ("CDs/Disc 1/01.flac", Some("CDs/Disc 1/01.flac")),
            ("BonusFeature/S01E01.mkv", None),
        ];
        for (relative, _) in expected {
            let file = input.join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, b"fixture").unwrap();
        }
        let first = scan::scan_sources(&store, vec![paths::display(&input)], |_| {}).unwrap();
        assert!(first.errors.is_empty());
        assert_eq!(first.files.len(), expected.len());
        for (relative, destination) in expected {
            let key = paths::key(&input.join(relative));
            let file = first
                .files
                .iter()
                .find(|file| paths::key(Path::new(&file.path)) == key)
                .unwrap();
            assert_eq!(
                file.extra_relative_path.as_ref().map(PathBuf::from),
                destination.map(PathBuf::from)
            );
            assert_eq!(
                file.hint,
                if destination.is_some() {
                    "extra"
                } else {
                    "episode"
                }
            );
        }
        let extra = first
            .files
            .iter()
            .find(|file| file.extra_relative_path.is_some())
            .unwrap();
        let mut legacy = serde_json::to_value(extra).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("extra_relative_path");
        legacy["hint"] = "episode".into();
        let legacy: SourceFile = serde_json::from_value(legacy).unwrap();
        assert!(legacy.extra_relative_path.is_none());
        store
            .put("sources", &paths::key(Path::new(&legacy.path)), &legacy)
            .unwrap();
        let rescanned = scan::scan_sources(&store, vec![legacy.path], |_| {}).unwrap();
        assert_eq!(
            rescanned.files[0].extra_relative_path,
            extra.extra_relative_path
        );
        assert_eq!(rescanned.files[0].hint, "extra");
    }

    #[test]
    fn matches_whole_directory_names_and_preserves_nested_structure() {
        for (source, expected) in [
            ("incoming/BoNuS/Disc 1/cover.jpg", "Disc 1/cover.jpg"),
            ("incoming/Extra/Art/cover.jpg", "Art/cover.jpg"),
            ("incoming/EXTRAS/cover.jpg", "cover.jpg"),
            ("incoming/CD/Disc 1/01.flac", "CD/Disc 1/01.flac"),
            ("incoming/CDs/Disc 2/01.flac", "CDs/Disc 2/01.flac"),
            ("incoming/Scan/cover.jpg", "Scan/cover.jpg"),
            ("incoming/Scans/Book/01.jpg", "Scans/Book/01.jpg"),
            (
                "incoming/Bonus/Scans/Extra/cover.jpg",
                "Scans/Extra/cover.jpg",
            ),
        ] {
            assert_eq!(
                relative_path(Path::new(source)),
                Some(PathBuf::from(expected)),
                "{source}"
            );
        }
        for source in [
            "incoming/BonusFeature/01.mkv",
            "incoming/Extract/01.mkv",
            "incoming/CD1/01.flac",
            "incoming/Discans/01.jpg",
            "incoming/scans.jpg",
        ] {
            assert!(relative_path(Path::new(source)).is_none(), "{source}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_share_named_extras_is_not_an_extras_directory() {
        assert!(relative_path(Path::new(r"\\server\extras\Title\01.mkv")).is_none());
        assert_eq!(
            relative_path(Path::new(r"\\server\media\Title\Extras\Book\01.jpg")),
            Some(PathBuf::from(r"Book\01.jpg"))
        );
    }
}
