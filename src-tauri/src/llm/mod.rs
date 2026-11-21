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

/// Prompt de résumé — docs/architecture.md §5.1 — avec contexte de prompt
/// utilisateur (F15).
///
/// - `context` vide : format « rencontre » par défaut (Synthèse / Points
///   clés / Décisions / Actions à venir).
/// - `context` non vide : le contexte **oriente le résumé** — il *remplace*
///   le format par défaut et devient l'instruction de synthèse (ex. « résume
///   en 10 phrases »). La transcription reste fidèle ; seul le résumé est
///   dirigé, jamais la transcription.
pub fn build_prompt(transcription: &Transcription, context: &str) -> String {
    let mut s = String::new();
    s.push_str("Voici la transcription horodatée d'une rencontre :\n\n");
    s.push_str(&transcription.text);
    s.push_str("\n\n");
    let ctx = context.trim();
    if ctx.is_empty() {
        s.push_str("Tu es un assistant de synthèse de rencontre.\n");
        s.push_str("Produis un résumé en Markdown avec exactement ces sections :\n");
        s.push_str("## Synthèse\n(un paragraphe, 3 à 5 phrases)\n");
        s.push_str("## Points clés\n(puces)\n");
        s.push_str("## Décisions\n(puces ; « aucune » si rien)\n");
        s.push_str(
            "## Actions à venir\n(puces : qui · quoi · échéance si mentionnée ; « aucune » si rien)\n",
        );
    } else {
        s.push_str(
            "Résume la transcription en suivant strictement ces instructions :\n\n",
        );
        s.push_str(ctx);
        s.push_str("\n");
    }
    s.push_str(
        "Règles : reste dans la langue de la transcription, n'invente rien ; \
          si tu doutes, dis-le explicitement. Produis directement le résumé, \
          sans préambule ni reformulation des règles.",
    );
    s
}

/// Dictionnaire de mots de liaison FR/EN (heuristic de détection de boucle).
///
/// Le token est un run alphanumérique (les apostrophes/ponctuations coupent) ;
/// on retire ce qui reste de « bruit » pour ne comparer que le *contenu*
/// (les faits), pas la grammaire.
fn stop_words() -> std::collections::HashSet<&'static str> {
    [
        "le", "la", "les", "l", "d", "de", "des", "du", "un", "une", "et", "ou",
        "mais", "donc", "or", "ni", "car", "que", "qui", "quoi", "dont", "dans",
        "pour", "par", "sur", "sous", "avec", "sans", "chez", "vers", "comme",
        "quand", "comment", "pourquoi", "o", "ainsi", "alors", "a", "as", "est",
        "sont", "etre", "avoir", "fait", "faire", "peut", "peux", "peu", "plus",
        "moins", "aussi", "tres", "bien", "mal", "tout", "toute", "tous", "toutes",
        "même", "memes", "aucun", "aucune", "autre", "autres", "certain", "certains",
        "certaine", "certaines", "ce", "cette", "ces", "ca", "cela", "celui", "ceux",
        "celles", "ceci", "lui", "eux", "elles", "ils", "on", "je", "tu", "il",
        "elle", "nous", "vous", "moi", "toi", "soi", "se", "te", "me", "y", "en",
        "ne", "mon", "ta", "son", "sa", "ses", "notre", "votre", "leurs", "leur",
        "nos", "vos", "mes", "tes", "the", "an", "so", "if", "in", "at", "to",
        "of", "for", "with", "into", "onto", "from", "by", "is", "are", "was",
        "were", "be", "been", "being", "have", "has", "had", "having", "do",
        "does", "did", "will", "would", "can", "could", "should", "may", "might",
        "must", "this", "that", "these", "those", "it", "its", "he", "she",
        "they", "we", "you", "my", "your", "our", "his", "her", "their", "us",
        "them", "who", "whom", "what", "which", "when", "where", "why", "how",
        "not", "no", "also", "than", "too", "very", "just", "only", "both",
        "each", "every", "some", "any", "such", "same", "about", "after",
        "before", "between", "through", "during", "again", "further", "once",
        "here", "there", "up", "down", "out", "off", "over", "under", "new",
        "old", "first", "second", "third", "next", "last", "more", "most",
        "other", "another", "many", "much", "few", "less", "least",
    ]
    .iter()
    .copied()
    .collect()
}

/// Mots de contenu d'un fragment : runs alphanumériques (≥ 2), minuscules,
/// mots de liaison retirés. Alimente la détection de boucle (paraphrase).
fn content_words(s: &str) -> std::collections::HashSet<String> {
    let stop = stop_words();
    let mut set = std::collections::HashSet::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            let mut lc = ch.to_lowercase();
            if let Some(f) = lc.next() {
                cur.push(f);
            }
        } else {
            if cur.chars().count() >= 2 && !stop.contains(cur.as_str()) {
                set.insert(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.chars().count() >= 2 && !stop.contains(cur.as_str()) {
        set.insert(cur);
    }
    set
}

/// Similarité de Jaccard (|∩| / |∪|) entre deux ensembles de mots.
fn jaccard(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    inter / union
}

/// Découpe un texte en paragraphes (blocs de lignes non vides, séparés par
/// ≥ 1 ligne vide) — la structure que prend le résumé quand il dégénère.
fn paragraphs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                out.push(cur.join("\n"));
                cur.clear();
            }
        } else {
            cur.push(line.to_string());
        }
    }
    if !cur.is_empty() {
        out.push(cur.join("\n"));
    }
    out
}

/// Coupe une **boucle de répétition** du modèle (le résumé re-récité, en le
/// *paraphrasant*, jusqu'à `max_tokens` sans EOS).
///
/// Même avec le chat template (appliqué côté shim C, cf. `engine.rs`),
/// sur certaines entrées le modèle re-synthétise le même contenu au lieu
/// d'émettre l'EOS. La pénalité de
/// répétition du sampling (cf. `engine.rs` / shim C) est *tokenique* : elle
/// casse une boucle de répétition *mot à mot*, pas une boucle **paraphrasée**
/// (les tokens diffèrent à chaque cycle). On détecte donc la boucle **après**
/// génération, sur la structure du texte : le résumé dégénéré se dépose en
/// paragraphes (lignes vides) qui **re-restatent la même scène** (les faits —
/// produits, montants, personnes).
///
/// Signal : un paragraphe « paraphrase » un antérieur si leur similarité de
/// *mots de contenu* (Jaccard, mots de liaison retirés) est ≥ 0.4 — une
/// reformulation d'une même scène se reconnaise malgré la paraphrase, alors
/// que des sections distinctes d'un résumé sain (Synthèse / Points clés / …)
/// ne se ressemblent pas. On ne tronque que si la répétition est **soutenue**
/// (≥ 2 paragraphes qui paraphrasent, et ≥ la moitié de l'ensemble) : c'est ce
/// qui distingue une boucle d'un résumé multi-sections honnête. La coupe est
/// posée au 1er paragraphe qui paraphrase — le préfixe conservé est le résumé
/// propre. Ne se déclenche qu'à ≥ 3 paragraphes. Retourne le texte intact sinon.
fn truncate_repetition(text: &str) -> String {
    let paras = paragraphs(text);
    let n = paras.len();
    if n < 3 {
        return text.to_string();
    }
    let sigs: Vec<std::collections::HashSet<String>> = paras.iter().map(|p| content_words(p)).collect();
    let mut hit = 0usize;
    let mut first_dup_at = None;
    for j in 1..n {
        if sigs[j].len() < 10 {
            continue; // paragraphe trop court pour être un résumé.
        }
        let best = (0..j).fold(0.0f64, |m, i| m.max(jaccard(&sigs[i], &sigs[j])));
        if best >= 0.4 {
            hit += 1;
            if first_dup_at.is_none() {
                first_dup_at = Some(j);
            }
        }
    }
    if let Some(at) = first_dup_at {
        if hit >= 2 && hit as f64 / n as f64 >= 0.5 {
            return paras[..at].join("\n\n");
        }
    }
    text.to_string()
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
            "modèle LLM introuvable : {} — téléchargez-le ou déposez le \
             GGUF dans models/",
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
    // Boucle de répétition du modèle (paraphrase du même résumé, sans EOS) :
    // on tronque à la 1re occurrence — le 1er paragraphe est le résumé propre.
    let text = truncate_repetition(&text);
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{build_prompt, truncate_repetition};
    use crate::asr::Transcription;

    fn tr() -> Transcription {
        Transcription {
            text: "Bonjour. On a discuté du planning et de la suite.".into(),
            segments: Vec::new(),
            language: "fr".into(),
        }
    }

    // F15 : contexte vide = format « rencontre » par défaut.
    #[test]
    fn default_context_uses_structured_format() {
        let p = build_prompt(&tr(), "");
        assert!(p.contains("## Synthèse"));
        assert!(p.contains("## Points clés"));
        assert!(p.contains("## Décisions"));
        assert!(p.contains("## Actions à venir"));
        // Pas d'orientation (aucun contexte fourni).
        assert!(!p.contains("suivant strictement ces instructions"));
    }

    // F15 : un contexte ORIENTE le résumé — il remplace le format par défaut.
    #[test]
    fn context_orients_the_summary() {
        let p = build_prompt(&tr(), "Résume en 10 phrases maximum.");
        // L'instruction de l'utilisateur est présente…
        assert!(p.contains("Résume en 10 phrases maximum."));
        assert!(p.contains("suivant strictement ces instructions"));
        // …et le format structuré par défaut est *retiré* (le contexte prévaut).
        assert!(!p.contains("## Synthèse"));
        assert!(!p.contains("## Points clés"));
        assert!(!p.contains("## Décisions"));
        assert!(!p.contains("## Actions à venir"));
        // La transcription reste bien dans le prompt (jamais modifiée).
        assert!(p.contains("On a discuté du planning et de la suite."));
    }

    // Boucle de répétition (cas réel « résumé en 10 phrases ») : le modèle
    // re-récite le même résumé — à l'identique (tokens) ou *paraphrasé* —
    // jusqu'à `max_tokens` sans EOS. On tronque au 1er bloc, toujours le
    // résumé propre.
    const BLOCK_A: &str = "Un homme demande à commander des produits dans un \
                           établissement. Il commande trois croissants au beurre, un \
                           pain au raisin et un pain au chocolat. Il souhaite également \
                           une tarte aux fraises. Le serveur lui indique que la commande \
                           est pour six personnes et lui facture 25,50 euros. L'homme \
                           remercie le serveur et part.";
    const BLOCK_A2: &str = "Un homme entre dans un établissement et commande trois \
                            croissants au beurre, un pain au raisin et un pain au \
                            chocolat. Il souhaite également une tarte aux fraises. Le \
                            serveur lui facture 25,50 euros pour six personnes. L'homme \
                            remercie le serveur et part.";

    // Boucle pure (bloc identique re-récité) : un seul bloc doit rester.
    #[test]
    fn cuts_pure_repetition_loop() {
        let looped = format!("{} \n\n{} \n\n{}", BLOCK_A, BLOCK_A, BLOCK_A);
        let out = truncate_repetition(&looped);
        let nparas = out.split("\n\n").filter(|p| !p.trim().is_empty()).count();
        assert_eq!(nparas, 1, "doit ne rester qu'un seul bloc : {out}");
        assert!(out.contains("Un homme demande à commander"));
    }

    // Boucle paraphrasée (bloc reformulé) : la similarité des *mots de
    // contenu* doit la saisir malgré la paraphrase.
    #[test]
    fn cuts_paraphrase_repetition_loop() {
        let looped = format!("{} \n\n{} \n\n{} \n\n{} \n\n{}", BLOCK_A, BLOCK_A2, BLOCK_A, BLOCK_A2, BLOCK_A);
        let out = truncate_repetition(&looped);
        let nparas = out.split("\n\n").filter(|p| !p.trim().is_empty()).count();
        assert_eq!(nparas, 1, "doit ne rester qu'un seul bloc : {out}");
        assert!(out.contains("Un homme demande à commander"));
        assert!(!out.contains("Un homme entre dans"));
    }

    // Boucle avec auto-commentaire (cas réel GPU du 19/11 : A B A B A…) :
    // le 1er bloc est conservé, les reformulations jetées.
    #[test]
    fn cuts_loop_with_self_comment() {
        let a = "Un homme achète des produits dans une boulangerie. Il demande trois \
                croissants au beurre, un pain au raisin et un pain au chocolat. Il \
                choisit ensuite une tarte aux fraises. Le montant total est de 25,50 \
                euros pour six personnes. Il remercie la personne qui lui a servi. La \
                conversation se termine par une bonne journée pour tous.";
        let b = "Il y a eu une légère inexactitude dans le résumé initial. L'homme a \
                 demandé un pain au chocolat, non un pain aux noisettes. Le reste est \
                 correct.";
        let a2 = "Le client demande trois croissants au beurre, un pain au raisin, un \
                  pain au chocolat et une tarte aux fraises pour six personnes. Il paie \
                  25,50 euros et quitte le boulanger.";
        let a3 = "Le client achète trois croissants au beurre, un pain au raisin, un \
                  pain au chocolat et une tarte aux fraises pour six personnes. Il paie \
                  25,50 euros et quitte le boulanger.";
        let looped = format!("{} \n\n{} \n\n{} \n\n{} \n\n{}", a, b, a2, b, a3);
        let out = truncate_repetition(&looped);
        // Le 1er bloc (A) est intact ; les reformulations (a2, a3) sont jetées.
        assert!(out.contains("Un homme achète des produits"));
        assert!(!out.contains("Le client demande"));
        assert!(!out.contains("Le client achète"));
    }

    // Un résumé *sain* multi-paragraphes (sections distinctes) n'est pas
    // touché : pas de répétition soutenue.
    #[test]
    fn leaves_healthy_multisection_summary() {
        let text = "L'équipe a planifié le déploiement du nouveau service pour la fin du \
                   trimestre, en insistant fortement sur la qualité et les délais de \
                   livraison.\n\n\
                   Le déploiement suit un rythme de livraison rigoureux, avec un suivi \
                   des retours clients et des corrections apportées chaque semaine.\n\n\
                   Aucun budget supplémentaire n'est voté pour l'instant, le comité \
                   attend les chiffres de rentabilité avant de trancher.";
        let out = truncate_repetition(&text);
        assert_eq!(out, text, "le résumé sain ne doit pas être modifié");
    }

    // Un résumé court (1-2 paragraphes) n'est jamais modifié.
    #[test]
    fn leaves_short_summary() {
        let one = "Résumé court d'une rencontre simple, sans rien à signaler.";
        assert_eq!(truncate_repetition(one), one);
        let two = format!("{} \n\n Un second paragraphe distinct, sans rien à signaler non plus.", one);
        assert_eq!(truncate_repetition(&two), two);
    }
}
