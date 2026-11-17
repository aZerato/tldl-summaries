//! Historique local des traitements (SQLite).

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: i64,
    pub file: String,
    pub created: String,
    pub duration_secs: f64,
    pub asr_model: String,
    pub llm_model: String,
    pub transcription: String,
    pub summary_markdown: String,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(dir.join("history.db"))
            .map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file TEXT NOT NULL,
                created TEXT NOT NULL,
                duration_secs REAL NOT NULL,
                asr_model TEXT NOT NULL,
                llm_model TEXT NOT NULL,
                transcription TEXT NOT NULL,
                summary_markdown TEXT NOT NULL
            );",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { conn })
    }

    pub fn save(
        &self,
        file: &str,
        created: &str,
        duration_secs: f64,
        asr_model: &str,
        llm_model: &str,
        transcription: &str,
        summary_markdown: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO records (file, created, duration_secs, asr_model, llm_model, transcription, summary_markdown)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![file, created, duration_secs, asr_model, llm_model, transcription, summary_markdown],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list(&self, limit: i64) -> Result<Vec<HistoryRecord>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, file, created, duration_secs, asr_model, llm_model, transcription, summary_markdown
                 FROM records ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![limit], |r| {
                Ok(HistoryRecord {
                    id: r.get(0)?,
                    file: r.get(1)?,
                    created: r.get(2)?,
                    duration_secs: r.get(3)?,
                    asr_model: r.get(4)?,
                    llm_model: r.get(5)?,
                    transcription: r.get(6)?,
                    summary_markdown: r.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }
}
