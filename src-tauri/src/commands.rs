//! API exposée au frontend (Tauri commands).

use tauri::{AppHandle, Emitter, State};

use crate::{asr, config::Config, gpu, media, models, pipeline, storage, AppState};

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
    // pour le prompt **LLM — le résumé**, jamais la transcription) avant le
    // pipeline.
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

/// Re-résumé d'un enregistrement **sans re-transcrire** (F15/F16) : la
/// transcription stockée (`transcription_json`) est relue et seule l'étape
/// LLM est relancée, avec le contexte choisi. Un **nouvel** enregistrement est
/// consigné (même transcription, nouveau résumé) ; en cas d'erreur LLM,
/// aucun n'est créé (l'ancien reste intact).
///
/// Le contexte n'agit que sur le résumé — jamais sur la transcription.
#[tauri::command]
pub async fn re_summarize(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    // Id du contexte de prompt (F15) ; `None` = contexte par défaut.
    context_id: Option<i64>,
) -> Result<storage::HistoryRecord, String> {
    let rec = state
        .storage
        .lock()
        .unwrap()
        .get(id)
        .map_err(|e| format!("historique : {e}"))?;
    let transcription = transcription_from_record(&rec)?;
    // F15 : contexte (nom pour la consignation, instructions pour le prompt
    // LLM — le résumé, jamais la transcription).
    let (context_instructions, context_name) = match context_id {
        Some(cid) => {
            let c = state
                .storage
                .lock()
                .unwrap()
                .get_context(cid)
                .map_err(|e| format!("contexte : {e}"))?;
            (c.instructions, c.name)
        }
        None => (String::new(), "Contexte par défaut".to_string()),
    };
    // Modèle/temperature/device : ceux de la config courante (F10 : bascule
    // CPU si le GPU demandé est absent).
    let (llm_model, temperature, use_gpu) = {
        let c = state.config.lock().unwrap();
        (c.llm_model.clone(), c.temperature, c.use_gpu)
    };
    let dev = gpu::resolve(use_gpu);
    // Les valeurs consommées par la closure : la closure est `move` — on
    // prélève ce qu'elle utilise, et on garde `dev.name` pour la consignation.
    let n_gpu_layers = dev.n_gpu_layers;
    let device_name = dev.name.clone();
    let llm_model_run = llm_model.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        pipeline::re_summary(
            &transcription,
            &llm_model_run,
            temperature,
            &context_instructions,
            n_gpu_layers,
            &mut |p| {
                let _ = app.emit(
                    "pipeline-progress",
                    &ProgressEvent {
                        step: p.step.clone(),
                        position: p.position,
                        total: p.total,
                    },
                );
            },
        )
    })
    .await
    .map_err(|_| "thread résumé interrompu".to_string())?;
    let summary_markdown = res?;

    // Consignation : la transcription (texte + JSON) est reportée à
    // l'identique ; seul le résumé change.
    let new_id = {
        let st = state.storage.lock().unwrap();
        st.save(
            &rec.file,
            storage::now_epoch(),
            rec.duration_secs,
            &rec.asr_model,
            &llm_model,
            &device_name,
            &context_name,
            &rec.transcription,
            rec.transcription_json.as_deref().unwrap_or_default(),
            &summary_markdown,
        )
        .map_err(|e| format!("historique : {e}"))?
    };
    let st = state.storage.lock().unwrap();
    st.get(new_id).map_err(|e| format!("historique : {e}"))
}

/// Transcription d'un enregistrement, relue pour un re-résumé : le JSON
/// stocké (texte + segments + langue) s'il est lisible, sinon le texte seul
/// (lignes antérieures à la migration — mêmes replis que `toOutput` côté
/// frontend).
fn transcription_from_record(rec: &storage::HistoryRecord) -> Result<asr::Transcription, String> {
    match &rec.transcription_json {
        Some(j) => serde_json::from_str(j)
            .map_err(|e| format!("transcription de l'enregistrement illisible : {e}")),
        None => Ok(asr::Transcription {
            text: rec.transcription.clone(),
            segments: Vec::new(),
            language: String::new(),
        }),
    }
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

/// Supprime un enregistrement de l'historique (F7) ; erreur si introuvable.
#[tauri::command]
pub fn delete_history(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state
        .storage
        .lock()
        .unwrap()
        .delete_record(id)
        .map_err(|e| format!("historique : {e}"))
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

#[cfg(test)]
mod tests {
    use super::transcription_from_record;
    use crate::storage::HistoryRecord;

    fn rec(json: Option<&str>) -> HistoryRecord {
        HistoryRecord {
            id: 1,
            file: "f.wav".into(),
            created: 1_700_000_000,
            duration_secs: 42.0,
            asr_model: "small".into(),
            llm_model: "qwen25-7b-q4".into(),
            device: "CPU".into(),
            context_name: String::new(),
            transcription: "Texte intégral de la rencontre.".into(),
            transcription_json: json.map(str::to_string),
            summary_markdown: String::new(),
        }
    }

    /// Ligne post-migration : le JSON est relisible — texte, segments et
    /// langue reviennent tels quels (le re-résumé ne transcrit pas).
    #[test]
    fn record_json_roundtrips() {
        let json = r#"{"text":"T1. T2.","segments":[{"start":0.0,"end":1.5,"text":"T1."}],"language":"fr"}"#;
        let tr = transcription_from_record(&rec(Some(json))).expect("relecture");
        assert_eq!(tr.text, "T1. T2.");
        assert_eq!(tr.segments.len(), 1);
        assert_eq!(tr.segments[0].end, 1.5);
        assert_eq!(tr.language, "fr");
    }

    /// Ligne antérieure à la migration (pas de JSON) : repli sur le texte
    /// seul — le re-résumé reste possible (le prompt ne lit que le texte).
    #[test]
    fn record_without_json_falls_back_to_text() {
        let tr = transcription_from_record(&rec(None)).expect("replis texte seul");
        assert_eq!(tr.text, "Texte intégral de la rencontre.");
        assert!(tr.segments.is_empty());
        assert_eq!(tr.language, "");
    }

    /// JSON illisible : erreur explicite (pas de repli silencieux).
    #[test]
    fn record_corrupt_json_is_an_error() {
        assert!(transcription_from_record(&rec(Some("{pas du json"))).is_err());
    }
}
