use super::*;
use std::path::PathBuf;

/// Refresh only explicit work directories, preserving the rest of the library snapshot.
pub(crate) fn index_works(store: &Store, works: &[(Work, Vec<PathBuf>)]) -> Result<LibraryIndex> {
    store.diagnostics.measure_with(
        "library.scan_related",
        serde_json::json!({"works": works.len()}),
        |span| {
            if store.list::<Plan>("plans")?.iter().any(|p| {
                crate::renaming::in_progress(p)
                    && p.rows.iter().any(|r| {
                        r.state != "complete" && works.iter().any(|(w, _)| w.id == r.work_id)
                    })
            }) {
                return Err("重命名尚未完成，请继续任务后再扫描资料库".into());
            }
            let _scan = store.diagnostics.measure(
                "library.scan.lock_wait",
                serde_json::json!({}),
                || store.library_scan.lock().map_err(|e| e.to_string()),
            )?;
            let settings = store.settings()?;
            let mut previous = store
                .get::<LibraryIndex>("index", "main")?
                .unwrap_or_default();
            let mut folders: BTreeMap<String, (PathBuf, &Work)> = BTreeMap::new();
            for (work, targets) in works {
                let known = store.folders_for_work(&work.id)?;
                for folder in known
                    .iter()
                    .map(|raw| paths::normalize(raw))
                    .chain(targets.iter().cloned().map(Ok))
                {
                    let folder = folder?;
                    let key = paths::key(&folder);
                    if folders
                        .get(&key)
                        .is_some_and(|(_, owner)| owner.id != work.id)
                    {
                        return Err("目录关联与所选作品不一致，请重新扫描并确认关联".into());
                    }
                    folders.insert(key, (folder, work));
                }
            }
            span.field("folders", folders.len());
            // An old asset without a directory association must not silently disappear.
            for asset in previous
                .assets
                .iter()
                .filter(|a| works.iter().any(|(w, _)| w.id == a.work_id))
            {
                if !folders.values().any(|(dir, work)| {
                    work.id == asset.work_id && paths::contained(Path::new(&asset.path), dir)
                }) {
                    return Err("资源目录关联缺失，请重新扫描并确认关联".into());
                }
            }
            let mut fresh = LibraryIndex {
                available: true,
                ..Default::default()
            };
            let mut assets = Vec::new();
            for (dir, work) in folders.values() {
                let root = crate::resources::root_for(&settings, dir)?;
                paths::safe_target(dir, &root)?;
                let owner = store.get::<String>("folders", &paths::key(dir))?;
                if owner.as_ref().is_some_and(|id| id != &work.id) {
                    return Err("目录关联与所选作品不一致，请重新扫描并确认关联".into());
                }
                match fs::symlink_metadata(dir) {
                    Ok(m) if m.is_dir() && !paths::is_link(&m) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::NotFound
                            && !previous
                                .assets
                                .iter()
                                .any(|a| paths::contained(Path::new(&a.path), dir))
                            && !previous
                                .resources
                                .iter()
                                .any(|r| paths::contained(Path::new(&r.path), dir)) =>
                    {
                        continue
                    }
                    _ => {
                        return Err(format!(
                            "相关作品目录不可用，请检查连接或重新扫描：{}",
                            paths::display(dir)
                        ))
                    }
                }
                // Folder keys are case folded; keep filesystem spelling in asset identities.
                let actual = PathBuf::from(paths::display(
                    &fs::canonicalize(dir).map_err(|e| e.to_string())?,
                ));
                let nested = !settings.output_root.is_empty() || work.kind == "music";
                if !index_folder(
                    store,
                    &settings,
                    &actual,
                    &work.kind,
                    nested,
                    Some(&work.id),
                    &mut fresh,
                    &mut assets,
                )? || !fresh.errors.is_empty()
                {
                    return Err(fresh.errors.join("\n"));
                }
            }
            let in_scope = |raw: &str| {
                folders
                    .values()
                    .any(|(dir, _)| paths::contained(Path::new(raw), dir))
            };
            let mut seen = HashSet::new();
            assets.retain(|a| seen.insert(paths::key(Path::new(&a.path))));
            let by_path: HashMap<_, _> = assets
                .iter()
                .map(|a| (paths::key(Path::new(&a.path)), a))
                .collect();
            seen.clear();
            fresh
                .resources
                .retain(|r| seen.insert(paths::key(Path::new(&r.path))));
            for resource in &mut fresh.resources {
                resource.asset = by_path
                    .get(&paths::key(Path::new(&resource.path)))
                    .map(|a| (*a).clone());
            }
            span.field("resources", fresh.resources.len());
            span.field("assets", assets.len());
            previous.assets.retain(|a| !in_scope(&a.path));
            previous.assets.extend(assets);
            previous.resources.retain(|r| !in_scope(&r.path));
            previous.resources.extend(fresh.resources);
            previous.unbound.retain(|f| !in_scope(&f.path));
            previous.unbound.extend(fresh.unbound);
            // A partial refresh cannot establish whole-library availability or scan time.
            store.diagnostics.measure(
                "library.scan.persist",
                serde_json::json!({"scope": "related"}),
                || store.put("index", "main", &previous),
            )?;
            Ok(previous)
        },
    )
}
