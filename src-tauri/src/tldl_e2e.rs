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
//! Mode F11 (sans HTTP serveur requis) :
//!   tldl-e2e download <id> <url>
//! Fais passer le vrai moteur `tldl_lib::models::download` (fichier `.part`
//! + renommage atomique, progression) sur l'`id` donné ; vérifie que la
//! taille disque = progression = `Content-Length`, puis nettoie.
//!   tldl-e2e delete <id>
//! Fais passer `tldl_lib::models::delete` sur un fichier factice (id hors
//! catalogue) : écriture, suppression, vérification de la disparition.
//!   tldl-e2e history
//! Fais passer `tldl_lib::storage` (save/list/get/search) sur un dossier
//! temporaire : jamais l'historique réel de l'app.
//!
//! Sans input valide : message d'information + exit 0 (aucun échec).

use std::env;
use std::path::Path;

use tldl_lib::pipeline::{self, PipelineOptions};

fn main() {
    let argv: Vec<String> = env::args().skip(1).collect();

    // Mode F11 : valider le moteur de téléchargement sur une URL quelconque
    // (serveur local HTTP, ou HTTPS réel — le TLS rustls est donc aussi exercé).
    if argv.first().is_some_and(|a| a.as_str() == "download") {
        run_download_e2e(&argv[1..]);
        return;
    }
    // Mode F11 : valider la suppression d'un modèle local.
    if argv.first().is_some_and(|a| a.as_str() == "delete") {
        run_delete_e2e(&argv[1..]);
        return;
    }
    // Mode F7 : valider l'historique SQLite (dossier temporaire).
    if argv.first().is_some_and(|a| a.as_str() == "history") {
        run_history_e2e();
        return;
    }

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

/// E2E du moteur de téléchargement F11 (usage : `tldl-e2e download <id> <url>`).
///
/// Le fichier est écrit sous `models/whisper-<id>.bin` (id hors catalogue) et
/// supprimé à la fin. En cas d'échec HTTP, vérifie qu'aucun `.part` ne reste.
fn run_download_e2e(rest: &[String]) {
    let (id, url) = match (rest.first(), rest.get(1)) {
        (Some(i), Some(u)) => (i.trim().to_string(), u.trim().to_string()),
        _ => {
            eprintln!("usage : tldl-e2e download <id> <url>");
            std::process::exit(2);
        }
    };

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut last: (u64, u64) = (0, 0);
    let res = tldl_lib::models::download(
        tldl_lib::models::ModelKind::Asr,
        &id,
        &url,
        &cancel,
        &mut |r, t| {
            last = (r, t);
        },
    );
    let path = tldl_lib::models::model_path(tldl_lib::models::ModelKind::Asr, &id);
    if let Err(e) = res {
        // Invariant F11 : un échec ne laisse pas de `.part` derrière.
        let fname = path.file_name().unwrap().to_string_lossy();
        let part = path.with_file_name(format!("{fname}.part"));
        if part.exists() {
            eprintln!("ÉCHEC E2E download : `.part` résiduel ({part:?})");
            std::process::exit(1);
        }
        eprintln!("E2E download ERR (attendu si 404) : {e}");
        std::process::exit(1);
    }

    let size = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) => {
            eprintln!("ÉCHEC E2E download : lecture du fichier ({e})");
            std::process::exit(1);
        }
    };
    if last.0 != size || last.1 != size {
        eprintln!(
            "ÉCHEC E2E download : incohérence taille — disque {size}, reçu {}, total {}",
            last.0, last.1
        );
        let _ = std::fs::remove_file(&path);
        std::process::exit(1);
    }
    let _ = std::fs::remove_file(&path); // nettoyage (l'id n'est pas au catalogue)
    println!("E2E download OK : {size} octets (progression {}/{} = Content-Length)", last.0, last.1);
}

/// E2E de la suppression F11 (usage : `tldl-e2e delete <id>`).
///
/// Crée un fichier factice `whisper-<id>.bin` (id hors catalogue, aucun
/// modèle réel n'est touché), le supprime via `tldl_lib::models::delete`
/// et vérifie qu'il a bien disparu.
fn run_delete_e2e(rest: &[String]) {
    let id = match rest.first() {
        Some(i) if !i.trim().is_empty() => i.trim().to_string(),
        _ => {
            eprintln!("usage : tldl-e2e delete <id>");
            std::process::exit(2);
        }
    };
    let path = tldl_lib::models::model_path(tldl_lib::models::ModelKind::Asr, &id);
    std::fs::write(&path, b"tldl-e2e").unwrap_or_else(|e| {
        eprintln!("ÉCHEC E2E delete : écriture du factice ({e})");
        std::process::exit(1);
    });
    if let Err(e) = tldl_lib::models::delete(tldl_lib::models::ModelKind::Asr, &id) {
        eprintln!("ÉCHEC E2E delete : {e}");
        let _ = std::fs::remove_file(&path);
        std::process::exit(1);
    }
    if path.exists() {
        eprintln!("ÉCHEC E2E delete : le fichier subsiste ({path:?})");
        let _ = std::fs::remove_file(&path);
        std::process::exit(1);
    }
    println!("E2E delete OK : {path:?} supprimé");
}

/// E2E de l'historique F7 (usage : `tldl-e2e history`).
///
/// Va-et-retour factice dans un dossier temporaire (jamais l'historique réel
/// de l'app) : save → list → get → search, puis nettoyage.
fn run_history_e2e() {
    use tldl_lib::storage;

    let dir = std::env::temp_dir().join("tldl-e2e-history");
    let _ = std::fs::remove_dir_all(&dir);
    let st = match storage::Storage::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : ouverture ({e})");
            std::process::exit(1);
        }
    };

    let json = r#"{"text":"Bonjour le monde.","segments":[{"start":0.0,"end":2.5,"text":"Bonjour le monde."}],"language":"fr"}"#;
    let mk = |file: &str, dur: f64, asr: &str, llm: &str, text: &str, sum: &str| {
        st.save(file, storage::now_epoch(), dur, asr, llm, text, json, sum)
    };
    let id1 = match mk(
        "C:\\fake\\audio1.wav",
        125.0,
        "small",
        "gemma3-4b-q4",
        "Bonjour le monde. Texte de test pour la recherche plein texte.",
        "## Résumé\n- point un",
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : save ({e})");
            std::process::exit(1);
        }
    };
    let id2 = match mk(
        "C:\\fake\\video2.mp4",
        30.0,
        "medium",
        "qwen25-7b-q4",
        "Deuxième enregistrement, sans résumé.",
        "",
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : save ({e})");
            std::process::exit(1);
        }
    };

    // list : plus récents en premier.
    let rows = match st.list(50) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : list ({e})");
            std::process::exit(1);
        }
    };
    if rows.len() != 2 || rows[0].id != id2 || rows[1].id != id1 {
        eprintln!(
            "ÉCHEC E2E history : list inattendu ({} lignes, attendu {id2} puis {id1})",
            rows.len()
        );
        std::process::exit(1);
    }
    if rows[0].transcription_json.as_deref() != Some(json) {
        eprintln!("ÉCHEC E2E history : transcription_json absente");
        std::process::exit(1);
    }
    let c = rows[0].created;
    if !(1_500_000_000..=4_100_000_000).contains(&c) {
        eprintln!("ÉCHEC E2E history : horodatage inattendu ({c})");
        std::process::exit(1);
    }

    // get : contenu complet de l'enregistrement le plus ancien.
    match st.get(id1) {
        Ok(g) => {
            if g.transcription
                != "Bonjour le monde. Texte de test pour la recherche plein texte."
                || g.transcription_json.as_deref() != Some(json)
                || g.summary_markdown != "## Résumé\n- point un"
                || g.duration_secs != 125.0
            {
                eprintln!("ÉCHEC E2E history : get incohérent");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("ÉCHEC E2E history : get ({e})");
            std::process::exit(1);
        }
    }
    if st.get(99_999_999).is_ok() {
        eprintln!("ÉCHEC E2E history : get(99999999) aurait dû échouer");
        std::process::exit(1);
    }

    // search : plein texte (résumé, fichier) — un seul coupable chacune.
    let s = match st.search("point un") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : search ({e})");
            std::process::exit(1);
        }
    };
    if s.len() != 1 || s[0].id != id1 {
        eprintln!("ÉCHEC E2E history : search « point un » → {} lignes", s.len());
        std::process::exit(1);
    }
    let s2 = match st.search("video2") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : search ({e})");
            std::process::exit(1);
        }
    };
    if s2.len() != 1 || s2[0].id != id2 {
        eprintln!("ÉCHEC E2E history : search « video2 » → {} lignes", s2.len());
        std::process::exit(1);
    }
    let s3 = match st.search("inexistant-xyz") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E history : search ({e})");
            std::process::exit(1);
        }
    };
    if !s3.is_empty() {
        eprintln!("ÉCHEC E2E history : search « inexistant-xyz » → {} lignes", s3.len());
        std::process::exit(1);
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("E2E history OK : save/list/get/search (dossier temporaire, jamais l'historique réel)");
}
