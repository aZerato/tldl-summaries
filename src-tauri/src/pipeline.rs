//! Orchestration : sonde → WAV 16 kHz → ASR → LLM → sortie UI.
//! (docs/architecture.md §5)

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{asr, llm, media};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "probe" | "convert" | "asr" | "llm" | "done"
    pub step: String,
    /// Position (secondes pour convert/asr, tokens pour llm, 0 sinon)
    pub position: f64,
    /// Durée/total de la même unité
    pub total: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineOutput {
    pub transcription: asr::Transcription,
    pub summary_markdown: String,
    /// `Some` si le résumé est indisponible (modèle LLM absent — F4).
    pub summary_error: Option<String>,
    pub asr_model: String,
    pub llm_model: String,
    /// Device utilisé (F10) : ex. « CPU », « NVIDIA … ».
    pub device: String,
    /// Note de bascule (F10) : `Some` si GPU demandé mais absent.
    pub device_note: Option<String>,
    /// Chemin du fichier source (F13) : lecture dans le lecteur de la page
    /// résultat (protocole asset Tauri).
    pub file: String,
    pub duration_secs: f64,
}

pub struct PipelineOptions {
    pub asr_model: String,
    pub llm_model: String,
    pub language: String,
    pub temperature: f32,
    /// GPU résolu (F10) : `true` si le GPU est réellement utilisé.
    pub use_gpu: bool,
    /// LLM (F10) : `-1` = GPU (toutes couches), `0` = CPU.
    pub n_gpu_layers: i32,
    /// Nom du device résolu (F10) : ex. « CPU », « NVIDIA … ».
    pub device: String,
    /// Note de bascule (F10) : `Some` si GPU demandé mais absent.
    pub device_note: Option<String>,
    /// Contexte de prompt utilisateur (F15) — n'agit que sur le **résumé
    /// LLM** (la transcription reste fidèle à l'audio) ; vide = contexte
    /// « rencontre ».
    pub context_instructions: String,
}

/// Exécute le pipeline complet. `progress` est appelé à chaque étape.
pub fn run(
    input: &str,
    opts: &PipelineOptions,
    progress: &mut dyn FnMut(&Progress),
) -> Result<PipelineOutput, String> {
    progress(&Progress {
        step: "probe".into(),
        position: 0.0,
        total: 0.0,
    });

    // 1) Sonde du média (format, durée, pistes).
    let info = media::probe(input).map_err(|e| format!("sonde média : {e}"))?;
    if !info.has_audio {
        return Err("aucune piste audio détectée dans ce fichier".into());
    }

    // 2) → 4) sous dossier temporaire, nettoyé systématiquement (C1/C4).
    let work = temp_dir()?;
    let wav = work.join("audio-16k.wav");
    let res = run_inner(input, &wav, info, opts, progress);
    let _ = std::fs::remove_dir_all(&work);
    res
}

fn run_inner(
    input: &str,
    wav: &std::path::Path,
    info: media::MediaInfo,
    opts: &PipelineOptions,
    progress: &mut dyn FnMut(&Progress),
) -> Result<PipelineOutput, String> {
    // 2) Conversion WAV PCM 16 kHz mono (C5 : vidéo → audio dans l'app).
    progress(&Progress {
        step: "convert".into(),
        position: 0.0,
        total: info.duration_secs,
    });
    media::to_wav_16k(std::path::Path::new(input), wav)
        .map_err(|e| format!("conversion WAV : {e}"))?;
    progress(&Progress {
        step: "convert".into(),
        position: info.duration_secs,
        total: info.duration_secs,
    });

    // 3) Transcription (whisper.cpp).
    let tr = asr::transcribe(
        wav,
        &asr::AsrConfig {
            model: opts.asr_model.clone(),
            language: opts.language.clone(),
        },
        opts.use_gpu,
        &mut |pos, dur| {
            progress(&Progress {
                step: "asr".into(),
                position: pos,
                total: dur,
            });
        },
    )
    .map_err(|e| format!("ASR : {e}"))?;

    // 4) Résumé (llama.cpp, F4) — dégradation gracie si le modèle est absent :
    //    la transcription reste utilisable (l'app est 100 % locale, chaque
    //    modèle est indépendant et opt-in).
    let (summary_markdown, summary_error) =
        if !crate::models::model_path(crate::models::ModelKind::Llm, &opts.llm_model)
            .exists()
        {
            (
                String::new(),
                Some(format!(
                    "Modèle LLM « {} » introuvable — résumé indisponible. \
                     Déposez le GGUF dans models/ (F11).",
                    opts.llm_model
                )),
            )
        } else {
            let cfg = llm::LlmConfig {
                model: opts.llm_model.clone(),
                temperature: opts.temperature,
                max_tokens: 1024,
            };
            // Dégradation gracie : une erreur LLM (chargement, dépassement
            // de contexte, GGUF corrompu) ne doit pas faire perdre la
            // transcription — le résumé passe en bannière.
            let r = llm::summarize(&tr, &cfg, &opts.context_instructions, opts.n_gpu_layers, &mut |pos, max| {
                progress(&Progress {
                    step: "llm".into(),
                    position: pos as f64,
                    total: max as f64,
                });
            });
            match r {
                Ok(md) => (md, None),
                Err(e) => (String::new(), Some(e)),
            }
        };

    progress(&Progress {
        step: "done".into(),
        position: 1.0,
        total: 1.0,
    });

    Ok(PipelineOutput {
        transcription: tr,
        summary_markdown,
        summary_error,
        asr_model: opts.asr_model.clone(),
        llm_model: opts.llm_model.clone(),
        device: opts.device.clone(),
        device_note: opts.device_note.clone(),
        file: input.to_string(),
        duration_secs: info.duration_secs,
    })
}

/// Re-résumé sans re-transcription (F15/F16) : seule l'étape **LLM** est
/// relancée, sur une transcription existante (réutilisée à l'identique —
/// pas de sonde, pas de conversion, pas d'ASR).
///
/// `context_instructions` : contexte de prompt (F15) — n'agit que sur le
/// résumé, jamais sur la transcription. `n_gpu_layers` : offload GPU (F10).
/// `progress` : reçoit les étapes « llm » puis « done » (même protocole que
/// `run` — la barre de progression de l'import s'en sert).
pub fn re_summary(
    transcription: &asr::Transcription,
    llm_model: &str,
    temperature: f32,
    context_instructions: &str,
    n_gpu_layers: i32,
    progress: &mut dyn FnMut(&Progress),
) -> Result<String, String> {
    progress(&Progress {
        step: "llm".into(),
        position: 0.0,
        total: 0.0,
    });
    // Le résumé est l'unique produit ici : une erreur LLM (modèle absent,
    // chargement, crash) est renvoyée telle quelle — pas de repli possible
    // (la transcription d'origine reste intacte dans l'historique).
    let cfg = llm::LlmConfig {
        model: llm_model.to_string(),
        temperature,
        max_tokens: 1024,
    };
    let r = llm::summarize(
        transcription,
        &cfg,
        context_instructions,
        n_gpu_layers,
        &mut |pos, max| {
            progress(&Progress {
                step: "llm".into(),
                position: pos as f64,
                total: max as f64,
            });
        },
    );
    progress(&Progress {
        step: "done".into(),
        position: 1.0,
        total: 1.0,
    });
    r
}

#[cfg(test)]
mod e2e {
    use super::*;

    /// E2E F3 : pipeline complet (sonde → WAV → whisper → LLM-gated).
    ///
    /// Exige `TLDL_E2E_INPUT` (fichier média) ; sinon ignoré.
    /// `TLDL_FFMPEG_DIR` pour le FFmpeg embarqué.
    #[test]
    fn pipeline_e2e() {
        let input = std::env::var("TLDL_E2E_INPUT").unwrap_or_default();
        if input.is_empty() || !std::path::Path::new(&input).exists() {
            eprintln!("skip : TLDL_E2E_INPUT absent");
            return;
        }
        let mut progress = move |_: &Progress| {};
        let out = run(
            &input,
            &PipelineOptions {
                asr_model: "small".into(),
                llm_model: "qwen25-7b-q4".into(),
                language: "auto".into(),
                temperature: 0.3,
                use_gpu: false,
                n_gpu_layers: 0,
                device: "CPU".into(),
                device_note: None,
                context_instructions: String::new(),
            },
            &mut progress,
        );
        match out {
            Ok(o) => {
                assert!(
                    o.summary_error.is_some(),
                    "LLM absent : summary_error attendu (dégradation gracie)"
                );
                assert!(
                    !o.transcription.segments.is_empty(),
                    "aucun segment produit"
                );
                eprintln!(
                    "OK langue={:?} segments={} texte={:?}",
                    o.transcription.language,
                    o.transcription.segments.len(),
                    o.transcription.text
                );
            }
            Err(e) => panic!("pipeline : {e}"),
        }
    }
}

#[cfg(test)]
mod serde_tests {
    use super::*;

    /// Contrat de fil : les types retournés au frontend s'écrivent en
    /// `camelCase` (Tauri ne convertit que les arguments des commandes, pas
    /// les retours). Le frontend lit `summaryMarkdown`, `durationSecs`, … —
    /// pas les champs snake_case.
    #[test]
    fn pipeline_output_keys_are_camel_case() {
        let out = PipelineOutput {
            transcription: asr::Transcription {
                text: "t".into(),
                segments: vec![asr::Segment {
                    start: 0.0,
                    end: 1.0,
                    text: "x".into(),
                }],
                language: "fr".into(),
            },
            summary_markdown: "s".into(),
            summary_error: None,
            asr_model: "small".into(),
            llm_model: "qwen25-1.5b-q4".into(),
            device: "CPU".into(),
            device_note: None,
            file: "media.wav".into(),
            duration_secs: 42.0,
        };
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["summaryMarkdown"], "s");
        assert_eq!(v["summaryError"], serde_json::Value::Null);
        assert_eq!(v["asrModel"], "small");
        assert_eq!(v["llmModel"], "qwen25-1.5b-q4");
        assert_eq!(v["device"], "CPU");
        assert_eq!(v["deviceNote"], serde_json::Value::Null);
        assert_eq!(v["file"], "media.wav");
        assert_eq!(v["durationSecs"], 42.0);
        // Les champs snake_case ne doivent pas apparaître au fil.
        for k in [
            "summary_markdown",
            "summary_error",
            "asr_model",
            "llm_model",
            "device_note",
            "duration_secs",
        ] {
            assert!(
                v.get(k).is_none(),
                "clé snake_case {k} trouvée au fil (le frontend lit du camelCase)"
            );
        }
    }
}

/// Dossier temporaire de travail (WAV intermédiaire), effacé à la fin.
fn temp_dir() -> Result<PathBuf, String> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("tldl-summaries-{id}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
