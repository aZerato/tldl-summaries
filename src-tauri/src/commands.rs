//! API exposée au frontend (Tauri commands).

use tauri::{AppHandle, Emitter, State};

use crate::{config::Config, media, models, pipeline, storage, AppState};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub step: String,
    pub position: f64,
    pub total: f64,
}

/// Sonde d'un fichier (format, durée, pistes).
#[tauri::command]
pub fn probe_media(path: String) -> Result<media::MediaInfo, String> {
    media::probe(&path)
}

/// Pipeline complet (sonde → WAV → ASR → LLM).
/// La progression est émise à l'événement `pipeline-progress`.
#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    asr_model: String,
    llm_model: String,
    language: String,
) -> Result<pipeline::PipelineOutput, String> {
    let (asr_model, llm_model, language, temperature, use_gpu) = {
        let c = state.config.lock().unwrap();
        (
            if asr_model.is_empty() {
                c.asr_model.clone()
            } else {
                asr_model
            },
            if llm_model.is_empty() {
                c.llm_model.clone()
            } else {
                llm_model
            },
            if language.is_empty() {
                c.language.clone()
            } else {
                language
            },
            c.temperature,
            c.use_gpu,
        )
    };
    let input_copy = input;
    let res = tauri::async_runtime::spawn_blocking(move || {
        let opts = pipeline::PipelineOptions {
            asr_model,
            llm_model,
            language,
            temperature,
            use_gpu,
        };
        pipeline::run(&input_copy, &opts, &mut |p| {
            let _ = app.emit(
                "pipeline-progress",
                &ProgressEvent {
                    step: p.step.clone(),
                    position: p.position,
                    total: p.total,
                },
            );
        })
    })
    .await
    .map_err(|_| "thread pipeline interrompu".to_string())?;
    res
}

/// Catalogue des modèles + état local.
#[tauri::command]
pub fn list_models() -> Vec<models::ModelInfo> {
    models::status()
}

/// Téléchargement d'un modèle (F11, opt-in).
///
/// La progression est émise à l'événement `model-download-progress`.
/// Résout en `Ok` si téléchargé **ou annulé** ; `Err` si échec réseau/lecture.
#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    let model = models::catalog()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "modèle inconnu".to_string())?;
    let source = model
        .source
        .clone()
        .ok_or_else(|| "ce modèle n'est pas téléchargeable (dépôt manuel : models/)".to_string())?;
    let kind = model.kind;

    let cancel = models::register_canceller(&id)
        .map_err(|_| "un téléchargement est déjà en cours pour ce modèle".to_string())?;

    let key = id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        models::download(kind, &key, &source, cancel.as_ref(), &mut |r, t| {
            let _ = app.emit(
                "model-download-progress",
                &models::DownloadProgress {
                    id: key.clone(),
                    received: r,
                    total: t,
                },
            );
        })
    })
    .await
    .map_err(|_| "téléchargement interrompu".to_string())?;

    models::deregister_canceller(&id);
    res
}

/// Annule un téléchargement de modèle en cours (F11).
/// Retourne `true` s'il y avait un téléchargement à annuler.
#[tauri::command]
pub fn cancel_model_download(id: String) -> bool {
    models::cancel_download(&id)
}

/// Supprime un modèle local (F11) : le fichier est retiré de `models/`.
#[tauri::command]
pub fn delete_model(id: String) -> Result<(), String> {
    let model = models::catalog()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "modèle inconnu".to_string())?;
    models::delete(model.kind, &id)
}

/// Historique des traitements (plus récents en premier).
#[tauri::command]
pub fn list_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<storage::HistoryRecord>, String> {
    state.storage.lock().unwrap().list(limit.unwrap_or(50))
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(
    state: State<'_, AppState>,
    config: Config,
) -> Result<(), String> {
    *state.config.lock().unwrap() = config.clone();
    config.save()
}
