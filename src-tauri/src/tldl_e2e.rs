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
//!   tldl-e2e contexts
//! Fais passer `tldl_lib::storage` (contextes de prompt, F15 :
//! create/list/get/update/delete) sur un dossier temporaire.
//!   tldl-e2e devices
//! Fais passer `tldl_lib::gpu` (F10) : énumère les devices ggml (le CPU est
//! toujours attendu ; le GPU seulement si la build a activé un backend —
//! `TLDL_GPU`) et vérifie `resolve` (GPU demandé → bascule CPU si absent).
//!   tldl-e2e resummarize
//! Fais passer le **re-résumé sans re-transcription** (F15/F16) : enregistrement
//! de référence (transcription JSON) → `pipeline::re_summary` (seule l'étape
//! LLM) → nouvel enregistrement → `find_by_file` (dossier temporaire, jamais
//! l'historique réel). `TLDL_E2E_LLM` = modèle (défaut `qwen25-7b-q4`) ;
//! `TLDL_E2E_GPU=1` → offload GPU (`n_gpu_layers = -1`), sinon CPU.
//!   tldl-e2e folders
//! Fais passer `tldl_lib::storage` (dossiers de rangement, F19 :
//! create/move/list/delete sans perte) sur un dossier temporaire : jamais
//! l'historique réel de l'app.
//!   tldl-e2e section [fichier_média] [dossier_ffmpeg]
//! Fais passer la **coupe de section** F14 sur le pipeline réel : section
//! `[20%, 80%]` du fichier → WAV de la section seule → ASR ; vérifie que
//! les bornes sont reportées, que tous les segments sont **dans**
//! `[début, fin]` (horodatages décalés du début de section) et que la
//! transcription est plus courte que le fichier complet. Modèle ASR requis
//! (`TLDL_E2E_ASR`, défaut « small ») ; LLM absent = dégradation gracie.
//!   tldl-e2e llm
//! Fais passer la **passe réelle LLM** (carry LLM, fix chat template) :
//! vraie génération sur le vrai modèle (`TLDL_E2E_LLM`, défaut
//! `qwen25-7b-q4`) — contexte « Résume en 10 phrases maximum. » →
//! `llm::summarize` (lance le binaire `tldl-llm` réel, même dossier que
//! celui-ci). Vérifie : EOS naturel (arrêt avant `max_tokens`), pas de
//! méta-préambule (« voici le résumé » …), au plus 10 phrases (heuristique
//! [.!?]). Transcription de test **longue** (≈ 700 tokens — au-delà du
//! `n_batch` 512, cf. régression `GGML_ASSERT(n_tokens_all <= n_batch)`).
//! Modèle absent → ignorée (exit 0), comme les autres modes.
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
    // Mode F15 : valider les contextes de prompt (dossier temporaire).
    if argv.first().is_some_and(|a| a.as_str() == "contexts") {
        run_contexts_e2e();
        return;
    }
    // Mode F10 : détection des devices ggml (CPU + GPU si backend actif).
    if argv.first().is_some_and(|a| a.as_str() == "devices") {
        run_devices_e2e();
        return;
    }
    // Mode F15/F16 : re-résumé sans re-transcription (dossier temporaire).
    if argv.first().is_some_and(|a| a.as_str() == "resummarize") {
        run_resummarize_e2e();
        return;
    }
    // Mode F19 : dossiers de rangement (dossier temporaire).
    if argv.first().is_some_and(|a| a.as_str() == "folders") {
        run_folders_e2e();
        return;
    }
    // Mode F14 : coupe de section (WAV de la section, segments décalés).
    if argv.first().is_some_and(|a| a.as_str() == "section") {
        run_section_e2e(&argv[1..]);
        return;
    }
    // Mode carry LLM : passe réelle « 10 phrases » (fix chat template).
    if argv.first().is_some_and(|a| a.as_str() == "llm") {
        run_llm_e2e();
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
        n_gpu_layers: 0,
        device: "CPU".into(),
        device_note: None,
        context_instructions: String::new(),
        // F14 : pas de section demandée → fichier complet (le pipeline borne
        // à la sonde : 0/0 = « tout »).
        section_start: 0.0,
        section_end: 0.0,
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
    // F14 : pas de section demandée → fichier complet (0 / durée).
    let mk = |file: &str, dur: f64, asr: &str, llm: &str, dev: &str, ctx: &str, text: &str, sum: &str| {
        st.save(file, storage::now_epoch(), dur, asr, llm, dev, ctx, text, json, sum, 0.0, dur)
    };
    let id1 = match mk(
        "C:\\fake\\audio1.wav",
        125.0,
        "small",
        "gemma3-4b-q4",
        "CPU",
        "Contexte par défaut",
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
        "NVIDIA GeForce RTX 4060",
        "Contrôle qualité",
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
    // F10 : le device est consigné (plus récent en tête).
    if rows[0].device != "NVIDIA GeForce RTX 4060" || rows[1].device != "CPU" {
        eprintln!("ÉCHEC E2E history : device inattendu ({}, {})", rows[0].device, rows[1].device);
        std::process::exit(1);
    }
    // F15 : le nom de contexte est consigné (plus récent en tête).
    if rows[0].context_name != "Contrôle qualité" || rows[1].context_name != "Contexte par défaut" {
        eprintln!("ÉCHEC E2E history : context_name inattendu ({}, {})", rows[0].context_name, rows[1].context_name);
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

/// E2E des contextes de prompt F15 (usage : `tldl-e2e contexts`).
///
/// Va-et-retour factice dans un dossier temporaire (jamais la base réelle de
/// l'app) : create → list (ordre favoris) → get → update → delete, puis
/// nettoyage.
fn run_contexts_e2e() {
    use tldl_lib::storage;

    let dir = std::env::temp_dir().join("tldl-e2e-contexts");
    let _ = std::fs::remove_dir_all(&dir);
    let st = match storage::Storage::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : ouverture ({e})");
            std::process::exit(1);
        }
    };

    let mk = |name: &str, instr: &str| st.create_context(name, instr);
    let a = match mk("Contrôle qualité", "Résume sous forme de rapport de contrôle.") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : create ({e})");
            std::process::exit(1);
        }
    };
    let b = match mk("Décisions", "Isole décisions et actions à venir.") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : create ({e})");
            std::process::exit(1);
        }
    };

    // Ordre : favoris d'abord, puis création (a avant b).
    st.set_context_favorite(a, true)
        .unwrap_or_else(|e| {
            eprintln!("ÉCHEC E2E contexts : favori ({e})");
            std::process::exit(1);
        });
    let rows = match st.list_contexts() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : list ({e})");
            std::process::exit(1);
        }
    };
    if rows.len() != 2 || rows[0].id != a || rows[1].id != b {
        eprintln!("ÉCHEC E2E contexts : list inattendue ({} lignes)", rows.len());
        std::process::exit(1);
    }
    if !rows[0].is_favorite || rows[1].is_favorite {
        eprintln!("ÉCHEC E2E contexts : drapeaux favoris inattendus");
        std::process::exit(1);
    }

    // get : contenu complet.
    let g = match st.get_context(a) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : get ({e})");
            std::process::exit(1);
        }
    };
    if g.name != "Contrôle qualité"
        || g.instructions != "Résume sous forme de rapport de contrôle."
    {
        eprintln!("ÉCHEC E2E contexts : get incohérent ({:?})", g);
        std::process::exit(1);
    }

    // update : appliquée, et impossible sur un id inexistant.
    st.update_context(a, "Contrôle qualité v2", "Instructions réécrites.")
        .unwrap_or_else(|e| {
            eprintln!("ÉCHEC E2E contexts : update ({e})");
            std::process::exit(1);
        });
    if st.get_context(a).unwrap().instructions != "Instructions réécrites." {
        eprintln!("ÉCHEC E2E contexts : update non appliquée");
        std::process::exit(1);
    }
    if st.update_context(9_999_999, "x", "y").is_ok() {
        eprintln!("ÉCHEC E2E contexts : update(9999999) aurait dû échouer");
        std::process::exit(1);
    }

    // delete : une fois, puis erreur.
    st.delete_context(b)
        .unwrap_or_else(|e| {
            eprintln!("ÉCHEC E2E contexts : delete ({e})");
            std::process::exit(1);
        });
    if st.delete_context(b).is_ok() {
        eprintln!("ÉCHEC E2E contexts : double delete aurait dû échouer");
        std::process::exit(1);
    }
    let rows = match st.list_contexts() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E contexts : list finale ({e})");
            std::process::exit(1);
        }
    };
    if rows.len() != 1 || rows[0].id != a {
        eprintln!("ÉCHEC E2E contexts : état final inattendu ({} lignes)", rows.len());
        std::process::exit(1);
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("E2E contexts OK : create/list/get/update/delete (dossier temporaire, jamais la base réelle)");
}

/// E2E de la détection de devices F10 (usage : `tldl-e2e devices`).
///
/// Interroge le registre de devices du ggml de whisper.cpp (lié dans
/// l'app) : attend au moins le device CPU ; le GPU n'apparaît que si la
/// build a activé un backend GPU (`TLDL_GPU` — cf. build.rs) et que la
/// machine en possède un.
fn run_devices_e2e() {
    let devs = tldl_lib::gpu::list_devices();
    if devs.is_empty() {
        eprintln!("ÉCHEC E2E devices : aucun device déclaré par ggml (le CPU est toujours attendu)");
        std::process::exit(1);
    }
    let has_cpu = devs.iter().any(|d| d.kind == "cpu");
    if !has_cpu {
        eprintln!("ÉCHEC E2E devices : device CPU absent ({devs:?})");
        std::process::exit(1);
    }
    // `resolve` : GPU demandé → premier GPU/iGPU détecté, sinon bascule CPU.
    let gpu_req = tldl_lib::gpu::resolve(true);
    if !gpu_req.use_gpu && !gpu_req.fallback {
        eprintln!("ÉCHEC E2E devices : resolve(true) incohérent ({gpu_req:?})");
        std::process::exit(1);
    }
    let cpu_req = tldl_lib::gpu::resolve(false);
    if cpu_req.use_gpu || cpu_req.n_gpu_layers != 0 || cpu_req.fallback {
        eprintln!("ÉCHEC E2E devices : resolve(false) devrait forcer le CPU ({cpu_req:?})");
        std::process::exit(1);
    }
    println!("devices ggml :");
    for d in &devs {
        println!("  {} ({})", d.name, d.kind);
    }
    println!("resolve(use_gpu=true) : {} ({}, fallback={})", gpu_req.name, gpu_req.kind, gpu_req.fallback);
    println!("E2E devices OK");
}

/// E2E du re-résumé sans re-transcription F15/F16 (usage : `tldl-e2e resummarize`).
///
/// Va-et-retour factice dans un dossier temporaire (jamais l'historique réel
/// de l'app) : enregistrement de référence (transcription JSON) → relecture
/// `Transcription` → `pipeline::re_summary` (seule l'étape LLM est relancée,
/// avec un contexte) → nouvel enregistrement (même transcription) →
/// `find_by_file` (le nouveau précède l'ancien).
///
/// Le modèle LLM est **requis** (le re-résumé ne produit pas de repli) :
/// absent → ignorée (exit 0), comme la sonde E2E sans input.
fn run_resummarize_e2e() {
    use tldl_lib::{asr, models, pipeline, storage};

    let llm = env::var("TLDL_E2E_LLM")
        .unwrap_or_else(|_| String::from("qwen25-7b-q4"))
        .trim()
        .to_string();
    if !models::model_path(models::ModelKind::Llm, &llm).exists() {
        println!("E2E resummarize ignorée : modèle LLM {llm} absent (TLDL_E2E_LLM)");
        return;
    }
    let n_gpu_layers: i32 = if env::var("TLDL_E2E_GPU").unwrap_or_default() == "1" {
        -1
    } else {
        0
    };
    let device = if n_gpu_layers < 0 { "GPU (E2E)" } else { "CPU" };

    let dir = std::env::temp_dir().join("tldl-e2e-resummarize");
    let _ = std::fs::remove_dir_all(&dir);
    let st = match storage::Storage::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : ouverture ({e})");
            std::process::exit(1);
        }
    };

    let text = "Un client entre dans une boulangerie et commande des viennoiseries \
                pour six personnes. Il règle 25,50 euros et remercie le serveur.";
    let json = format!(
        r#"{{"text":"{}","segments":[{{"start":0.0,"end":8.0,"text":"{}"}}],"language":"fr"}}"#,
        text, text
    );
    let file = "C:\\fake\\re.wav";
    let id1 = match st.save(
        file,
        storage::now_epoch(),
        42.0,
        "small",
        &llm,
        device,
        "Contexte par défaut",
        text,
        &json,
        "## Synthèse\nRésumé de référence.",
        0.0,
        42.0,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : save ({e})");
            std::process::exit(1);
        }
    };

    // Relecture de la transcription stockée (le chemin exact du re-résumé).
    let rec = match st.get(id1) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : get ({e})");
            std::process::exit(1);
        }
    };
    let tr: asr::Transcription = match serde_json::from_str(rec.transcription_json.as_deref().unwrap_or("")) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : relecture du JSON ({e})");
            std::process::exit(1);
        }
    };

    let ctx = "Résume en 3 phrases maximum, sans listes.";
    let mut last_step = String::new();
    let out = pipeline::re_summary(&tr, &llm, 0.3, &ctx, n_gpu_layers, &mut |p| {
        if p.step != last_step {
            last_step = p.step.clone();
            println!("  [étape] {}", p.step);
        }
    });
    let summary = match out {
        Ok(md) => md,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : {e}");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    if summary.trim().is_empty() {
        eprintln!("ÉCHEC E2E resummarize : résumé vide");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    println!("résumé produit :");
    println!("{}", summary);

    // Nouvel enregistrement : même transcription (reportée à l'identique),
    // nouveau résumé + nouveau contexte.
    let id2 = match st.save(
        file,
        storage::now_epoch(),
        42.0,
        "small",
        &llm,
        device,
        "E2E re-résumé",
        text,
        &json,
        &summary,
        0.0,
        42.0,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : save du re-résumé ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    let rows = match st.find_by_file(file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E resummarize : find_by_file ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    if rows.len() != 2
        || rows[0].id != id2
        || rows[1].id != id1
        || rows[0].transcription != text
        || rows[0].transcription_json.as_deref() != Some(json.as_str())
        || rows[0].context_name != "E2E re-résumé"
    {
        eprintln!("ÉCHEC E2E resummarize : find_by_file inattendu ({rows:?})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("E2E resummarize OK : re-résumé LLM seul, transcription réutilisée (jamais l'historique réel)");
}

/// E2E des dossiers de rangement F19 (usage : `tldl-e2e folders`).
///
/// Va-et-retour factice dans un dossier temporaire (jamais l'historique réel
/// de l'app) : create → save → move → list (comptes) → sortie vers « Sans
/// dossier » → delete (sans perte), puis nettoyage.
fn run_folders_e2e() {
    use tldl_lib::storage;

    let dir = std::env::temp_dir().join("tldl-e2e-folders");
    let _ = std::fs::remove_dir_all(&dir);
    let st = match storage::Storage::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : ouverture ({e})");
            std::process::exit(1);
        }
    };

    let json = r#"{"text":"Bonjour le monde.","segments":[],"language":"fr"}"#;
    let mk = |file: &str, ctx: &str, text: &str, sum: &str| {
        st.save(
            file,
            storage::now_epoch(),
            10.0,
            "small",
            "gemma3-4b-q4",
            "CPU",
            ctx,
            text,
            json,
            sum,
            0.0,
            10.0,
        )
    };
    let a = match mk(
        "C:\\fake\\audio1.wav",
        "Contexte par défaut",
        "premier texte",
        "résumé A",
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : save ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    let b = match mk("C:\\fake\\video2.mp4", "Contrôle qualité", "deuxième texte", "résumé B") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : save ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };

    let f1 = match st.create_folder("Comptes rendus") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : create ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };

    // Rangement + compte du dossier.
    if let Err(e) = st.move_record(a, Some(f1)) {
        eprintln!("ÉCHEC E2E folders : move a -> dossier ({e})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    let folders = match st.list_folders() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : list ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    if folders.len() != 1 || folders[0].id != f1 || folders[0].count != 1 {
        eprintln!("ÉCHEC E2E folders : list inattendue ({folders:?})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    // Contenu du dossier + « Sans dossier ».
    let in_f1 = match st.list_in_folder(Some(f1), 50) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : list_in_folder ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    if in_f1.len() != 1 || in_f1[0].id != a || in_f1[0].folder_id != Some(f1) {
        eprintln!("ÉCHEC E2E folders : contenu du dossier inattendu ({in_f1:?})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    let sans = match st.list_in_folder(None, 50) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ÉCHEC E2E folders : « Sans dossier » ({e})");
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    };
    if sans.len() != 1 || sans[0].id != b {
        eprintln!("ÉCHEC E2E folders : « Sans dossier » inattendu ({sans:?})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    // Sortie vers « Sans dossier » (un seul dossier par enregistrement).
    if let Err(e) = st.move_record(a, None) {
        eprintln!("ÉCHEC E2E folders : move a -> « Sans dossier » ({e})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    if st.list_in_folder(None, 50).unwrap().len() != 2 {
        eprintln!("ÉCHEC E2E folders : « Sans dossier » incomplet après sortie");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    // Rangement de groupe (F19) : tous les résumés d'un même fichier
    // (même transcription) d'un seul coup — même si le groupe est parti
    // (c1 rangé dans le dossier, c2 « Sans dossier »).
    let fail = |msg: &str| -> ! {
        eprintln!("ÉCHEC E2E folders : {msg}");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    };
    let c1 = match mk(
        "C:\\fake\\audio3.wav",
        "Contexte par défaut",
        "troisième texte",
        "résumé C1",
    ) {
        Ok(v) => v,
        Err(e) => fail(&format!("save c1 ({e})")),
    };
    let c2 = match mk(
        "C:\\fake\\audio3.wav",
        "Contrôle qualité",
        "quatrième texte",
        "résumé C2",
    ) {
        Ok(v) => v,
        Err(e) => fail(&format!("save c2 ({e})")),
    };
    if let Err(e) = st.move_record(c1, Some(f1)) {
        fail(&format!("move c1 -> dossier ({e})"));
    }
    let n = match st.move_file_records("C:\\fake\\audio3.wav", Some(f1)) {
        Ok(v) => v,
        Err(e) => fail(&format!("move groupe -> dossier ({e})")),
    };
    if n != 2
        || st.get(c1).unwrap().folder_id != Some(f1)
        || st.get(c2).unwrap().folder_id != Some(f1)
    {
        fail(&format!("groupe non entier après move ({n})"));
    }
    if st.move_file_records("C:\\fake\\inexistant.wav", Some(f1)).is_ok() {
        fail("move de groupe inconnu aurait dû échouer");
    }
    if st
        .move_file_records("C:\\fake\\audio3.wav", Some(9_999_999))
        .is_ok()
    {
        fail("move groupe vers dossier inconnu aurait dû échouer");
    }
    if let Err(e) = st.move_file_records("C:\\fake\\audio3.wav", None) {
        fail(&format!("move groupe -> « Sans dossier » ({e})"));
    }
    if st.list_in_folder(None, 50).unwrap().len() != 4 {
        fail("« Sans dossier » incomplet après sortie du groupe");
    }

    // Cas d'erreur : dossier inconnu, enregistrement inconnu.
    if st.move_record(b, Some(9_999_999)).is_ok() {
        eprintln!("ÉCHEC E2E folders : move vers dossier inconnu aurait dû échouer");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    if st.move_record(9_999_999, None).is_ok() {
        eprintln!("ÉCHEC E2E folders : move d'enregistrement inconnu aurait dû échouer");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    // Suppression : les enregistrements repassent « Sans dossier »
    // (jamais supprimés) ; double suppression = erreur.
    if let Err(e) = st.delete_folder(f1) {
        eprintln!("ÉCHEC E2E folders : delete ({e})");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    if st.delete_folder(f1).is_ok() {
        eprintln!("ÉCHEC E2E folders : double delete aurait dû échouer");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    if !st.list_folders().unwrap().is_empty() {
        eprintln!("ÉCHEC E2E folders : dossiers résiduels");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }
    if st.get(a).unwrap().folder_id.is_some() {
        eprintln!("ÉCHEC E2E folders : a encore rangé après delete");
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(1);
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("E2E folders OK : create/move/list/delete sans perte (dossier temporaire, jamais l'historique réel)");
}

/// E2E de la coupe de section F14 (usage : `tldl-e2e section [fichier_média]
/// [dossier_ffmpeg]`).
///
/// Deux passes de pipeline réel sur le même modèle ASR :
/// 1) le **fichier complet** (sonde : durée, texte de référence) ;
/// 2) une **section** `[20 %, 80 %]` du fichier : la WAV intermédiaire ne
///    contient que la section, les horodatages émis sont relatifs à la
///    section puis **décalés du début** pour rester alignés sur le fichier
///    source.
/// Assertions : bornes reportées, segments contenus dans `[début, fin]`
/// (tolérance 0,5 s), transcription non vide et plus courte que le complet.
/// Le LLM est optionnel : absent → dégradation gracie (la transcription est
/// le sujet).
fn run_section_e2e(rest: &[String]) {
    use tldl_lib::models;

    // input : argv[0] ou TLDL_E2E_INPUT ; ffmpeg : argv[1] ou TLDL_FFMPEG_DIR.
    let input = rest
        .first()
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("TLDL_E2E_INPUT").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let ffmpeg = rest
        .get(1)
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("TLDL_FFMPEG_DIR").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    if input.is_empty() || !Path::new(&input).exists() {
        println!(
            "E2E section ignorée : input manquant.\n  \
             (passer un fichier en argv[1] ou TLDL_E2E_INPUT ; \
             TLDL_FFMPEG_DIR pour le FFmpeg embarqué)"
        );
        return; // exit 0
    }
    if !ffmpeg.is_empty() {
        env::set_var("TLDL_FFMPEG_DIR", &ffmpeg);
    }
    // Le modèle ASR est requis (transcription réelle) : absent → ignorée.
    let asr_id = env::var("TLDL_E2E_ASR")
        .unwrap_or_else(|_| String::from("small"))
        .trim()
        .to_string();
    let asr_path = models::model_path(models::ModelKind::Asr, &asr_id);
    if !asr_path.exists() {
        println!("E2E section ignorée : modèle ASR {asr_id} absent (menu Modèles, TLDL_E2E_ASR) — attendu : {asr_path:?}");
        return;
    }

    // Le LLM est optionnel ici : absent → dégradation gracie (summary_error).
    let mk = |section_start: f64, section_end: f64| PipelineOptions {
        asr_model: asr_id.clone(),
        llm_model: "qwen25-7b-q4".into(),
        language: "auto".into(),
        temperature: 0.3,
        use_gpu: false,
        n_gpu_layers: 0,
        device: "CPU".into(),
        device_note: None,
        context_instructions: String::new(),
        section_start,
        section_end,
    };

    let mut last_step = String::new();
    let mut step = |p: &pipeline::Progress| {
        if p.step != last_step {
            last_step = p.step.clone();
            println!("  [étape] {}", p.step);
        }
    };

    // Passe 1 : fichier complet — la durée sonde cale les bornes de la
    // section, le texte sert de référence.
    println!("E2E section : {}", input);
    let full = match pipeline::run(&input, &mk(0.0, 0.0), &mut step) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ÉCHEC E2E section : pipeline fichier complet ({e})");
            std::process::exit(1);
        }
    };
    if full.transcription.segments.is_empty() {
        eprintln!("ÉCHEC E2E section : transcription complète vide");
        std::process::exit(1);
    }
    let duration = full.duration_secs;
    if duration <= 0.0 {
        eprintln!("ÉCHEC E2E section : durée sonde nulle ({duration})");
        std::process::exit(1);
    }
    let (start, end) = (duration * 0.2, duration * 0.8);
    println!(
        "      section [{:.2}s, {:.2}s] (fichier {:.2}s)  asr={asr_id}  gpu=false",
        start, end, duration
    );

    // Passe 2 : la section seule.
    let out = match pipeline::run(&input, &mk(start, end), &mut step) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ÉCHEC E2E section : pipeline section ({e})");
            std::process::exit(1);
        }
    };
    println!("segments : {}", out.transcription.segments.len());
    for s in &out.transcription.segments {
        println!("  [{:>7.2}s - {:>7.2}s] {}", s.start, s.end, s.text);
    }
    println!("texte :");
    println!("  {}", out.transcription.text);

    // 1) Les bornes de la section sont reportées à l'identique.
    if (out.section_start - start).abs() > 1e-6 || (out.section_end - end).abs() > 1e-6 {
        eprintln!(
            "ÉCHEC E2E section : bornes non reportées ({} → {}, attendu {} → {})",
            out.section_start, out.section_end, start, end
        );
        std::process::exit(1);
    }
    // 2) Tous les segments sont **dans** la section (décalés du début, donc
    //    alignés sur le fichier source ; tolérance 0,5 s de bord de segment).
    let tol = 0.5;
    for s in &out.transcription.segments {
        if s.start < start - tol || s.end > end + tol {
            eprintln!(
                "ÉCHEC E2E section : segment hors section [{:.2}s - {:.2}s], attendu [{}, {}]",
                s.start, s.end, start, end
            );
            std::process::exit(1);
        }
    }
    // 3) La transcription n'est pas vide.
    if out.transcription.segments.is_empty() || out.transcription.text.trim().is_empty() {
        eprintln!("ÉCHEC E2E section : transcription vide");
        std::process::exit(1);
    }
    // 4) Le texte de section est strictement plus court que le complet.
    if out.transcription.text.len() >= full.transcription.text.len() {
        eprintln!(
            "ÉCHEC E2E section : la transcription de section n'est pas plus courte que le fichier complet ({} ≥ {} octets)",
            out.transcription.text.len(),
            full.transcription.text.len()
        );
        std::process::exit(1);
    }
    // Contrôle souple (affichage) : le texte de section doit « couler » dans
    // le texte complet (même modèle, même décodage) — borne de coupe au
    // milieu d'un mot, la sous-séquence exacte peut ne pas tenir.
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    };
    let contained = norm(&full.transcription.text).contains(&norm(&out.transcription.text));
    println!(
        "contrôle contenu : {} (texte section ⊂ texte complet)",
        if contained { "OK" } else { "approximatif (borne de coupe ?)" }
    );

    println!("E2E section OK : WAV de la section seule, segments dans [{:.2}s, {:.2}s], texte plus court que le complet", start, end);
}

/// E2E de la passe réelle LLM (carry LLM) : usage `tldl-e2e llm`.
///
/// Vraie génération sur le vrai modèle (`TLDL_E2E_LLM`, défaut
/// `qwen25-7b-q4`) : contexte « Résume en 10 phrases maximum. » →
/// `llm::summarize` (lance le binaire `tldl-llm` réel, même dossier que
/// celui-ci), sur une transcription de test **longue** (≈ 700 tokens —
/// bien au-delà du `n_batch` 512 : régression du `GGML_ASSERT` sur un
/// fichier de 15 min dans l'app). Exigence dure : EOS naturel (la génération
/// s'arrête avant
/// `max_tokens`), pas de méta-préambule, au plus 10 phrases (heuristique
/// [.!?]) — le filet anti-boucle `truncate_repetition` est en place
/// (cf. `llm/mod.rs`). Modèle absent → ignorée (exit 0), comme les autres
/// modes.
fn run_llm_e2e() {
    use tldl_lib::asr::Transcription;
    use tldl_lib::llm::{self, LlmConfig};
    use tldl_lib::models;

    let llm_id = env::var("TLDL_E2E_LLM")
        .unwrap_or_else(|_| String::from("qwen25-7b-q4"))
        .trim()
        .to_string();
    if !models::model_path(models::ModelKind::Llm, &llm_id).exists() {
        println!("E2E llm ignorée : modèle LLM {llm_id} absent (TLDL_E2E_LLM)");
        return;
    }

    // Transcription longue (≈ 700 tokens — bien au-delà du `n_batch` 512,
    // cf. régression `GGML_ASSERT(n_tokens_all <= n_batch)` sur un fichier
    // de 15 min) : plusieurs chapitres de réunion, tous distincts.
    let text = "Nous avons passé en revue la feuille de route du \
               troisième trimestre. L'équipe a validé la migration vers le \
               nouveau prestataire de paiement, avec une échéance fixée à la \
               fin septembre. La sécurité était la préoccupation principale : \
               le prestataire doit être certifié SOC 2. Nous avons décidé de \
               lancer le spike technique lundi. Une démo sera programmée fin \
               mois pour le comité de pilotage. Aucun budget n'a été voté, en \
               attendant les premières factures. La prochaine étape est la \
               mise en place des tests d'intégration. Le point d'avancement \
               aura lieu toutes les deux semaines. \
               On a aussi fait le point sur la bascule des environnements \
               de recette. L'infrastructure doit être dupliquée avant la fin \
               du sprint, et les variables de configuration seront \
               centralisées dans un dépôt à part. Deux ingénieurs seront \
               dédiés à la bascule, avec un retour d'expérience à chaque \
               étape. En cas de régression bloquante, l'équipe peut revenir \
               en arrière sans impact pour les clients. \
               Le volet monitoring a été détaillé : les alertes critiques \
               partiront vers la garde, et les seuils de latence seront \
               revus chaque mois. Un tableau de bord sera tenu à jour pour \
               le comité, avec la qualité, le volume et les incidents. Les \
               valeurs de référence seront fixées d'ici la fin du mois, à \
               partir des mesures réelles de production. \
               Sur la donnée, l'export historique sera géré par un script de \
               migration, et la double écriture restera en place le temps \
               d'un cycle complet. La conformité RGPD a été rappelée : les \
               données personnelles seront pseudonymisées avant tout \
               transfert. Un audit sera demandé au prestataire avant \
               l'ouverture de la production. \
               Le plan de communication interne sera envoyé à l'ensemble des \
               équipes avant la fin de semaine, avec les consignes de prise \
               en main. Les équipes métier auront une session de formation de \
               deux heures, animée par les responsables de projet. Les \
               supports de formation seront mis en ligne dans l'intranet, et \
               chaque participant recevra le replay. \
               Les risques ont été passés en revue : le principal reste la \
               dépendance au certificat du prestataire, et un plan B est \
               prévu avec un sous-traitant de repli. Le calendrier sera tenu \
               au plus près, avec un point hebdomadaire entre les deux \
               points de pilotage. La charge de la décision revient au comité \
               de direction, qui se prononcera avant la fin du mois. \
               Côté organisation, chaque équipe remonte ses charges réelles \
               chaque vendredi, et les arbitrages seront notés dans le \
               suivi de sprint. Les indicateurs de qualité seront suivis sur \
               l'ensemble du trimestre, avec un objectif de disponibilité à \
               fixer d'ici la fin du mois. Les écarts seront discutés en \
               revue de production, et les actions correctives feront l'objet \
               d'un suivi nominatif. \
               Pour conclure, les prochaines étapes sont arrêtées : le spike \
               technique démarre lundi, la démo est fixée à la fin du mois, \
               et le comité de pilotage se réunira après la bascule. Les \
               points de blocage remontent directement au chef de projet, \
               qui consolide l'ensemble avant chaque point d'avancement. La \
               prochaine feuille de route sera arbitrée au premier \
               trimestre suivant, en fonction des résultats mesurés.";
    let tr = Transcription {
        text: text.into(),
        segments: Vec::new(),
        language: "fr".into(),
    };
    let ctx = "Résume en 10 phrases maximum.";
    let cfg = LlmConfig {
        model: llm_id.clone(),
        temperature: 0.3,
        max_tokens: 1024,
    };

    let mut last = (0u32, 0u32);
    let summary = match llm::summarize(&tr, &cfg, ctx, 0, &mut |n, max| last = (n, max)) {
        Ok(md) => md,
        Err(e) => {
            eprintln!("ÉCHEC E2E llm : {e}");
            std::process::exit(1);
        }
    };
    let (produced, max_tokens) = last;
    println!("tokens produits : {produced} / {max_tokens}");
    println!("résumé produit :");
    println!("{summary}");

    // 0) Non vide.
    if summary.trim().is_empty() {
        eprintln!("ÉCHEC E2E llm : résumé vide");
        std::process::exit(1);
    }
    // 1) Pas de méta-préambule.
    let lower = summary.to_lowercase();
    let preamble = ["voici le résumé", "voici un résumé", "voici la transcription"]
        .iter()
        .any(|p| lower.contains(p));
    if preamble {
        eprintln!("ÉCHEC E2E llm : méta-préambule détecté");
        std::process::exit(1);
    }
    // 2) EOS naturel : la génération s'arrête avant max_tokens.
    if produced >= max_tokens {
        eprintln!(
            "ÉCHEC E2E llm : pas d'EOS naturel (génération arrivée à max_tokens={max_tokens})"
        );
        std::process::exit(1);
    }
    // 3) Au plus 10 phrases (heuristique sur la ponctuation terminale).
    let sentences = summary
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .count();
    if sentences > 10 {
        eprintln!("ÉCHEC E2E llm : {sentences} phrases (exigence : 10 maximum)");
        std::process::exit(1);
    }

    println!(
        "E2E llm OK : passe réelle « 10 phrases » — EOS naturel à {produced} tokens, sans méta-préambule, {sentences} phrase(s)"
    );
}
