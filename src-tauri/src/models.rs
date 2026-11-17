//! Catalogue des modèles (GGUF) et répertoire local des modèles.
//!
//! Référentiel : docs/models.md.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Asr,
    Llm,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub kind: ModelKind,
    pub name: String,
    /// Poids du fichier GGUF, en Mo (estimation)
    pub size_mb: u32,
    /// Quantification ("f16", "Q4_K_M", ...)
    pub quant: String,
    /// RAM/VRAM estimée, en Go
    pub ram_gb: f32,
    /// Par défaut du profil « équilibré » (docs/models.md)
    pub is_default: bool,
    /// Modèle déjà présent dans models/
    pub available: bool,
}

/// Répertoire de données de l'app (%LOCALAPPDATA% / ~/.local/share /
/// ~/Library/Application Support), sous-dossier `app.tldl.summaries`.
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("app.tldl.summaries")
}

/// Dossier des modèles (hors du git).
pub fn models_dir() -> PathBuf {
    app_data_dir().join("models")
}

/// Chemin attendu d'un modèle (ASR : `.bin` whisper, LLM : `.gguf`).
pub fn model_path(kind: ModelKind, id: &str) -> PathBuf {
    let name = match kind {
        ModelKind::Asr => format!("whisper-{id}.bin"),
        ModelKind::Llm => format!("llm-{id}.gguf"),
    };
    models_dir().join(name)
}

/// Catalogue statique — voir docs/models.md (les valeurs mesurées
/// remplacent les estimations au fil des benchmarks, F12).
pub fn catalog() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "tiny".into(),
            kind: ModelKind::Asr,
            name: "Whisper tiny".into(),
            size_mb: 75,
            quant: "f16".into(),
            ram_gb: 0.5,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "base".into(),
            kind: ModelKind::Asr,
            name: "Whisper base".into(),
            size_mb: 150,
            quant: "f16".into(),
            ram_gb: 1.0,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "small".into(),
            kind: ModelKind::Asr,
            name: "Whisper small".into(),
            size_mb: 480,
            quant: "f16".into(),
            ram_gb: 1.5,
            is_default: true,
            available: false,
        },
        ModelInfo {
            id: "medium".into(),
            kind: ModelKind::Asr,
            name: "Whisper medium".into(),
            size_mb: 1550,
            quant: "f16".into(),
            ram_gb: 3.0,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "large-v3-turbo".into(),
            kind: ModelKind::Asr,
            name: "Whisper large-v3-turbo".into(),
            size_mb: 1620,
            quant: "f16".into(),
            ram_gb: 3.5,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "qwen25-3b-q4".into(),
            kind: ModelKind::Llm,
            name: "Qwen2.5 3B Q4_K_M".into(),
            size_mb: 2100,
            quant: "Q4_K_M".into(),
            ram_gb: 3.5,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "gemma3-4b-q4".into(),
            kind: ModelKind::Llm,
            name: "Gemma 3 4B Q4_K_M".into(),
            size_mb: 2700,
            quant: "Q4_K_M".into(),
            ram_gb: 4.0,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "qwen25-7b-q4".into(),
            kind: ModelKind::Llm,
            name: "Qwen2.5 7B Q4_K_M".into(),
            size_mb: 4700,
            quant: "Q4_K_M".into(),
            ram_gb: 7.0,
            is_default: true,
            available: false,
        },
        ModelInfo {
            id: "llama31-8b-q4".into(),
            kind: ModelKind::Llm,
            name: "Llama 3.1 8B Q4_K_M".into(),
            size_mb: 4900,
            quant: "Q4_K_M".into(),
            ram_gb: 7.0,
            is_default: false,
            available: false,
        },
        ModelInfo {
            id: "gemma3-12b-q4".into(),
            kind: ModelKind::Llm,
            name: "Gemma 3 12B Q4_K_M".into(),
            size_mb: 7500,
            quant: "Q4_K_M".into(),
            ram_gb: 10.0,
            is_default: false,
            available: false,
        },
    ]
}

/// Catalogue + état local (modèle déjà téléchargé ou déposé à la main).
pub fn status() -> Vec<ModelInfo> {
    catalog()
        .into_iter()
        .map(|m| ModelInfo {
            available: model_path(m.kind, &m.id).exists(),
            ..m
        })
        .collect()
}

/// Téléchargement d'un modèle (F11, opt-in).
///
/// À implémenter : source GGUF (Hugging Face) avec progression ;
/// hors ligne = dépôt manuel du fichier dans `models_dir()`.
pub fn download(
    model: &ModelInfo,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    // TODO(F11) : téléchargement GGUF (ou dépôt manuel documenté).
    let _ = (model, progress);
    Err("téléchargement non branché (F11) — déposez le fichier dans models/"
        .into())
}
