//! API exposée au frontend (Tauri commands).

use tauri::{AppHandle, Emitter, State};

use crate::{config::Config, gpu, media, models, pipeline, storage, AppState};

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
    // Id du contexte de prompt (F15) ; `None` = contexte par défaut.
    context_id: Option<i64>,
) -> Result<pipeline::PipelineOutput, String> {
    // F15 : résolution du contexte (nom pour la consignation, instructions
    // pour le prompt LLM) avant le pipeline.
    let (context_instructions, context_name) = match context_id {
        Some(id) => {
            let c = state
                .storage
                .lock()
                .unwrap()
                .get_context(id)
                .map_err(|e| format!("contexte : {e}"))?;
            (c.instructions, c.name)
        }
        None => (String::new(), "Contexte par défaut".to_string()),
    };
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
    // F10 : résolution du device (auto-détection ; bascule CPU si le GPU
    // demandé est absent — C3 « GPU optionnel, CPU viable »).
    let dev = gpu::resolve(use_gpu);
    let input_copy = input.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let opts = pipeline::PipelineOptions {
            asr_model,
            llm_model,
            language,
            temperature,
            use_gpu: dev.use_gpu,
            n_gpu_layers: dev.n_gpu_layers,
            device: dev.name.clone(),
            device_note: dev.fallback.then(|| {
                "GPU demandé mais non détecté — exécution sur CPU.".to_string()
            }),
            context_instructions,
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
    let output = res?;

    // F7 : consignation automatique dans l'historique (SQLite).
    {
        let transcription_json = serde_json::to_string(&output.transcription)
            .map_err(|e| e.to_string())?;
        let created = storage::now_epoch();
        let st = state.storage.lock().unwrap();
        st.save(
            &input,
            created,
            output.duration_secs,
            &output.asr_model,
            &output.llm_model,
            &output.device,
            &context_name,
            &output.transcription.text,
            &transcription_json,
            &output.summary_markdown,
        )
        .map_err(|e| format!("historique : {e}"))?;
    }

    Ok(output)
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

/// Enregistrement complet de l'historique (réouverture — F7).
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    id: i64,
) -> Result<storage::HistoryRecord, String> {
    state.storage.lock().unwrap().get(id)
}

/// Recherche plein texte dans l'historique (F7).
#[tauri::command]
pub fn search_history(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<storage::HistoryRecord>, String> {
    state.storage.lock().unwrap().search(&query)
}

/// Résultats déjà produits pour un fichier (F16) — plus récents en premier.
#[tauri::command]
pub fn history_for_file(
    state: State<'_, AppState>,
    file: String,
) -> Result<Vec<storage::HistoryRecord>, String> {
    state.storage.lock().unwrap().find_by_file(&file)
}

/// Contextes de prompt (F15) — favoris d'abord.
#[tauri::command]
pub fn list_contexts(
    state: State<'_, AppState>,
) -> Result<Vec<storage::PromptContext>, String> {
    state.storage.lock().unwrap().list_contexts()
}

/// Crée (`id = None`) ou met à jour (`id = Some`) un contexte de prompt.
/// Renvoie l'id de la ligne.
#[tauri::command]
pub fn save_context(
    state: State<'_, AppState>,
    id: Option<i64>,
    name: String,
    instructions: String,
) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("un nom de contexte est requis".into());
    }
    let instructions = instructions.trim_end();
    let st = state.storage.lock().unwrap();
    match id {
        None => st.create_context(name, instructions),
        Some(i) => {
            st.update_context(i, name, instructions)
                .map_err(|e| format!("contexte : {e}"))?;
            Ok(i)
        }
    }
}

/// Supprime un contexte de prompt (F15).
#[tauri::command]
pub fn delete_context(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state
        .storage
        .lock()
        .unwrap()
        .delete_context(id)
        .map_err(|e| format!("contexte : {e}"))
}

/// Bascule le statut favori d'un contexte (F15).
#[tauri::command]
pub fn set_context_favorite(
    state: State<'_, AppState>,
    id: i64,
    favorite: bool,
) -> Result<(), String> {
    state
        .storage
        .lock()
        .unwrap()
        .set_context_favorite(id, favorite)
        .map_err(|e| format!("contexte : {e}"))
}

/// Devices de calcul disponibles (F10) — CPU et GPU/iGPU détectés par
/// le ggml de whisper.cpp (lié dans l'app).
#[tauri::command]
pub fn list_devices() -> Vec<gpu::DeviceInfo> {
    gpu::list_devices()
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
