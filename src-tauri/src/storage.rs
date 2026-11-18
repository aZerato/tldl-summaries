//! Historique local des traitements (SQLite) — F7.
//!
//! Chaque pipeline réussi est automatiquement consigné : fichier, horodatage
//! Unix (secondes, formaté côté frontend), durée, modèles, texte intégral,
//! JSON complet de la transcription (segments horodatés, pour la
//! réouverture — F5) et résumé Markdown.
//!
//! Les bases créées avant la colonne `transcription_json` sont migrées à
//! l'ouverture (`ALTER TABLE`, idempotent).

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: i64,
    pub file: String,
    /// Horodatage Unix (secondes) — formaté côté frontend.
    pub created: i64,
    pub duration_secs: f64,
    pub asr_model: String,
    pub llm_model: String,
    pub transcription: String,
    /// JSON complet du `Transcription` (texte + segments + langue) ;
    /// `None` sur les lignes écrites avant la migration.
    pub transcription_json: Option<String>,
    pub summary_markdown: String,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(dir.join("history.db")).map_err(|e| e.to_string())?;
        conn
            .execute_batch(
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
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Migration : ajoute `transcription_json` aux bases existantes.
    fn migrate(conn: &Connection) -> Result<(), String> {
        let mut st = conn
            .prepare("PRAGMA table_info(records)")
            .map_err(|e| e.to_string())?;
        let mut has_json = false;
        for name in st
            .query_map(params!(), |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
        {
            let name = name.map_err(|e| e.to_string())?;
            if name == "transcription_json" {
                has_json = true;
                break;
            }
        }
        if has_json {
            return Ok(());
        }
        conn
            .execute_batch("ALTER TABLE records ADD COLUMN transcription_json TEXT")
            .map_err(|e| e.to_string())
    }

    /// Consigne un traitement réussi ; renvoie l'id de la ligne.
    pub fn save(
        &self,
        file: &str,
        created: i64,
        duration_secs: f64,
        asr_model: &str,
        llm_model: &str,
        transcription: &str,
        transcription_json: &str,
        summary_markdown: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO records (file, created, duration_secs, asr_model, llm_model, transcription, transcription_json, summary_markdown)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    file,
                    created,
                    duration_secs,
                    asr_model,
                    llm_model,
                    transcription,
                    transcription_json,
                    summary_markdown
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Plus récents en premier.
    pub fn list(&self, limit: i64) -> Result<Vec<HistoryRecord>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, file, created, duration_secs, asr_model, llm_model, transcription, transcription_json, summary_markdown
                 FROM records
                 ORDER BY id DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![limit], |r| row_to_record(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Enregistrement complet (réouverture — F7).
    pub fn get(&self, id: i64) -> Result<HistoryRecord, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, file, created, duration_secs, asr_model, llm_model, transcription, transcription_json, summary_markdown
                 FROM records
                 WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![id], |r| row_to_record(r))
            .map_err(|e| e.to_string())?;
        for row in rows {
            return row.map_err(|e| e.to_string());
        }
        Err("enregistrement introuvable".to_string())
    }

    /// Recherche plein texte (fichier, transcription, résumé) — F7.
    pub fn search(&self, query: &str) -> Result<Vec<HistoryRecord>, String> {
        let q = format!("%{}%", escape_like(query));
        let mut st = self
            .conn
            .prepare(
                r"SELECT id, file, created, duration_secs, asr_model, llm_model, transcription, transcription_json, summary_markdown
                 FROM records
                 WHERE file LIKE ?1 ESCAPE '\'
                    OR transcription LIKE ?1 ESCAPE '\'
                    OR summary_markdown LIKE ?1 ESCAPE '\'
                 ORDER BY id DESC
                 LIMIT 100",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![q], |r| row_to_record(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }
}

/// Mappe une ligne `records` vers un `HistoryRecord` (ordre du SELECT, 9 colonnes).
fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<HistoryRecord> {
    // Colonne `created` en affinité TEXT : l'entier Unix y est stocké en
    // texte — on lit en `String` puis on reparsé (robuste aux deux stockages).
    let created: String = r.get(2)?;
    let created: i64 = created
        .parse()
        .map_err(|_| rusqlite::Error::ToSqlConversionFailure("created".into()))?;
    Ok(HistoryRecord {
        id: r.get(0)?,
        file: r.get(1)?,
        created,
        duration_secs: r.get(3)?,
        asr_model: r.get(4)?,
        llm_model: r.get(5)?,
        transcription: r.get(6)?,
        transcription_json: r.get(7)?,
        summary_markdown: r.get(8)?,
    })
}

/// Échappe les jokers LIKE (`\`, `%`, `_`) pour la recherche plein texte.
fn escape_like(q: &str) -> String {
    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Horodatage Unix (secondes) — colonne `created`, formatée côté frontend.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_save_list_get_search() {
        let dir = std::env::temp_dir().join("tldl-storage-test");
        let _ = std::fs::remove_dir_all(&dir);
        let st = Storage::open(&dir).expect("ouverture");
        let json = r#"{"text":"t","segments":[],"language":"fr"}"#;
        let a = st
            .save(
                "a.wav",
                now_epoch(),
                10.0,
                "small",
                "gemma3-4b-q4",
                "premier texte",
                json,
                "résumé A",
            )
            .expect("save a");
        let b = st
            .save(
                "b.mp4",
                now_epoch(),
                20.0,
                "medium",
                "qwen25-7b-q4",
                "deuxième texte",
                json,
                "",
            )
            .expect("save b");

        let rows = st.list(50).expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, b);
        assert_eq!(rows[1].id, a);
        assert_eq!(rows[0].transcription_json.as_deref(), Some(json));
        let c = rows[0].created;
        assert!((1_500_000_000..=4_100_000_000).contains(&c));

        let g = st.get(a).expect("get");
        assert_eq!(g.transcription, "premier texte");
        assert_eq!(g.summary_markdown, "résumé A");
        assert!(st.get(9_999_999).is_err());

        let s = st.search("premier").expect("search");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, a);
        let s2 = st.search("b.mp4").expect("search");
        assert_eq!(s2.len(), 1);
        assert_eq!(s2[0].id, b);
        assert!(st.search("inexistant-xyz").expect("search").is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
