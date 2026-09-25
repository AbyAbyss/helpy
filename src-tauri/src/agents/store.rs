//! Agents, batches and Helpy-kept reminders in SQLite, saved after every
//! step; also the ask conversation and the notes Helpy remembers.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use super::model::{Agent, Batch};
use super::tools::reminders::HelpyReminder;
use crate::ai::memory::Note;

pub struct Store {
    db: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS agents (id TEXT PRIMARY KEY, batch TEXT NOT NULL, created INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS batches (id TEXT PRIMARY KEY, created INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS reminders (id TEXT PRIMARY KEY, at INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS documents (key TEXT PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY AUTOINCREMENT, created INTEGER NOT NULL, text TEXT NOT NULL);",
        )?;
        Ok(Self { db: Mutex::new(db) })
    }

    pub fn save_agent(&self, a: &Agent) {
        let Ok(data) = serde_json::to_string(a) else {
            return;
        };
        let r = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO agents (id, batch, created, data) VALUES (?1, ?2, ?3, ?4)",
            params![a.id, a.batch, a.created, data],
        );
        if let Err(e) = r {
            log::error!("couldn't save agent {}: {e}", a.id);
        }
    }

    pub fn save_batch(&self, b: &Batch) {
        let Ok(data) = serde_json::to_string(b) else {
            return;
        };
        let _ = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO batches (id, created, data) VALUES (?1, ?2, ?3)",
            params![b.id, b.created, data],
        );
    }

    pub fn delete_agent(&self, id: &str) {
        let _ = self
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM agents WHERE id = ?1", params![id]);
    }

    fn rows<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Vec<T> {
        let db = self.db.lock().unwrap();
        let Ok(mut stmt) = db.prepare(sql) else {
            return Vec::new();
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(Result::ok)
                    .filter_map(|d| serde_json::from_str(&d).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn agents(&self) -> Vec<Agent> {
        self.rows("SELECT data FROM agents ORDER BY created")
    }

    pub fn batches(&self) -> Vec<Batch> {
        self.rows("SELECT data FROM batches ORDER BY created")
    }

    /// Forgets finished agents older than `before` (unix ms), and batches
    /// left empty.
    pub fn prune(&self, finished_before: i64, ids: &[String]) {
        let db = self.db.lock().unwrap();
        for id in ids {
            let _ = db.execute(
                "DELETE FROM agents WHERE id = ?1 AND created < ?2",
                params![id, finished_before],
            );
        }
        let _ = db.execute(
            "DELETE FROM batches WHERE id NOT IN (SELECT batch FROM agents)",
            [],
        );
    }

    /// One JSON document under a key (the ask conversation, for example).
    pub fn document<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT data FROM documents WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
    }

    pub fn save_document<T: serde::Serialize>(&self, key: &str, value: &T) {
        let Ok(data) = serde_json::to_string(value) else {
            return;
        };
        let r = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO documents (key, data) VALUES (?1, ?2)",
            params![key, data],
        );
        if let Err(e) = r {
            log::error!("couldn't save {key}: {e}");
        }
    }

    pub fn delete_document(&self, key: &str) {
        let _ = self
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM documents WHERE key = ?1", params![key]);
    }

    pub fn notes(&self) -> Vec<Note> {
        let db = self.db.lock().unwrap();
        let Ok(mut stmt) = db.prepare("SELECT id, created, text FROM notes ORDER BY id") else {
            return Vec::new();
        };
        stmt.query_map([], |r| {
            Ok(Note {
                id: r.get(0)?,
                created: r.get(1)?,
                text: r.get(2)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    pub fn add_note(&self, text: &str, created: i64) -> Note {
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "INSERT INTO notes (created, text) VALUES (?1, ?2)",
            params![created, text],
        );
        Note {
            id: db.last_insert_rowid(),
            created,
            text: text.to_string(),
        }
    }

    pub fn delete_note(&self, id: i64) {
        let _ = self
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM notes WHERE id = ?1", params![id]);
    }

    pub fn clear_notes(&self) {
        let _ = self.db.lock().unwrap().execute("DELETE FROM notes", []);
    }

    pub fn add_reminder(&self, r: &HelpyReminder) {
        let Ok(data) = serde_json::to_string(r) else {
            return;
        };
        let _ = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO reminders (id, at, data) VALUES (?1, ?2, ?3)",
            params![r.id, r.at, data],
        );
    }

    /// Reminders due by `now`, removed as they're handed out.
    pub fn take_due(&self, now: i64) -> Vec<HelpyReminder> {
        let due: Vec<HelpyReminder> = {
            let db = self.db.lock().unwrap();
            let Ok(mut stmt) = db.prepare("SELECT data FROM reminders WHERE at <= ?1 ORDER BY at")
            else {
                return Vec::new();
            };
            stmt.query_map(params![now], |r| r.get::<_, String>(0))
                .map(|rows| {
                    rows.filter_map(Result::ok)
                        .filter_map(|d| serde_json::from_str(&d).ok())
                        .collect()
                })
                .unwrap_or_default()
        };
        let db = self.db.lock().unwrap();
        for r in &due {
            let _ = db.execute("DELETE FROM reminders WHERE id = ?1", params![r.id]);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::model::RunMode;

    #[test]
    fn agents_and_reminders_round_trip() {
        let s = Store::in_memory();
        let mut a = Agent::new(
            "a1".into(),
            "b1".into(),
            0,
            "Research".into(),
            "Find".into(),
            5,
        );
        a.counters.steps = 7;
        s.save_agent(&a);
        a.counters.steps = 8;
        s.save_agent(&a);
        s.save_batch(&Batch {
            id: "b1".into(),
            mode: RunMode::Single,
            request: "x".into(),
            created: 5,
            trigger: None,
        });
        let back = s.agents();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].counters.steps, 8);
        assert_eq!(s.batches().len(), 1);

        s.add_reminder(&HelpyReminder {
            id: "r1".into(),
            at: 100,
            title: "Call".into(),
            notes: String::new(),
        });
        s.add_reminder(&HelpyReminder {
            id: "r2".into(),
            at: 900,
            title: "Later".into(),
            notes: String::new(),
        });
        assert_eq!(
            s.take_due(500)
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["r1"]
        );
        assert!(s.take_due(500).is_empty());

        s.prune(10, &["a1".into()]);
        assert!(s.agents().is_empty() && s.batches().is_empty());
    }
}
