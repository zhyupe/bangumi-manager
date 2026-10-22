use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    path::PathBuf,
    sync::MutexGuard,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

pub struct Store {
    http: std::sync::OnceLock<reqwest::blocking::Client>,
    pub diagnostics: crate::diagnostics::Diagnostics,
    pub conn: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub running: AtomicBool,
    pub queue_lock: Mutex<()>,
    pub queue_worker: AtomicBool,
    pub cancel: AtomicBool,
    // Serialize task start, stop requests and persisted progress.
    pub active_plan: Mutex<Option<String>>,
    pub library_scan: Mutex<()>,
    pub probe_running: AtomicBool,
    pub probe_cancel: AtomicBool,
    pub cue_running: AtomicBool,
    pub cue_cancel: AtomicBool,
}
impl Store {
    pub fn open(path: PathBuf) -> Result<Arc<Self>> {
        let diagnostics = crate::diagnostics::Diagnostics::new(path.join("logs"));
        let mut startup = diagnostics.span("app.open_database", serde_json::json!({}));
        let _scope = startup.enter();
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let conn = Connection::open(path.join("catalog.sqlite3")).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;
            CREATE TABLE IF NOT EXISTS documents (collection TEXT NOT NULL, id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(collection,id));
            CREATE TABLE IF NOT EXISTS api_cache (key TEXT PRIMARY KEY, data TEXT NOT NULL, updated INTEGER NOT NULL);
            PRAGMA user_version=1;").map_err(|e| e.to_string())?;
        let store = Arc::new(Self {
            http: std::sync::OnceLock::new(),
            diagnostics,
            conn: Mutex::new(conn),
            data_dir: path,
            running: AtomicBool::new(false),
            queue_lock: Mutex::new(()),
            queue_worker: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            active_plan: Mutex::new(None),
            library_scan: Mutex::new(()),
            probe_running: AtomicBool::new(false),
            probe_cancel: AtomicBool::new(false),
            cue_running: AtomicBool::new(false),
            cue_cancel: AtomicBool::new(false),
        });
        for mut plan in store.list::<Plan>("plans")? {
            if plan.state == "cancelling" {
                plan.mark_cancelled();
                store.put("plans", &plan.id, &plan)?;
            } else if plan.state == "running" {
                plan.state = "paused".into();
                store.put("plans", &plan.id, &plan)?;
            }
        }
        let plans = store.list::<Plan>("plans")?;
        let mut queue = store
            .get::<QueueState>("queue", "main")?
            .unwrap_or_default();
        queue
            .ids
            .retain(|id| plans.iter().any(|p| p.id == *id && p.state == "queued"));
        for plan in plans.iter().filter(|p| p.state == "queued") {
            if !queue.ids.contains(&plan.id) {
                queue.ids.push(plan.id.clone());
            }
        }
        if !queue.ids.is_empty() {
            queue.paused = true;
            queue.error = "应用已重启，请确认后继续队列".into();
        }
        store.put("queue", "main", &queue)?;
        for mut work in store.list::<Work>("works")? {
            if work.seasons.is_empty() && work.kind != "movie" {
                for binding in &work.bindings {
                    let numbers: std::collections::BTreeSet<_> = if binding.provider == "tmdb" {
                        work.episodes
                            .iter()
                            .filter(|e| e.binding_id == binding.id)
                            .map(|e| e.season)
                            .collect()
                    } else {
                        [binding.season].into_iter().collect()
                    };
                    for number in numbers {
                        work.seasons.push(Season {
                            binding_id: binding.id.clone(),
                            number,
                            remote_number: if binding.provider == "tmdb" {
                                number
                            } else {
                                1
                            },
                            title: if binding.provider == "tmdb" {
                                format!("Season {number:02}")
                            } else {
                                binding.source_title.clone()
                            },
                            ..Default::default()
                        });
                    }
                }
                store.put("works", &work.id, &work)?;
            }
        }
        startup.finish("ok");
        Ok(store)
    }
    pub fn http_client(&self) -> Result<reqwest::blocking::Client> {
        if let Some(client) = self.http.get() {
            return Ok(client.clone());
        }
        let client = self
            .diagnostics
            .measure("http.client", serde_json::json!({}), || {
                reqwest::blocking::Client::builder()
                    .connect_timeout(std::time::Duration::from_secs(8))
                    .timeout(std::time::Duration::from_secs(25))
                    .user_agent(concat!("BangumiManager/", env!("CARGO_PKG_VERSION"), " (desktop personal library organizer)"))
                    .build()
                    .map_err(|e| e.to_string())
            })?;
        let _ = self.http.set(client.clone());
        Ok(self.http.get().cloned().unwrap_or(client))
    }
    // The lock covers SQLite calls only, never network/file operations or JSON decoding.
    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.diagnostics.database("db.lock_wait", || {
            self.conn.lock().map_err(|e| e.to_string())
        })
    }
    pub fn put<T: Serialize>(&self, collection: &str, id: &str, item: &T) -> Result<()> {
        self.diagnostics.database(&format!("db.put.{collection}"), || {

        let json = serde_json::to_string(item).map_err(|e| e.to_string())?;
        let conn = self.connection()?;
        crate::catalog::validate_document(&conn, collection, id, &json)?;
        conn.execute(
            "INSERT INTO documents VALUES (?1,?2,?3) ON CONFLICT(collection,id) DO UPDATE SET data=excluded.data",
            params![collection,id,json]
        ).map_err(|e| e.to_string())?;
        Ok(())

    })
    }
    pub fn get<T: DeserializeOwned>(&self, collection: &str, id: &str) -> Result<Option<T>> {
        self.diagnostics
            .database(&format!("db.get.{collection}"), || {
                let value: Option<String> = self
                    .connection()?
                    .query_row(
                        "SELECT data FROM documents WHERE collection=?1 AND id=?2",
                        params![collection, id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                value
                    .map(|v| serde_json::from_str(&v).map_err(|e| e.to_string()))
                    .transpose()
            })
    }
    pub fn list<T: DeserializeOwned>(&self, collection: &str) -> Result<Vec<T>> {
        self.diagnostics
            .database(&format!("db.list.{collection}"), || {
                let conn = self.connection()?;
                let mut stmt = conn
                    .prepare("SELECT data FROM documents WHERE collection=?1 ORDER BY id")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([collection], |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                let values = rows
                    .collect::<std::result::Result<Vec<String>, _>>()
                    .map_err(|e| e.to_string())?;
                drop(stmt);
                drop(conn);
                values
                    .into_iter()
                    .map(|v| serde_json::from_str(&v).map_err(|e| e.to_string()))
                    .collect()
            })
    }
    pub fn folders_for_work(&self, work_id: &str) -> Result<Vec<String>> {
        self.diagnostics.database("db.folders_for_work", || {
            let conn = self.connection()?;
            let mut stmt = conn
                .prepare("SELECT id FROM documents WHERE collection='folders' AND data=?1 ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([serde_json::to_string(work_id).unwrap()], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<std::result::Result<Vec<String>, _>>()
                .map_err(|e| e.to_string())
        })
    }

    pub fn remove(&self, collection: &str, id: &str) -> Result<()> {
        self.diagnostics
            .database(&format!("db.remove.{collection}"), || {
                self.connection()?
                    .execute(
                        "DELETE FROM documents WHERE collection=?1 AND id=?2",
                        params![collection, id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
    }
    pub fn clear(&self, collection: &str) -> Result<()> {
        self.diagnostics
            .database(&format!("db.clear.{collection}"), || {
                self.connection()?
                    .execute("DELETE FROM documents WHERE collection=?1", [collection])
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
    }
    pub fn put_documents(&self, documents: Vec<(&str, String, String)>) -> Result<()> {
        self.diagnostics.database("db.transaction", || {
            let mut conn = self.connection()?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            for (collection, id, data) in documents {
                crate::catalog::validate_document(&tx, collection, &id, &data)?;
                tx.execute("INSERT INTO documents VALUES (?1,?2,?3) ON CONFLICT(collection,id) DO UPDATE SET data=excluded.data", params![collection,id,data]).map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())
        })
    }
    pub fn settings(&self) -> Result<Settings> {
        Ok(self.get("settings", "main")?.unwrap_or_default())
    }
    pub fn cache(&self, key: &str, stale: bool) -> Result<Option<serde_json::Value>> {
        self.diagnostics.database("db.cache", || {
            let conn = self.connection()?;
            let row: Option<(String, i64)> = conn
                .query_row(
                    "SELECT data,updated FROM api_cache WHERE key=?1",
                    [key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            drop(conn);
            match row {
                Some((v, t)) if stale || chrono::Utc::now().timestamp() - t < 86400 => {
                    serde_json::from_str(&v)
                        .map(Some)
                        .map_err(|e| e.to_string())
                }
                _ => Ok(None),
            }
        })
    }
    pub fn cache_put(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let serialized = value.to_string();
        self.diagnostics.database("db.cache_put", || {

        self.connection()?.execute(
            "INSERT INTO api_cache VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET data=excluded.data,updated=excluded.updated",
            params![key,serialized,chrono::Utc::now().timestamp()]
        ).map_err(|e| e.to_string())?;
        Ok(())

    })
    }
}
