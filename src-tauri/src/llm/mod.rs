//! LLM — résumé structuré (llama.cpp b10884, ADR-003).
//!
//! Le moteur tourne dans le **processus dédié** `tldl-llm`
//! (`src/tldl_llm_bin.rs`) : le ggml de whisper.cpp et celui de llama.cpp
//! ne peuvent pas cohabiter dans un même binaire (mêmes symboles C
//! `ggml_*`, ABIs différentes). `summarize` lance le binaire, relaie la
//! progression token par token (stderr) et rapporte le Markdown.

use crate::asr::Transcription;
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Identifiant du modèle (catalogue : models.rs)
    pub model: String,
    /// Résumé déterministe : 0.3 par défaut (cf. docs/models.md)
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: "qwen25-7b-q4".into(),
            temperature: 0.3,
            max_tokens: 1024,
        }
    }
}

/// Prompt « rencontre » — docs/architecture.md §5.1 — avec contexte de
/// prompt utilisateur (F15) : `context` vide = prompt par défaut.
pub fn build_prompt(transcription: &Transcription, context: &str) -> String {
    let mut s = String::new();
    s.push_str("Tu es un assistant de synthèse de rencontre.\n");
    let ctx = context.trim();
    if !ctx.is_empty() {
        s.push_str("Contexte d'analyse demandé par l'utilisateur :\n");
        s.push_str(ctx);
        s.push_str("\n\n");
    }
    s.push_str("Voici la transcription horodatée d'une rencontre :\n\n");
    s.push_str(&transcription.text);
    s.push_str("\n\nProduis un résumé en Markdown avec exactement ces sections :\n");
    s.push_str("## Synthèse\n(un paragraphe, 3 à 5 phrases)\n");
    s.push_str("## Points clés\n(puces)\n");
    s.push_str("## Décisions\n(puces ; « aucune » si rien)\n");
    s.push_str(
        "## Actions à venir\n(puces : qui · quoi · échéance si mentionnée ; « aucune » si rien)\n",
    );
    s.push_str(
        "Règles : reste dans la langue de la transcription, n'invente rien ; \
          si tu doutes, dis-le explicitement.",
    );
    s
}

/// Binaire `tldl-llm` : à côté de l'exécutable courant, ou `TLDL_LLM_BIN`.
pub fn llm_bin() -> PathBuf {
    if let Ok(p) = std::env::var("TLDL_LLM_BIN") {
        if !p.trim().is_empty() {
            return PathBuf::from(p.trim());
        }
    }
    let name = format!("tldl-llm{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Produit le résumé Markdown d'une transcription.
///
/// - `context` : contexte de prompt utilisateur (F15), vide = défaut.
/// - `n_gpu_layers` : offload GPU (F10) ; `-1` = GPU, `0` = CPU.
/// - `progress` : (tokens produits, max tokens) — relai des lignes
///   `tldl-progress` émises par `tldl-llm` sur son stderr.
pub fn summarize(
    transcription: &Transcription,
    cfg: &LlmConfig,
    context: &str,
    n_gpu_layers: i32,
    progress: &mut dyn FnMut(u32, u32),
) -> Result<String, String> {
    let model_file = crate::models::model_path(crate::models::ModelKind::Llm, &cfg.model);
    if !model_file.exists() {
        return Err(format!(
            "modèle LLM introuvable : {} — téléchargez-le (F11) ou déposez \
             le GGUF dans models/",
            model_file.display()
        ));
    }
    let prompt = build_prompt(transcription, context);

    // Le prompt peut être long : il passe par un fichier, pas par argv.
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let prompt_file = std::env::temp_dir().join(format!("tldl-llm-prompt-{id}.txt"));
    std::fs::write(&prompt_file, prompt).map_err(|e| e.to_string())?;

    let bin = llm_bin();
    let mut child = match Command::new(&bin)
        .arg(&model_file)
        .arg(&prompt_file)
        .arg(cfg.max_tokens.to_string())
        .arg(format!("{:.3}", cfg.temperature))
        .arg(n_gpu_layers.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&prompt_file);
            return Err(format!(
                "démarrage du processus LLM impossible ({}): {e}",
                bin.display()
            ));
        }
    };

    let mut stdout = child.stdout.take().expect("stdout pipé");
    let stderr = child.stderr.take().expect("stderr pipé");

    // Progression : un thread draine le stderr du processus enfant
    // (1 ligne/token) et relaie vers `progress`.
    let (tx, rx) = mpsc::channel::<(u32, u32)>();
    let err_thread = std::thread::spawn(move || {
        let mut lines: Vec<String> = Vec::new();
        for line in std::io::BufReader::new(stderr).lines() {
            match line {
                Ok(l) => {
                    if let Some(rest) = l.strip_prefix("tldl-progress ") {
                        let mut it = rest.split_ascii_whitespace();
                        if let (Some(n), Some(max)) = (
                            it.next().and_then(|s| s.parse::<u32>().ok()),
                            it.next().and_then(|s| s.parse::<u32>().ok()),
                        ) {
                            let _ = tx.send((n, max));
                        }
                    } else if !l.trim().is_empty() {
                        lines.push(l);
                    }
                }
                Err(_) => {}
            }
        }
        lines
    });

    // Le status arrive quand le processus sort ; le stdout se lit ensuite
    // (pipe à EOF, lecteur unique).
    let (res_tx, res_rx) = mpsc::channel::<Result<std::process::ExitStatus, String>>();
    // Le status passe par le canal — pas besoin de `join` du thread.
    let _worker = std::thread::spawn(move || {
        let r = child.wait().map_err(|e| e.to_string());
        let _ = res_tx.send(r);
    });

    // Boucle de polling (même schéma qu'`asr::transcribe`) : relai de la
    // progression + attente du status.
    let status = loop {
        match rx.try_recv() {
            Ok((n, max)) => progress(n, max),
            Err(mpsc::TryRecvError::Empty) => match res_rx.try_recv() {
                Ok(st) => break st,
                Err(mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break res_rx.recv().map_err(|e| e.to_string())?;
                }
            },
            Err(mpsc::TryRecvError::Disconnected) => {
                break res_rx.recv().map_err(|e| e.to_string())?;
            }
        }
    };
    let status: std::process::ExitStatus = match status {
        Ok(s) => s,
        Err(e) => return Err(e),
    };

    // Le thread ne peut que lire des pipes : pas de panique attendu, mais
    // on garde la main (join() -> Result).
    let err_lines: Vec<String> = err_thread.join().unwrap_or_default();
    let _ = std::fs::remove_file(&prompt_file);

    if !status.success() {
        let tail = err_lines
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "aucun message".into());
        return Err(format!("le processus LLM a échoué : {tail}"));
    }

    let mut text = String::new();
    stdout
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let text = text.trim_end().to_string();
    if text.trim().is_empty() {
        return Err("résumé vide produit par le modèle".into());
    }
    Ok(text)
}
