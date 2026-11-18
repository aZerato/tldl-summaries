//! Catalogue des modèles (GGUF) et répertoire local des modèles.
//!
//! Référentiel : docs/models.md.

use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

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
    /// URL de téléchargement (Hugging Face) — F11, opt-in.
    /// Pas envoyée au frontend (`skip`) : l'UI n'a que l'état `available`.
    #[serde(skip)]
    pub source: Option<String>,
}

/// Progression d'un téléchargement de modèle — émise à l'événement
/// `model-download-progress` (F11).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    /// Octets reçus.
    pub received: u64,
    /// Taille totale (0 si inconnue — pas de `Content-Length`).
    pub total: u64,
}

/// Tokens d'annulation des téléchargements en cours (clé = id du modèle).
static CANCELLERS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_canceller(id: &str) -> Result<Arc<AtomicBool>, String> {
    let mut map = CANCELLERS.lock().unwrap();
    if map.contains_key(id) {
        return Err("un téléchargement est déjà en cours pour ce modèle".into());
    }
    let c = Arc::new(AtomicBool::new(false));
    map.insert(id.to_string(), c.clone());
    Ok(c)
}

pub fn deregister_canceller(id: &str) {
    CANCELLERS.lock().unwrap().remove(id);
}

/// Demande d'annulation d'un téléchargement en cours (F11).
/// Retourne `true` s'il y avait un téléchargement à annuler.
pub fn cancel_download(id: &str) -> bool {
    let map = CANCELLERS.lock().unwrap();
    match map.get(id) {
        Some(c) => {
            c.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
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
///
/// Sources : ASR = `ggerganov/whisper.cpp` (f16) ; LLM = `ggml-org`
/// (Gemma) + `bartowski` (Qwen / Llama), tous en Q4_K_M. Le dépôt manuel
/// dans `models/` reste le chemin hors ligne (F11) : l'URL n'est que la
/// commodité opt-in.
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
            source: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
                    .into(),
            ),
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
            source: Some("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
                    .into()),
        },
        ModelInfo {
            id: "qwen25-1.5b-q4".into(),
            kind: ModelKind::Llm,
            name: "Qwen2.5 1.5B Q4_K_M".into(),
            size_mb: 1000,
            quant: "Q4_K_M".into(),
            ram_gb: 2.0,
            is_default: false,
            available: false,
            source: Some(
                "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/ggml-org/gemma-3-4b-it-GGUF/resolve/main/gemma-3-4b-it-Q4_K_M.gguf"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"
                    .into(),
            ),
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
            source: Some(
                "https://huggingface.co/ggml-org/gemma-3-12b-it-GGUF/resolve/main/gemma-3-12b-it-Q4_K_M.gguf"
                    .into(),
            ),
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

/// Téléchargement d'un modèle (F11, opt-in) : source Hugging Face.
///
/// Écriture dans `models/` via **fichier temporaire** (`.part`) +
/// **renommage atomique** : un annulation/échec ne laisse jamais un modèle
/// à moitié, et `status()` ne le déclare donc pas disponible.
///
/// - `progress` reçoit (octets reçus, octets totaux) à chaque chunk.
/// - L'annulation se fait via `cancel_download(id)` (le token est lu ici).
/// - Retourne `Ok(())` si téléchargé **ou annulé** ; `Err` si échec réseau.
pub fn download(
    kind: ModelKind,
    id: &str,
    url: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let fname = match kind {
        ModelKind::Asr => format!("whisper-{id}.bin"),
        ModelKind::Llm => format!("llm-{id}.gguf"),
    };
    let dir = models_dir();
    let target = dir.join(&fname);
    if target.exists() {
        return Err("ce modèle est déjà présent — pas de téléchargement à faire".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{fname}.part"));

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP : {e}"))?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("réseau : {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "HTTP {} — le fichier n'est pas disponible (le modèle existe-t-il ?)",
            status.as_u16()
        ));
    }
    let total: u64 = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 256 * 1024];
    let mut received: u64 = 0;
    let mut cancelled = false;
    let mut out: Result<(), String> = Ok(());
    'write: loop {
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
            break 'write;
        }
        match resp.read(&mut buf) {
            Ok(0) => break 'write,
            Ok(n) => match file.write_all(&buf[..n]) {
                Ok(()) => {
                    received += n as u64;
                    progress(received, total);
                }
                Err(e) => {
                    out = Err(format!("écriture disque (espace suffisant ?) : {e}"));
                    break 'write;
                }
            },
            Err(e) => {
                out = Err(format!("lecture du flux : {e}"));
                break 'write;
            }
        }
    }
    // Fermeture explicite AVANT remove_file / rename : sous Windows le descripteur
    // verrouille le fichier (pas de suppression/renommage sur fichier ouvert).
    drop(file);
    match out {
        Ok(()) if cancelled => {
            let _ = std::fs::remove_file(&tmp);
            Ok(()) // annulation = pas une erreur pour l'UI
        }
        Ok(()) if received == 0 => {
            let _ = std::fs::remove_file(&tmp);
            Err("fichier vide (0 octet) — téléchargement interrompu".into())
        }
        Ok(()) => std::fs::rename(&tmp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        }),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Suppression d'un modèle local (F11) : retire le fichier de `models/`.
///
/// Refus si un téléchargement est en cours pour ce modèle (le `.part` est
/// alors actif). Le modèle supprimé redevient « à télécharger » dans l'UI ;
/// un modèle sélectionné dans la config provoque la dégradation gracie
/// habituelle (F4).
pub fn delete(kind: ModelKind, id: &str) -> Result<(), String> {
    if CANCELLERS.lock().unwrap().contains_key(id) {
        return Err(
            "un téléchargement est en cours pour ce modèle — annulez-le d'abord".into(),
        );
    }
    let path = model_path(kind, id);
    if !path.exists() {
        return Err("aucun modèle local à supprimer".into());
    }
    std::fs::remove_file(&path)
        .map_err(|e| format!("suppression impossible (fichier en usage ?) : {e}"))
}
