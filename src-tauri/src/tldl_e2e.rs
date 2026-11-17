//! `tldl-e2e` — pipeline complet (F3) exécuté hors de l'app, pour valider l'E2E
//! sans passer par le binaire de test `cargo test`.
//!
//! Pourquoi un binaire dédié : sur la cible GNU (MinGW), le binaire de test
//! généré par cargo (`tldl_lib-*.exe`) meurt au chargement avec 0xC0000139 —
//! le merge `.rsrc` du linker GNU ne fusionne pas le manifest Common-Controls
//! v6 requis par `rfd`/`TaskDialogIndirect`. Ce binaire ne lie que
//! `tldl_lib::pipeline` (+ asr/media/llm/models) : ni le shell Tauri ni `rfd`
//! ne sont référencés, donc `TaskDialogIndirect` n'est pas importé et le
//! binaire se charge sans manifest v6.
//!
//! Usage :
//!   tldl-e2e [fichier_média] [dossier_ffmpeg]
//! ou via variables d'environnement :
//!   TLDL_E2E_INPUT=fichier   TLDL_FFMPEG_DIR=dossier
//!
//! Sans input valide : message d'information + exit 0 (aucun échec).

use std::env;
use std::path::Path;

use tldl_lib::pipeline::{self, PipelineOptions};

fn main() {
    let argv: Vec<String> = env::args().skip(1).collect();

    // `trim()` : les valeurs d'environnement (ou argv) peuvent apporter des
    // espaces parasites (ex. `set VAR=... && ...` sous cmd) qui cassent le
    // chemin `<dossier>\ffprobe.exe`.
    let input = argv
        .first()
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("TLDL_E2E_INPUT").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let ffmpeg = argv
        .get(1)
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("TLDL_FFMPEG_DIR").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    // Modèle LLM attendu (le binaire ne vérifie que la *présence* du GGUF :
    // absent → dégradation gracie, présent → vraie génération).
    let llm = env::var("TLDL_E2E_LLM")
        .unwrap_or_else(|_| String::from("qwen25-7b-q4"))
        .trim()
        .to_string();

    if input.is_empty() || !Path::new(&input).exists() {
        println!(
            "E2E ignorée : input manquant.\n  \
             (passer un fichier en argv[1] ou TLDL_E2E_INPUT ; \
              TLDL_FFMPEG_DIR pour le FFmpeg embarqué)"
        );
        return; // exit 0
    }
    if !ffmpeg.is_empty() {
        env::set_var("TLDL_FFMPEG_DIR", &ffmpeg);
    }

    println!("E2E : {}", input);
    println!("      asr=small  llm={llm}  langue=auto  gpu=false");

    let opts = PipelineOptions {
        asr_model: "small".into(),
        llm_model: llm,
        language: "auto".into(),
        temperature: 0.3,
        use_gpu: false,
    };

    let mut last_step = String::new();
    let out = pipeline::run(&input, &opts, &mut |p| {
        if p.step != last_step {
            last_step = p.step.clone();
            println!("  [étape] {}", p.step);
        }
    });

    let o = match out {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ÉCHEC pipeline : {e}");
            std::process::exit(1);
        }
    };

    println!("durée : {:.2}s", o.duration_secs);
    println!("langue : {}", o.transcription.language);
    println!("segments : {}", o.transcription.segments.len());
    for s in &o.transcription.segments {
        println!("  [{:>7.2}s - {:>7.2}s] {}", s.start, s.end, s.text);
    }
    println!("texte :");
    println!("  {}", o.transcription.text);

    // Dégradation gracie : résumé indisponible tant que le modèle LLM est
    // absent (F4). La transcription reste utilisable.
    match &o.summary_error {
        Some(e) => println!("résumé indisponible : {e}"),
        None => {
            println!("résumé :");
            println!("{}", o.summary_markdown);
        }
    }

    // Assertion E2E (F3) : la transcription doit produire des segments.
    if o.transcription.segments.is_empty() {
        eprintln!("ÉCHEC E2E : aucun segment produit");
        std::process::exit(1);
    }
    println!("E2E OK");
}
