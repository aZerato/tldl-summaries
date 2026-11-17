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
