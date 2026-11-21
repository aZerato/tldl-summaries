//! Historique local des traitements (SQLite) — F7 —, contextes de prompt
//! (F15) et dossiers de rangement (F19).
//!
//! Chaque pipeline réussi est automatiquement consigné : fichier, horodatage
//! Unix (secondes, formaté côté frontend), durée, modèles, nom du contexte
//! de prompt utilisé (F15), texte intégral, JSON complet de la transcription
//! (segments horodatés, pour la réouverture — F5) et résumé Markdown.
//!
//! Les bases créées avant les colonnes `transcription_json` /
//! `context_name` sont migrées à l'ouverture (`ALTER TABLE`, idempotent).

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

/// Contexte de prompt (F15) : instructions de **résumé** réutilisables —
/// n'agit que sur l'étape LLM, jamais sur la transcription.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptContext {
    pub id: i64,
    pub name: String,
    /// Instructions injectées dans le prompt LLM (vide = contexte « rencontre »).
    pub instructions: String,
    /// Réutilisation rapide (épinglé en tête de liste).
    pub is_favorite: bool,
    /// Horodatage Unix (secondes) — formaté côté frontend.
    pub created: i64,
}

/// Dossier de rangement (F19) : les résultats de l'historique sont rangés
/// dans **au plus un** dossier ; `records.folder_id = NULL` = « Sans
/// dossier ». La suppression d'un dossier ne supprime aucun résultat :
/// ses enregistrements repassent « Sans dossier ».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub name: String,
    /// Horodatage Unix (secondes) — formaté côté frontend.
    pub created: i64,
    /// Nombre d'enregistrements rangés dans ce dossier.
    pub count: i64,
}

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
    /// Device de calcul utilisé (F10) ; vide sur les lignes écrites
    /// avant la migration.
    pub device: String,
    /// Nom du contexte de prompt utilisé (F15) ; vide sur les lignes
    /// écrites avant la migration.
    pub context_name: String,
    pub transcription: String,
    /// JSON complet du `Transcription` (texte + segments + langue) ;
    /// `None` sur les lignes écrites avant la migration.
    pub transcription_json: Option<String>,
    pub summary_markdown: String,
    /// Dossier de rangement (F19) ; `None` = « Sans dossier ».
    pub folder_id: Option<i64>,
    /// F14 : début de la section traitée, dans le fichier source (s).
    /// `0.0` sur les lignes écrites avant la migration (= fichier complet).
    pub section_start: f64,
    /// F14 : fin de la section traitée, dans le fichier source (s).
    /// `= duration_secs` sur les lignes pré-migration (= fichier complet).
    pub section_end: f64,
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
                    device TEXT NOT NULL DEFAULT '',
                    context_name TEXT NOT NULL DEFAULT '',
                    transcription TEXT NOT NULL,
                    summary_markdown TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS contexts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    instructions TEXT NOT NULL,
                    is_favorite INTEGER NOT NULL DEFAULT 0,
                    created TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS folders (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    created TEXT NOT NULL
                );",
            )
            .map_err(|e| e.to_string())?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Migration idempotente : ajoute les colonnes manquantes aux bases
    /// existantes (`transcription_json` — F7, `device` — F10,
    /// `context_name` — F15, `folder_id` — F19).
    fn migrate(conn: &Connection) -> Result<(), String> {
        let cols = Self::table_columns(conn, "records")?;
        if !cols.contains(&"transcription_json".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN transcription_json TEXT")
                .map_err(|e| e.to_string())?;
        }
        if !cols.contains(&"device".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN device TEXT")
                .map_err(|e| e.to_string())?;
        }
        if !cols.contains(&"context_name".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN context_name TEXT")
                .map_err(|e| e.to_string())?;
        }
        if !cols.contains(&"folder_id".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN folder_id INTEGER")
                .map_err(|e| e.to_string())?;
        }
        // F14 : section traitée. Sur les lignes pré-migration : `0` et
        // `duration_secs` (= fichier complet) — re-dérivables des segments.
        if !cols.contains(&"section_start".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN section_start REAL NOT NULL DEFAULT 0")
                .map_err(|e| e.to_string())?;
        }
        if !cols.contains(&"section_end".to_string()) {
            conn
                .execute_batch("ALTER TABLE records ADD COLUMN section_end REAL NOT NULL DEFAULT 0")
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Colonnes existantes d'une table (pour les migrations idempotentes).
    fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
        let mut st = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let mut cols = Vec::new();
        for name in st
            .query_map(params!(), |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
        {
            cols.push(name.map_err(|e| e.to_string())?);
        }
        Ok(cols)
    }

    /// Consigne un traitement réussi ; renvoie l'id de la ligne.
    /// `device` : device de calcul utilisé (F10) ; `context_name` : nom du
    /// contexte de prompt (F15), vide si défaut. `section_start`/`section_end`
    /// : bornes absolues de la section traitée, dans le fichier source (F14) ;
    /// `0`/`duration_secs` = fichier complet.
    pub fn save(
        &self,
        file: &str,
        created: i64,
        duration_secs: f64,
        asr_model: &str,
        llm_model: &str,
        device: &str,
        context_name: &str,
        transcription: &str,
        transcription_json: &str,
        summary_markdown: &str,
        section_start: f64,
        section_end: f64,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO records (file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, section_start, section_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    file,
                    created,
                    duration_secs,
                    asr_model,
                    llm_model,
                    device,
                    context_name,
                    transcription,
                    transcription_json,
                    summary_markdown,
                    section_start,
                    section_end
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
                "SELECT id, file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, folder_id, section_start, section_end
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
                "SELECT id, file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, folder_id, section_start, section_end
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

    /// Résultats déjà produits pour un fichier (F16) — plus récents en
    /// premier. Un même fichier peut avoir plusieurs enregistrements
    /// (analyses distinctes selon les contextes de prompt — F15).
    pub fn find_by_file(&self, file: &str) -> Result<Vec<HistoryRecord>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, folder_id, section_start, section_end
                 FROM records
                 WHERE file = ?1
                 ORDER BY id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![file], |r| row_to_record(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Recherche plein texte (fichier, transcription, résumé) — F7.
    pub fn search(&self, query: &str) -> Result<Vec<HistoryRecord>, String> {
        let q = format!("%{}%", escape_like(query));
        let mut st = self
            .conn
            .prepare(
                r"SELECT id, file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, folder_id, section_start, section_end
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

    /// Supprime un enregistrement de l'historique (F7) ; erreur si introuvable.
    pub fn delete_record(&self, id: i64) -> Result<(), String> {
        let n = self
            .conn
            .execute("DELETE FROM records WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("enregistrement introuvable".to_string());
        }
        Ok(())
    }

    // --- Dossiers de rangement (F19) ---

    /// Crée un dossier ; renvoie l'id de la ligne.
    pub fn create_folder(&self, name: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO folders (name, created) VALUES (?1, ?2)",
                params![name, now_epoch()],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Liste des dossiers (ordre de création) + nombre d'enregistrements
    /// rangés dans chacun.
    pub fn list_folders(&self) -> Result<Vec<Folder>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT f.id, f.name, f.created,
                       (SELECT COUNT(*) FROM records r WHERE r.folder_id = f.id)
                  FROM folders f
                  ORDER BY f.id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params!(), |r| row_to_folder(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Supprime un dossier (F19) ; ses enregistrements repassent « Sans
    /// dossier » (jamais supprimés). Erreur si introuvable.
    pub fn delete_folder(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE records SET folder_id = NULL WHERE folder_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        let n = self
            .conn
            .execute("DELETE FROM folders WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("dossier introuvable".to_string());
        }
        Ok(())
    }

    /// Rangement (F19) : déplace un enregistrement vers un dossier, ou vers
    /// « Sans dossier » (`None`). Erreur si l'enregistrement ou le dossier
    /// est introuvable.
    pub fn move_record(&self, id: i64, folder_id: Option<i64>) -> Result<(), String> {
        if let Some(f) = folder_id {
            let c: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM folders WHERE id = ?1",
                    params![f],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if c == 0 {
                return Err("dossier introuvable".to_string());
            }
        }
        let n = self
            .conn
            .execute(
                "UPDATE records SET folder_id = ?1 WHERE id = ?2",
                params![folder_id, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("enregistrement introuvable".to_string());
        }
        Ok(())
    }

    /// Rangement de groupe (F19) : déplace **tous** les enregistrements
    /// d'un fichier (même transcription, un groupe de résumés) vers un
    /// dossier ou « Sans dossier » (`None`) — **atomique** (un seul
    /// `UPDATE`) : le groupe n'est jamais partagé entre deux dossiers.
    /// Renvoie le nombre d'enregistrements déplacés ; erreur si le
    /// fichier ou le dossier est introuvable.
    pub fn move_file_records(
        &self,
        file: &str,
        folder_id: Option<i64>,
    ) -> Result<usize, String> {
        if let Some(f) = folder_id {
            let c: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM folders WHERE id = ?1",
                    params![f],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if c == 0 {
                return Err("dossier introuvable".to_string());
            }
        }
        let n = self
            .conn
            .execute(
                "UPDATE records SET folder_id = ?1 WHERE file = ?2",
                params![folder_id, file],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("fichier introuvable".to_string());
        }
        Ok(n as usize)
    }

    /// Enregistrements rangés dans un dossier (F19) — `None` = « Sans
    /// dossier ». Plus récents en premier.
    pub fn list_in_folder(
        &self,
        folder_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<HistoryRecord>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, file, created, duration_secs, asr_model, llm_model, device, context_name, transcription, transcription_json, summary_markdown, folder_id, section_start, section_end
                  FROM records
                  WHERE folder_id IS ?2
                  ORDER BY id DESC
                  LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![limit, folder_id], |r| row_to_record(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    // --- Contextes de prompt (F15) ---

    /// Liste des contextes — favoris d'abord, puis création.
    pub fn list_contexts(&self) -> Result<Vec<PromptContext>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, name, instructions, is_favorite, created
                 FROM contexts
                 ORDER BY is_favorite DESC, id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params!(), |r| row_to_context(r))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    /// Contexte complet (résolution du prompt — F15).
    pub fn get_context(&self, id: i64) -> Result<PromptContext, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, name, instructions, is_favorite, created
                 FROM contexts
                 WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![id], |r| row_to_context(r))
            .map_err(|e| e.to_string())?;
        for row in rows {
            return row.map_err(|e| e.to_string());
        }
        Err("contexte introuvable".to_string())
    }

    /// Crée un contexte ; renvoie l'id de la ligne.
    pub fn create_context(&self, name: &str, instructions: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO contexts (name, instructions, created)
                 VALUES (?1, ?2, ?3)",
                params![name, instructions, now_epoch()],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Met à jour un contexte (nom + instructions) ; erreur si introuvable.
    pub fn update_context(
        &self,
        id: i64,
        name: &str,
        instructions: &str,
    ) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE contexts SET name = ?1, instructions = ?2 WHERE id = ?3",
                params![name, instructions, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("contexte introuvable".to_string());
        }
        Ok(())
    }

    /// Supprime un contexte ; erreur si introuvable.
    pub fn delete_context(&self, id: i64) -> Result<(), String> {
        let n = self
            .conn
            .execute("DELETE FROM contexts WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("contexte introuvable".to_string());
        }
        Ok(())
    }

    /// Bascule le statut favori (réutilisation rapide — F15).
    pub fn set_context_favorite(&self, id: i64, favorite: bool) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE contexts SET is_favorite = ?1 WHERE id = ?2",
                params![i32::from(favorite), id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("contexte introuvable".to_string());
        }
        Ok(())
    }
}

/// Mappe une ligne `records` vers un `HistoryRecord` (ordre du SELECT, 12 colonnes).
fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<HistoryRecord> {
    // Colonne `created` en affinité TEXT : l'entier Unix y est stocké en
    // texte — on lit en `String` puis on reparsé (robuste aux deux stockages).
    let created: String = r.get(2)?;
    let created: i64 = created
        .parse()
        .map_err(|_| rusqlite::Error::ToSqlConversionFailure("created".into()))?;
    // `device` / `context_name` : NULL sur les lignes écrites avant la
    // migration correspondante.
    let device: Option<String> = r.get(6)?;
    let context_name: Option<String> = r.get(7)?;
    Ok(HistoryRecord {
        id: r.get(0)?,
        file: r.get(1)?,
        created,
        duration_secs: r.get(3)?,
        asr_model: r.get(4)?,
        llm_model: r.get(5)?,
        device: device.unwrap_or_default(),
        context_name: context_name.unwrap_or_default(),
        transcription: r.get(8)?,
        transcription_json: r.get(9)?,
        summary_markdown: r.get(10)?,
        folder_id: r.get(11)?,
        // F14 : section traitée (bornes absolues du fichier source).
        section_start: r.get(12)?,
        section_end: r.get(13)?,
    })
}

/// Mappe une ligne `contexts` vers un `PromptContext` (ordre du SELECT, 5 colonnes).
fn row_to_context(r: &rusqlite::Row) -> rusqlite::Result<PromptContext> {
    // Idem `created` sur `records` : entier Unix stocké en texte.
    let created: String = r.get(4)?;
    let created: i64 = created
        .parse()
        .map_err(|_| rusqlite::Error::ToSqlConversionFailure("created".into()))?;
    Ok(PromptContext {
        id: r.get(0)?,
        name: r.get(1)?,
        instructions: r.get(2)?,
        is_favorite: r.get::<_, i32>(3)? != 0,
        created,
    })
}

/// Mappe une ligne `folders` vers un `Folder` (ordre du SELECT, 4 colonnes).
fn row_to_folder(r: &rusqlite::Row) -> rusqlite::Result<Folder> {
    // Idem `created` sur `records` : entier Unix stocké en texte.
    let created: String = r.get(2)?;
    let created: i64 = created
        .parse()
        .map_err(|_| rusqlite::Error::ToSqlConversionFailure("created".into()))?;
    Ok(Folder {
        id: r.get(0)?,
        name: r.get(1)?,
        created,
        count: r.get(3)?,
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
                "CPU",
                "Contexte par défaut",
                "premier texte",
                json,
                "résumé A",
                0.0,
                10.0,
            )
            .expect("save a");
        let b = st
            .save(
                "b.mp4",
                now_epoch(),
                20.0,
                "medium",
                "qwen25-7b-q4",
                "NVIDIA GeForce RTX 4060",
                "Contrôle qualité",
                "deuxième texte",
                json,
                "",
                0.0,
                20.0,
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

        // F10 : le device est consigné et relitu.
        assert_eq!(rows[0].device, "NVIDIA GeForce RTX 4060");
        assert_eq!(rows[1].device, "CPU");
        // F15 : le nom de contexte est consigné et relitu.
        assert_eq!(rows[0].context_name, "Contrôle qualité");
        assert_eq!(rows[1].context_name, "Contexte par défaut");

        // F16 : les résultats existants d'un fichier — un même fichier peut
        // porter plusieurs analyses (contextes différents) ; le plus récent
        // en premier.
        let c = st
            .save(
                "a.wav",
                now_epoch(),
                12.0,
                "small",
                "gemma3-4b-q4",
                "CPU",
                "Décisions",
                "troisième texte",
                json,
                "",
                0.0,
                12.0,
            )
            .expect("save c (même fichier que a)");
        let by_file = st.find_by_file("a.wav").expect("find a.wav");
        assert_eq!(by_file.len(), 2);
        assert_eq!(by_file[0].id, c, "le plus récent doit précéder");
        assert_eq!(by_file[1].id, a);
        assert_eq!(st.find_by_file("inexistant-xyz").expect("find").len(), 0);

        // F7 : suppression d'un enregistrement (double = erreur).
        st.delete_record(a).expect("delete a");
        assert!(st.get(a).is_err(), "a supprimé");
        assert!(st.delete_record(a).is_err(), "double suppression");
        assert_eq!(st.list(50).expect("list").len(), 2, "b + c restants");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F15 : va-et-retour des contextes de prompt (création, favoris,
    /// ordre, mise à jour, suppression).
    #[test]
    fn roundtrip_contexts() {
        let dir = std::env::temp_dir().join("tldl-storage-contexts-test");
        let _ = std::fs::remove_dir_all(&dir);
        let st = Storage::open(&dir).expect("ouverture");

        let a = st
            .create_context("Contrôle qualité", "Résume sous forme de rapport de contrôle.")
            .expect("create a");
        let b = st
            .create_context("Décisions", "Isole les décisions et actions.")
            .expect("create b");

        // Favoris d'abord, puis création (a avant b).
        st.set_context_favorite(a, true).expect("fav a");
        let rows = st.list_contexts().expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, a, "le favori doit précéder");
        assert_eq!(rows[0].is_favorite, true);
        assert_eq!(rows[1].id, b);

        let g = st.get_context(a).expect("get");
        assert_eq!(g.name, "Contrôle qualité");
        assert!(g.instructions.contains("contrôle"));
        assert!((1_500_000_000..=4_100_000_000).contains(&g.created));

        // Mise à jour.
        st.update_context(a, "Contrôle qualité v2", "Instructions réécrites.")
            .expect("update");
        let g2 = st.get_context(a).expect("get2");
        assert_eq!(g2.name, "Contrôle qualité v2");
        assert_eq!(g2.instructions, "Instructions réécrites.");
        st.update_context(9_999_999, "x", "y").expect_err("update introuvable");

        // Suppression.
        st.delete_context(b).expect("delete b");
        assert_eq!(st.list_contexts().expect("list2").len(), 1);
        st.delete_context(b).expect_err("double suppression");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F19 : dossiers de rangement (création, rangement, sortie vers
    /// « Sans dossier », comptes, suppression sans perte).
    #[test]
    fn roundtrip_folders() {
        let dir = std::env::temp_dir().join("tldl-storage-folders-test");
        let _ = std::fs::remove_dir_all(&dir);
        let st = Storage::open(&dir).expect("ouverture");

        let json = r#"{"text":"t","segments":[],"language":"fr"}"#;
        let mk = |file: &str, ctx: &str, text: &str| {
            st.save(
                file,
                now_epoch(),
                10.0,
                "small",
                "gemma3-4b-q4",
                "CPU",
                ctx,
                text,
                json,
                "",
                0.0,
                10.0,
            )
        };
        let a = mk("a.wav", "Contexte par défaut", "premier texte").expect("save a");
        let b = mk("b.mp4", "Contrôle qualité", "deuxième texte").expect("save b");

        let f1 = st.create_folder("Comptes rendus").expect("create f1");
        let f2 = st.create_folder("Clients").expect("create f2");

        // Rangements + comptes des dossiers.
        st.move_record(a, Some(f1)).expect("move a -> f1");
        st.move_record(b, Some(f2)).expect("move b -> f2");
        let folders = st.list_folders().expect("folders");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].id, f1);
        assert_eq!(folders[0].count, 1);
        assert_eq!(folders[1].id, f2);
        assert_eq!(folders[1].count, 1);

        // Contenus : « Sans dossier » vide, un par dossier.
        assert!(st.list_in_folder(None, 50).expect("sans").is_empty());
        let in_f1 = st.list_in_folder(Some(f1), 50).expect("f1");
        assert_eq!(in_f1.len(), 1);
        assert_eq!(in_f1[0].id, a);
        assert_eq!(in_f1[0].folder_id, Some(f1));

        // Sortie vers « Sans dossier » (un seul dossier par enregistrement).
        st.move_record(a, None).expect("move a -> sans");
        let unfiled = st.list_in_folder(None, 50).expect("sans 2");
        assert_eq!(unfiled.len(), 1);
        assert_eq!(unfiled[0].id, a);
        assert!(unfiled[0].folder_id.is_none());

        // Cas d'erreur : dossier inconnu, enregistrement inconnu.
        st.move_record(b, Some(9_999_999)).expect_err("dossier introuvable");
        st.move_record(9_999_999, None).expect_err("enregistrement introuvable");

        // Rangement de groupe : tous les enregistrements d'un même fichier
        // (même transcription) d'un seul coup — même si le groupe est
        // parti (c1 rangé dans f2, c2 « Sans dossier »).
        let c1 = mk("c.wav", "Contexte par défaut", "troisième texte").expect("save c1");
        let c2 = mk("c.wav", "Contrôle qualité", "quatrième texte").expect("save c2");
        st.move_record(c1, Some(f2)).expect("move c1 -> f2");
        let n = st
            .move_file_records("c.wav", Some(f1))
            .expect("move groupe -> f1");
        assert_eq!(n, 2);
        assert_eq!(st.get(c1).expect("c1").folder_id, Some(f1));
        assert_eq!(st.get(c2).expect("c2").folder_id, Some(f1));
        let fg = st.list_folders().expect("folders g");
        assert_eq!(fg.iter().find(|f| f.id == f1).expect("f1").count, 2);
        // Cas d'erreur : fichier inconnu, dossier inconnu.
        st.move_file_records("inexistant.wav", Some(f1))
            .expect_err("fichier introuvable");
        st.move_file_records("c.wav", Some(9_999_999))
            .expect_err("dossier introuvable");
        // Retour « Sans dossier » : le groupe entier, atomique.
        st.move_file_records("c.wav", None).expect("move groupe -> sans");
        assert!(st.get(c1).expect("c1").folder_id.is_none());
        assert!(st.get(c2).expect("c2").folder_id.is_none());

        // Suppression : les enregistrements du dossier repassent « Sans
        // dossier » (jamais supprimés) ; double suppression = erreur.
        // (F14 : voir `roundtrip_section` pour le va-et-retour de la section.)
        st.delete_folder(f1).expect("delete f1");
        assert_eq!(st.list_folders().expect("folders 2").len(), 1);
        // a, c1, c2 sont « Sans dossier » ; b est toujours rangé dans f2.
        assert_eq!(st.list_in_folder(None, 50).expect("sans 3").len(), 3);
        assert_eq!(st.get(b).expect("b").folder_id, Some(f2));
        st.delete_folder(f1).expect_err("double suppression");
        // Dernier dossier : b repasse « Sans dossier » (jamais supprimé).
        st.delete_folder(f2).expect("delete f2");
        assert!(st.list_folders().expect("folders 3").is_empty());
        assert_eq!(st.list_in_folder(None, 50).expect("sans 4").len(), 4);
        assert!(st.get(a).expect("a").folder_id.is_none());
        assert!(st.get(b).expect("b").folder_id.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F14 : la section traitée (bornes absolues du fichier source) est
    /// consignée et relue — `0`/`duration` = fichier complet.
    #[test]
    fn roundtrip_section() {
        let dir = std::env::temp_dir().join("tldl-storage-section-test");
        let _ = std::fs::remove_dir_all(&dir);
        let st = Storage::open(&dir).expect("ouverture");

        // Fichier complet (0 / durée) et section [30, 60] d'un fichier de 100 s.
        let full = st
            .save(
                "full.wav",
                now_epoch(),
                100.0,
                "small",
                "qwen25-7b-q4",
                "CPU",
                "Contexte par défaut",
                "texte complet",
                r#"{"text":"t","segments":[],"language":"fr"}"#,
                "résumé complet",
                0.0,
                100.0,
            )
            .expect("save full");
        let part = st
            .save(
                "part.wav",
                now_epoch(),
                100.0,
                "small",
                "qwen25-7b-q4",
                "CPU",
                "Contexte par défaut",
                "texte de section",
                r#"{"text":"t","segments":[],"language":"fr"}"#,
                "résumé de section",
                30.0,
                60.0,
            )
            .expect("save part");

        // Relecture : les bornes reviennent telles quelles.
        let g = st.get(full).expect("get full");
        assert_eq!(g.section_start, 0.0);
        assert_eq!(g.section_end, 100.0);
        let p = st.get(part).expect("get part");
        assert_eq!(p.section_start, 30.0);
        assert_eq!(p.section_end, 60.0);

        // La liste porte aussi la section.
        let rows = st.list(50).expect("list");
        assert_eq!(rows.len(), 2);
        let by_id = |id| rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(by_id(full).section_start, 0.0);
        assert_eq!(by_id(full).section_end, 100.0);
        assert_eq!(by_id(part).section_start, 30.0);
        assert_eq!(by_id(part).section_end, 60.0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
