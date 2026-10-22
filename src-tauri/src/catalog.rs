use crate::{db::Store, model::*};
use rusqlite::{params, Connection};
use std::sync::atomic::Ordering;

// Called under the same database lock as the write. Slow metadata requests and
// previews that started before deletion must not recreate a deleted title.
pub(crate) fn validate_document(
    conn: &Connection,
    collection: &str,
    id: &str,
    data: &str,
) -> Result<()> {
    let predicate = match collection {
        "works" => "d.id IN (?1, json_extract(?2, '$.parent_id'))",
        "folders" => "d.id=json_extract(?2, '$')",
        "group_asset_identity" => "d.id=json_extract(?2, '$.work_id')",
        "plans" | "organize_previews" => "COALESCE(json_extract(?2, '$.state'), '') NOT IN ('complete', 'cancelled') AND d.id IN (
            SELECT json_extract(r.value, '$.work_id') FROM json_each(?2, '$.rows') r
            UNION SELECT json_extract(w.value, '$.id') FROM json_each(?2, '$.works') w)",
        _ => return Ok(()),
    };
    let deleted: bool = conn.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM documents d WHERE d.collection='deleted_works' AND ({predicate}))"),
        params![id, data], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    if deleted {
        Err("作品已删除，请重新添加或重新生成整理计划".into())
    } else {
        Ok(())
    }
}

pub fn delete_work(store: &Store, id: &str) -> Result<()> {
    store.diagnostics.measure("catalog.delete_work", serde_json::json!({"work_id": id}), || {
        let _queue = store.queue_lock.lock().map_err(|e| e.to_string())?;
        let _scan = store.library_scan.lock().map_err(|e| e.to_string())?;
        let active = store.active_plan.lock().map_err(|e| e.to_string())?;
        if active.is_some() || store.running.load(Ordering::SeqCst) {
            return Err("复制任务执行中，请先暂停任务".into());
        }
        store.diagnostics.database("db.delete_work", || {
            let mut conn = store.connection()?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let exists = |query: &str| -> Result<bool> {
                tx.query_row(query, [id], |r| r.get(0)).map_err(|e| e.to_string())
            };
            if !exists("SELECT EXISTS (SELECT 1 FROM documents WHERE collection='works' AND id=?1)")? {
                return Ok(());
            }
            for (query, reason) in [
                ("SELECT EXISTS (SELECT 1 FROM documents d, json_each(d.data, '$.assets') a
                    WHERE d.collection='index' AND d.id='main' AND json_extract(a.value, '$.work_id')=?1)
                    OR EXISTS (SELECT 1 FROM documents d, json_each(d.data, '$.resources') r
                    WHERE d.collection='index' AND d.id='main' AND json_extract(r.value, '$.asset.work_id')=?1)
                    OR EXISTS (SELECT 1 FROM documents WHERE collection='group_asset_identity' AND json_extract(data, '$.work_id')=?1)",
                    "此作品已有关联资源，不能删除；离线资源也会保留关联"),
                ("SELECT EXISTS (SELECT 1 FROM documents WHERE collection='folders' AND json_extract(data, '$')=?1)",
                    "此作品已关联资料库目录，不能删除，请先核对目录关联"),
                ("SELECT EXISTS (SELECT 1 FROM documents d WHERE d.collection='plans'
                    AND COALESCE(json_extract(d.data, '$.state'), '') NOT IN ('complete', 'cancelled')
                    AND (EXISTS (SELECT 1 FROM json_each(d.data, '$.rows') r WHERE json_extract(r.value, '$.work_id')=?1)
                    OR EXISTS (SELECT 1 FROM json_each(d.data, '$.works') w WHERE json_extract(w.value, '$.id')=?1)))",
                    "此作品有未完成的任务，请先取消或完成任务后再删除"),
                ("SELECT EXISTS (SELECT 1 FROM documents d, json_each(d.data, '$.rows') r
                    WHERE d.collection='plans' AND json_extract(r.value, '$.work_id')=?1 AND (
                    json_extract(r.value, '$.state')='complete' OR json_extract(r.value, '$.backups_prepared')=1
                    OR json_extract(r.value, '$.source_cleanup_pending')=1
                    OR COALESCE(json_extract(r.value, '$.rename.phase'), '') NOT IN ('', 'pending')))",
                    "此作品仍有已执行的文件操作或恢复记录，不能删除"),
                ("SELECT EXISTS (SELECT 1 FROM documents WHERE collection='works' AND id<>?1 AND json_extract(data, '$.parent_id')=?1)",
                    "此作品仍被其他作品关联，请先解除关联后再删除"),
            ] {
                if exists(query)? { return Err(reason.into()); }
            }
            tx.execute("INSERT INTO documents (collection,id,data) VALUES ('deleted_works',?1,?2)
                ON CONFLICT(collection,id) DO UPDATE SET data=excluded.data", params![id, serde_json::json!({"deleted_at": now()}).to_string()])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM documents WHERE collection IN ('works','episode_order') AND id=?1", [id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM documents WHERE collection='organize_previews' AND (
                EXISTS (SELECT 1 FROM json_each(documents.data, '$.rows') r WHERE json_extract(r.value, '$.work_id')=?1)
                OR EXISTS (SELECT 1 FROM json_each(documents.data, '$.works') w WHERE json_extract(w.value, '$.id')=?1))", [id])
                .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        sync::{Arc, Barrier},
    };

    fn fixture() -> (tempfile::TempDir, Arc<Store>, Work) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("state")).unwrap();
        let work = Work {
            id: "unused".into(),
            title: "Fictional Moonlit Journey".into(),
            kind: "tv".into(),
            episodes: vec![Episode {
                id: "ep-1".into(),
                season: 1,
                number: 1,
                enabled: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        store.put("works", &work.id, &work).unwrap();
        (dir, store, work)
    }

    #[test]
    fn deleting_unused_title_preserves_files_history_and_unrelated_data() {
        let (dir, store, work) = fixture();
        let source = dir.path().join("Unmatched.mkv");
        fs::write(&source, b"source media").unwrap();
        let mut other = work.clone();
        other.id = "other".into();
        store.put("works", &other.id, &other).unwrap();
        store.put("episode_order", &work.id, &true).unwrap();
        store.put("episode_order", &other.id, &true).unwrap();
        let preview = json!({"state":"draft", "works":[work], "rows":[]});
        let history = json!({"state":"cancelled", "works":[work], "rows":[]});
        store
            .put("organize_previews", "selected", &preview)
            .unwrap();
        store
            .put("organize_previews", "unrelated", &json!({"works":[other]}))
            .unwrap();
        store.put("plans", "history", &history).unwrap();
        let backup = json!({"plan_id":"history", "state":"retained"});
        store.put("backups", "saved", &backup).unwrap();
        delete_work(&store, &work.id).unwrap();
        delete_work(&store, &work.id).unwrap();
        assert!(store.get::<Work>("works", &work.id).unwrap().is_none());
        assert!(store
            .get::<bool>("episode_order", &work.id)
            .unwrap()
            .is_none());
        assert!(store
            .get::<serde_json::Value>("organize_previews", "selected")
            .unwrap()
            .is_none());
        assert!(store.get::<Work>("works", &other.id).unwrap().is_some());
        assert_eq!(
            store.get::<bool>("episode_order", &other.id).unwrap(),
            Some(true)
        );
        assert!(store
            .get::<serde_json::Value>("organize_previews", "unrelated")
            .unwrap()
            .is_some());
        assert_eq!(
            store.get::<serde_json::Value>("plans", "history").unwrap(),
            Some(history)
        );
        assert_eq!(
            store.get::<serde_json::Value>("backups", "saved").unwrap(),
            Some(backup)
        );
        assert_eq!(fs::read(&source).unwrap(), b"source media");
    }

    #[test]
    fn deletion_rechecks_resource_directory_task_and_parent_associations() {
        let mut cases = vec![
            (
                "index",
                "main",
                json!({"available":false,"assets":[{"work_id":"unused"}]}),
            ),
            (
                "index",
                "main",
                json!({"resources":[{"available":false,"asset":{"work_id":"unused"}}]}),
            ),
            ("folders", "registered", json!("unused")),
            (
                "group_asset_identity",
                "mapped",
                json!({"work_id":"unused","episode_ids":["ep-1"]}),
            ),
            ("works", "child", json!({"parent_id":"unused"})),
            (
                "plans",
                "cancelled",
                json!({"state":"cancelled","rows":[{"work_id":"unused","state":"complete","extra":true}]}),
            ),
            (
                "plans",
                "cancelled",
                json!({"state":"cancelled","rows":[{"work_id":"unused","backups_prepared":true}]}),
            ),
        ];
        for state in ["draft", "queued", "running", "paused", "failed", "unknown"] {
            cases.push((
                "plans",
                "unfinished",
                json!({"state":state,"rows":[{"work_id":"unused"}]}),
            ));
        }
        for (collection, key, record) in cases {
            let (_dir, store, work) = fixture();
            store.put(collection, key, &record).unwrap();
            assert!(
                delete_work(&store, &work.id).is_err(),
                "{collection} {record}"
            );
            assert!(store.get::<Work>("works", &work.id).unwrap().is_some());
            assert!(store
                .get::<serde_json::Value>("deleted_works", &work.id)
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn late_writes_cannot_resurrect_a_deleted_title_or_create_stale_tasks() {
        let (_dir, store, work) = fixture();
        delete_work(&store, &work.id).unwrap();
        let reopened = Store::open(store.data_dir.clone()).unwrap();
        for (collection, key, data) in [
            ("works", "unused", json!(work)),
            ("folders", "late-folder", json!("unused")),
            (
                "group_asset_identity",
                "late-asset",
                json!({"work_id":"unused"}),
            ),
            (
                "plans",
                "late-plan",
                json!({"state":"draft","works":[work]}),
            ),
            (
                "organize_previews",
                "late-preview",
                json!({"state":"draft","rows":[{"work_id":"unused"}]}),
            ),
        ] {
            assert!(reopened
                .put(collection, key, &data)
                .unwrap_err()
                .contains("作品已删除"));
        }
        assert!(reopened
            .put_documents(vec![
                ("episode_order", work.id.clone(), "true".into()),
                (
                    "works",
                    work.id.clone(),
                    serde_json::to_string(&work).unwrap()
                ),
            ])
            .is_err());
        assert!(reopened
            .get::<bool>("episode_order", &work.id)
            .unwrap()
            .is_none());
        let mut newly_added = work.clone();
        newly_added.id = id();
        reopened
            .put("works", &newly_added.id, &newly_added)
            .unwrap();
    }

    #[test]
    fn concurrent_task_creation_and_deletion_cannot_both_succeed() {
        let (_dir, store, work) = fixture();
        let barrier = Arc::new(Barrier::new(2));
        let writer = store.clone();
        let ready = barrier.clone();
        let handle = std::thread::spawn(move || {
            ready.wait();
            writer.put(
                "plans",
                "new-plan",
                &json!({"state":"draft","rows":[{"work_id":"unused"}]}),
            )
        });
        barrier.wait();
        let deleted = delete_work(&store, &work.id);
        let created = handle.join().unwrap();
        assert_ne!(deleted.is_ok(), created.is_ok());
        assert_eq!(
            store.get::<Work>("works", &work.id).unwrap().is_some(),
            created.is_ok()
        );
    }
}
