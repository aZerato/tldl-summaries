//! Configuration de l'app (persistance JSON dans le répertoire de données).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// "leger" | "equilibre" | "puissant"
    pub profile: String,
    /// Identifiant du modèle ASR (catalogue : models.rs)
    pub asr_model: String,
    /// Identifiant du modèle LLM (catalogue : models.rs)
    pub llm_model: String,
    /// "auto" | "fr" | ...
    pub language: String,
    /// Température LLM (résumé déterministe par défaut)
    pub temperature: f32,
    /// Utiliser le GPU s'il en existe un (F10) ; `gpu::resolve` bascule
    /// CPU (avec note) si la build ou la machine n'en a pas.
    #[serde(default)]
    pub use_gpu: bool,
    /// F9 : séparation des intervenants (diarisation) activée. Le nombre
    /// d'intervenants (auto / N) n'est **pas** persisté ici : il est demandé
    /// au moment de la transcription (argument `run_pipeline`).
    #[serde(default)]
    pub diarize: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: "equilibre".into(),
            asr_model: "small".into(),
            llm_model: "qwen25-7b-q4".into(),
            language: "auto".into(),
            temperature: 0.3,
            use_gpu: false,
            diarize: false,
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        crate::models::app_data_dir().join("config.json")
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(Self::path()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| {
                eprintln!("config.json illisible : valeurs par défaut utilisées");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = Self::path().parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::path(), raw).map_err(|e| e.to_string())
    }
}
