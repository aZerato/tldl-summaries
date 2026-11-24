//! Diarisation (F9) — séparation des intervenants, 100 % locale (ADR-008).
//!
//! ONNX Runtime **chargé au runtime** (feature `load-dynamic` de `ort` :
//! dlopen du prébuild embarqué `assets/onnxruntime/<cible>/` — pattern
//! FFmpeg de `media.rs`, pas de lien statique ni de build par plateforme).
//! Frontend fbank Wespeaker bit-exact (`fbank`), embeddings ONNX, puis
//! **clustering en Rust** (pas de pyannote / pas de PyTorch).
//!
//! Dégradation gracie : ORT ou modèle absent → pas d'intervenant
//! (la transcription reste intacte).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

pub mod fbank;

/// Nombre de bandes mel du frontend Wespeaker (input ONNX `feats` [B,T,80]).
pub const NUM_BINS: usize = 80;
/// Dimension de l'embedding produit par le modèle (sortie `embs` [B,192]).
pub const EMBEDDING_DIM: usize = 192;
/// Durée d'une frame : 10 ms (saut 160 échantillons à 16 kHz).
const FRAME_SECS: f64 = 0.01;
/// Minimum de frames fbank pour qu'un segment soit embedded (1 s) :
/// en dessous, pas d'étiquette pour le segment (dégradation gracie).
const MIN_SEG_FRAMES: usize = 100;
/// Saut minimal de la courbe de fusion pour la détection auto du nombre
/// d'intervenants (`auto_k`) : en dessous, la courbe est lisse → voix
/// unique. Arrêté au bench F12 (docs/models.md §7) — seul, le saut
/// surestimait les fichiers 1-voix : la coupe n'est acceptée qu'avec
/// l'**effondrement** de la courbe (`COLLAPSE_RATIO`, ci-dessous).
pub const MIN_GAP: f32 = 0.05;
/// Seuil d'**effondrement** de la courbe de fusion (`auto_k`) : la
/// coupe (saut ≥ `MIN_GAP`) n'est acceptée que si la **dernière**
/// fusion est à ≤ `COLLAPSE_RATIO` × la **première** — la courbe
/// s'effondre si il y a plusieurs voix (ratio ≤ 0,50 : boulangerie
/// 0,25, synth 2/3 voix 0,50) et dérive doucement si 1 voix
/// (ratio ≥ 0,57). Marge au seuil 0,024–0,296 sur les 9 régimes du
/// bench F12 (docs/models.md §7).
const COLLAPSE_RATIO: f32 = 0.55;

/// Mode de clustering des embeddings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClusterMode {
    /// Nombre d'intervenants **auto** : le creux de la courbe de fusion
    /// (le plus grand saut ≥ `MIN_GAP`, accepté seulement si la courbe
    /// *effondre* : dernière fusion ≤ `COLLAPSE_RATIO` × première).
    Auto,
    /// Nombre forcé (N) : **exactement** N clusters — le choix explicite
    /// de l'utilisateur fait foi (il connaît son fichier).
    Count { n: u32 },
}

/// (triple de cible, nom du prébuild ONNX Runtime 1.31) de la plateforme.
fn ort_artifact() -> Option<(&'static str, &'static str)> {
    use std::env::consts;
    match (consts::FAMILY, consts::ARCH, consts::OS) {
        ("windows", "x86_64", _) => {
            Some(("x86_64-pc-windows-msvc", "onnxruntime.dll"))
        }
        ("unix", "aarch64", "macos") => {
            Some(("aarch64-apple-darwin", "libonnxruntime.1.31.0.dylib"))
        }
        ("unix", "x86_64", "linux") => {
            Some(("x86_64-unknown-linux-gnu", "libonnxruntime.so.1.31.0"))
        }
        ("unix", "aarch64", "linux") => {
            Some(("aarch64-unknown-linux-gnu", "libonnxruntime.so.1.31.0"))
        }
        _ => None,
    }
}

/// Chemin du prébuild ONNX Runtime de la cible courante, sans dépendre
/// d'une variable d'env : 1) `TLDL_ORT_DIR` (override — E2E/tests),
/// 2) bundle release : `<exe>/_up_/assets/onnxruntime/<cible>/` (resources),
/// 3) dev : remontée depuis l'exécutable jusqu'à `assets/onnxruntime/`.
/// `None` si introuvable (→ pas de diarisation, dégradation gracie).
pub fn resolve_ort_lib() -> Option<PathBuf> {
    let (triple, lib) = ort_artifact()?;
    if let Ok(dir) = std::env::var("TLDL_ORT_DIR") {
        let p = Path::new(&dir).join(lib);
        if p.exists() {
            return Some(p);
        }
    }
    let mut bases = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            bases.push(exe_dir.join("_up_").join("assets").join("onnxruntime"));
            let mut cur: Option<&Path> = Some(exe_dir);
            while let Some(d) = cur {
                bases.push(d.join("assets").join("onnxruntime"));
                cur = d.parent();
            }
        }
    }
    for base in &bases {
        let p = base.join(triple).join(lib);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Environnement ONNX Runtime **unique par processus**, au 1ᵉʳ usage.
/// `init_from` précède tout autre appel `ort` (dlopen de la lib), puis
/// `.build()` instancie l'environnement (telemetry off — C4). `None` si le
/// prébuild est absent/illisible → `Diarizer::load` refuse (dégradation).
static ENV: LazyLock<Option<Environment>> = LazyLock::new(|| {
    let lib = resolve_ort_lib()?;
    ort::init_from(&lib)
        .ok()
        .and_then(|b| b.with_telemetry(false).build().ok())
});

/// Diariseur : session ONNX chargée (une fois) + exécution.
pub struct Diarizer {
    session: Session,
}

impl Diarizer {
    /// Charge l'environnement ORT + le modèle d'embeddings ONNX.
    /// Err si ORT indisponible (prébuild absent) ou modèle illisible.
    pub fn load(model: &Path) -> Result<Self, String> {
        let env = match ENV.as_ref() {
            Some(e) => e.clone(),
            None => {
                return Err(
                    "ONNX Runtime indisponible (prébuild absent ou illisible) — \
                     diarisation désactivée."
                        .into(),
                )
            }
        };
        let mut sb = Session::builder(&env).map_err(|e| format!("ORT : {e}"))?;
        let session =
            sb.commit_from_file(model).map_err(|e| format!("chargement du modèle ONNX : {e}"))?;
        Ok(Self { session })
    }

    /// Une passe ONNX : `feats` [1,T,80] → `embs` [1,192] (aplati 192 f32).
    fn run_embedding(&mut self, data: Vec<f32>, t: usize) -> Result<Vec<f32>, String> {
        let shape = vec![1i64, t as i64, NUM_BINS as i64];
        let input = Tensor::from_array((shape, data)).map_err(|e| format!("tensor d'entrée : {e}"))?;
        let outputs = self
            .session
            .run(ort::inputs!["feats" => input])
            .map_err(|e| format!("inférence : {e}"))?;
        let out = outputs
            .get("embs")
            .ok_or_else(|| "pas de sortie `embs` sur le modèle".to_string())?;
        let arr = out
            .try_extract_array::<f32>()
            .map_err(|e| format!("sortie `embs` : {e}"))?;
        Ok(arr.iter().copied().collect())
    }

    /// Diarise `wav` (16 kHz mono i16) et affecte un intervenant à chaque
    /// segment (bornes **0-based**, alignées sur la WAV).
    ///
    /// **Un embedding par segment** (l'intera utterance : les frames de
    /// `[début, fin)`) et non par fenêtre fixe de 1,6 s : sur un court
    /// fichier, les fenêtres de 1,6 s de deux locuteurs sont quasi
    /// orthogonales l'une à l'autre, alors que l'embedding d'une utterance
    /// entière porte bien le signal locuteur.
    /// Les segments < `MIN_SEG_FRAMES` (1 s) n'ont pas d'étiquette
    /// (dégradation gracie). `progress` est appelé à chaque embedding ONNX
    /// avec (embeddings faits, segments totaux).
    pub fn run(
        &mut self,
        wav: &[i16],
        segments: &[(f64, f64)],
        mode: &ClusterMode,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<Option<String>>, String> {
        let fb = fbank::Fbank::new();
        let frames = fb.compute(wav);
        let n = frames.len();
        if n < MIN_SEG_FRAMES {
            return Ok(vec![None; segments.len()]);
        }
        // Embedding par segment : les frames de l'intervalle, global-mean
        // sur le segment entier (wespeaker), puis ONNX.
        let mut embs: Vec<Vec<f32>> = Vec::with_capacity(segments.len());
        let mut seg_of: Vec<usize> = Vec::with_capacity(segments.len());
        for (idx, &(s, e)) in segments.iter().enumerate() {
            let f0 = (s / FRAME_SECS) as usize;
            let f1 = (e / FRAME_SECS) as usize;
            let f0 = f0.min(n);
            let f1 = f1.min(n);
            if f1 - f0 < MIN_SEG_FRAMES {
                continue; // trop court → pas d'étiquette pour ce segment
            }
            let mut feats: Vec<[f32; NUM_BINS]> = frames[f0..f1].to_vec();
            fbank::global_mean(&mut feats);
            let data = win_flat(&feats);
            embs.push(self.run_embedding(data, f1 - f0)?);
            seg_of.push(idx);
            progress(embs.len(), segments.len());
        }
        if embs.is_empty() {
            return Ok(vec![None; segments.len()]);
        }
        let ids = cluster(&embs, mode);
        if std::env::var_os("TLDL_DIAR_DEBUG").is_some() {
            debug_report(&embs, &ids, mode);
        }
        // Chaque segment porte l'étiquette de son propre cluster.
        let mut out: Vec<Option<String>> = vec![None; segments.len()];
        for (i, &idx) in seg_of.iter().enumerate() {
            out[idx] = Some(speaker(ids[i]));
        }
        Ok(out)
    }
}

/// Aplatit les frames d'un segment en `T×80` f32 (ordre C, row-major).
fn win_flat(feats: &[[f32; NUM_BINS]]) -> Vec<f32> {
    let mut v = Vec::with_capacity(feats.len() * NUM_BINS);
    for row in feats {
        v.extend_from_slice(row);
    }
    v
}

/// Étiquette d'un intervenant (index 0-based → « Intervenante N »).
fn speaker(c: u32) -> String {
    format!("Intervenante {}", c + 1)
}

/// Clustering des embeddings (**k-moyennes cosinus**) :
/// - `Auto` : le nombre d'intervenants est **détecté** — le creux de la
///   courbe de fusion (le plus grand saut ≥ `MIN_GAP`, seulement si la
///   courbe *effondre* : `COLLAPSE_RATIO` — cf. bench F12) donne le
///   nombre de clusters (sinon → voix unique) ;
/// - `Count { n }` : **exactement** N clusters — le choix explicite de
///   l'utilisateur fait foi (il connaît son fichier).
/// Renvoie, par embedding, l'index de cluster (0..k-1, ordre
/// d'apparition).
pub fn cluster(embs: &[Vec<f32>], mode: &ClusterMode) -> Vec<u32> {
    match mode {
        ClusterMode::Auto => kmeans(embs, auto_k(embs)),
        // N forcé : exactement N (`kmeans` borne k au nb d'embeddings).
        ClusterMode::Count { n } => kmeans(embs, (*n).max(1) as usize),
    }
}

/// Partition en `k` clusters par **k-moyennes cosinus** :
/// - seeds : échantillonnage des points les plus éloignés (embedding 0,
///   puis l'embedding au cosinus minimal le plus bas face aux seeds —
///   bien répartis, un par voix) ;
/// - itérations : affectation au centroïde au cosinus maximal +
///   rééchantillonnage (moyenne des membres), jusqu'à stabilité.
/// Chaque embedding va au centroïde le plus proche. Renvoie
/// l'attribution renumérotée par **ordre d'apparition**.
fn kmeans(embs: &[Vec<f32>], k: usize) -> Vec<u32> {
    let n = embs.len();
    let k = k.clamp(1, n);
    if n <= 1 || k <= 1 {
        return vec![0u32; n];
    }
    // Seeds (échantillonnage des plus lointains) : embedding 0, puis la
    // candidate au cosinus minimal face aux seeds le plus bas.
    let mut seeds: Vec<usize> = vec![0];
    while seeds.len() < k {
        let mut best = 0usize;
        let mut best_d = f32::MIN;
        for i in 0..n {
            if seeds.contains(&i) {
                continue;
            }
            let d = seeds
                .iter()
                .map(|&s| 1.0f32 - cosine(&embs[i], &embs[s]))
                .fold(f32::MAX, f32::min);
            if d > best_d {
                best_d = d;
                best = i;
            }
        }
        if best_d == f32::MIN {
            break;
        }
        seeds.push(best);
    }
    let mut cent: Vec<Vec<f32>> = seeds.iter().map(|&s| embs[s].clone()).collect();
    let mut assign = vec![0u32; n];
    for _ in 0..64 {
        let mut changed = false;
        for i in 0..n {
            let mut bc = 0usize;
            let mut bs = f32::MIN;
            for (c, e) in cent.iter().enumerate() {
                let s = cosine(&embs[i], e);
                if s > bs {
                    bs = s;
                    bc = c;
                }
            }
            if assign[i] != bc as u32 {
                assign[i] = bc as u32;
                changed = true;
            }
        }
        // Centroïdes = moyenne des membres (le cosinus se normalise seul).
        for (c, e) in cent.iter_mut().enumerate() {
            let mut acc = vec![0.0f32; e.len()];
            let mut cnt = 0usize;
            for i in 0..n {
                if assign[i] as usize == c {
                    for (d, v) in acc.iter_mut().zip(&embs[i]) {
                        *d += *v;
                    }
                    cnt += 1;
                }
            }
            if cnt > 0 {
                for v in acc.iter_mut() {
                    *v /= cnt as f32;
                }
                *e = acc;
            }
        }
        if !changed {
            break;
        }
    }
    renumber(&assign)
}

/// Renumérotation par **ordre d'apparition** (embedding croissant) : le 1ᵉʳ
/// cluster rencontré → 0, le suivant → 1, etc.
fn renumber(ids: &[u32]) -> Vec<u32> {
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut next: u32 = 0;
    let mut out = Vec::with_capacity(ids.len());
    for &c in ids {
        if !remap.contains_key(&c) {
            remap.insert(c, next);
            next += 1;
        }
        out.push(remap[&c]);
    }
    out
}

/// Nombre d'intervenants **auto** : le **creux** de la courbe de fusion,
/// avec sa **condition d'effondrement**.
///
/// La courbe descend des fusions intra-intervenants (haut) vers les
/// fusions inter-intervenants (bas) : le plus grand saut entre
/// similarités consécutives est la coupe intra/inter — on arrête juste
/// avant.
///
/// **Effondrement (bench F12)** : le saut ≥ `MIN_GAP` *seul* surestimait
/// les fichiers **1 voix** — la dérive d'une voix unique produit des
/// sauts intra (0,12–0,20) qui chevauchent les sauts inter-voix
/// (0,17–0,33) : aucun seuil de saut ne discrimine. La coupe n'est donc
/// acceptée que si la courbe *effondre* : **dernière fusion ≤
/// `COLLAPSE_RATIO` × première** (multi-voix ≤ 0,50, 1-voix ≥ 0,57 —
/// 9/9 régimes du bench classés, cf. docs/models.md §7). Pas
/// d'effondrement ou pas de saut ≥ `MIN_GAP` → voix unique.
/// Égalité : le 1ᵉʳ (plus haut) saut l'emporte.
fn auto_k(embs: &[Vec<f32>]) -> usize {
    let n = embs.len();
    if n < 2 {
        return 1;
    }
    let curve = merge_curve(embs);
    // Effondrement : rapport dernière/première fusion (la courbe a
    // ≥ 1 pas ici — n ≥ 2).
    if *curve.last().unwrap() > COLLAPSE_RATIO * *curve.first().unwrap() {
        return 1;
    }
    let mut best: Option<(usize, f32)> = None; // (index du saut, valeur)
    for (i, w) in curve.windows(2).enumerate() {
        let g = w[0] - w[1];
        if best.map_or(true, |(_, g0)| g > g0) {
            best = Some((i, g));
        }
    }
    match best {
        Some((i, g)) if g >= MIN_GAP => (n - i - 1).max(1),
        _ => 1,
    }
}

/// Courbe complète de fusion (agrégatif sans seuil, jusqu'à un seul
/// cluster) — diagnostic `TLDL_DIAR_DEBUG` / calibrage F12.
pub fn merge_curve(embs: &[Vec<f32>]) -> Vec<f32> {
    agglomerate(embs).1
}

/// Agrégatif **liaison moyenne** sur la matrice de cosinus complète,
/// sans seuil, jusqu'à un seul cluster.
///
/// À chaque pas, la paire de clusters à la **liaison moyenne** la plus
/// élevée est fusionnée. Lance–Williams : la similarité entre le cluster
/// fusionné et chaque tiers se met à jour en O(1) (moyenne pondérée par
/// les tailles). Renvoie l'attribution (par embedding, ordre d'apparition)
/// et la courbe des similarités de pas (de la 1ʳᵉ fusion à la dernière).
fn agglomerate(embs: &[Vec<f32>]) -> (Vec<u32>, Vec<f32>) {
    let n = embs.len();
    let mut curve = Vec::new();
    if n <= 1 {
        return (vec![0u32; n], curve);
    }
    // Matrice des cosinus (symétrique, n×n).
    let mut sim = vec![vec![0f32; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let s = cosine(&embs[i], &embs[j]);
            sim[i][j] = s;
            sim[j][i] = s;
        }
    }
    let mut size = vec![1usize; n];
    let mut parent: Vec<Option<usize>> = (0..n).map(|_| None).collect();
    let mut alive = vec![true; n];
    loop {
        let count = alive.iter().filter(|&&a| a).count();
        if count <= 1 {
            break;
        }
        let mut best: Option<(usize, usize)> = None;
        let mut best_s = f32::MIN;
        for a in 0..n {
            if !alive[a] {
                continue;
            }
            for b in a + 1..n {
                if !alive[b] {
                    continue;
                }
                if sim[a][b] > best_s {
                    best_s = sim[a][b];
                    best = Some((a, b));
                }
            }
        }
        let (a, b) = match best {
            Some(p) => p,
            None => break,
        };
        // Fusion b → a (Lance–Williams, liaison moyenne).
        for c in 0..n {
            if !alive[c] || c == b {
                continue;
            }
            let w = (size[a] + size[b]) as f32;
            let s = (size[a] as f32 * sim[a][c] + size[b] as f32 * sim[b][c]) / w;
            sim[a][c] = s;
            sim[c][a] = s;
        }
        size[a] += size[b];
        alive[b] = false;
        parent[b] = Some(a);
        curve.push(best_s);
    }
    // Ancêtre (suivi du parent jusqu'au représentant alive).
    let root = |i: usize| {
        let mut r = i;
        while let Some(p) = parent[r] {
            r = p;
        }
        r
    };
    // Renumérotation dans l'ordre d'apparition (index croissant).
    let mut remap: std::collections::HashMap<usize, u32> =
        std::collections::HashMap::new();
    let mut next: u32 = 0;
    for i in 0..n {
        let r = root(i);
        if !remap.contains_key(&r) {
            remap.insert(r, next);
            next += 1;
        }
    }
    let assign: Vec<u32> = (0..n).map(|i| remap[&root(i)]).collect();
    (assign, curve)
}

/// Cosinus (borné à [-1, 1] par construction).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Centroïde d'un cluster : moyenne des embeddings membres.
/// (Dimension = celle du 1ᵉʳ membre — 192 en prod, tests plus courts.)
fn centroid(embs: &[Vec<f32>], members: &[usize]) -> Vec<f32> {
    let dim = embs[members[0]].len();
    let mut acc = vec![0.0f32; dim];
    for &i in members {
        let e = &embs[i];
        for k in 0..dim {
            acc[k] += e[k];
        }
    }
    let n = members.len().max(1) as f32;
    for k in 0..dim {
        acc[k] /= n;
    }
    acc
}

/// Rapport de calibrage (F12), activé par `TLDL_DIAR_DEBUG=1` : histogramme
/// global des cosinus paires, courbe de fusion + creux (top 5 sauts),
/// attribution et statistiques intra/inter du clustering effectif. Coût
/// négligeable (pas d'appel ONNX : seules les embeddings déjà produites
/// sont relues).
fn debug_report(embs: &[Vec<f32>], ids: &[u32], mode: &ClusterMode) {
    let n = embs.len();
    println!("[diar:debug] embeddings (segments) : {n}");

    // Histogramme des cosinus de toutes les paires (21 classes, -1..=1).
    let mut hist = [0u32; 21];
    let mut total = 0u32;
    for i in 0..n {
        for j in (i + 1)..n {
            let s = cosine(&embs[i], &embs[j]);
            let b = ((s + 1.0) * 10.0).round() as i32;
            hist[b.clamp(0, 20) as usize] += 1;
            total += 1;
        }
    }
    println!("[diar:debug] histogramme des cosinus (paires = {total}) :");
    for b in 0..21 {
        let lo = b as f32 / 10.0 - 1.0;
        let hi = lo + 0.1;
        let c = hist[b];
        let bar = "#".repeat((c.saturating_sub(1) as usize).min(40));
        println!(
            "  [{lo:+.1} ; {hi:.1}] {c:>6} {}",
            if c > 0 { bar } else { String::new() }
        );
    }

    // Courbe de fusion (agrégatif, liaison moyenne) : la similarité de
    // chaque pas, de la première fusion à la dernière.
    let curve = merge_curve(embs);
    let csv: Vec<String> = curve.iter().map(|s| format!("{s:.2}")).collect();
    println!("[diar:debug] courbe de fusion ({} pas) : {}", curve.len(), csv.join(" "));

    // Creux : les 5 plus grands sauts (→ nombre de clusters implicite) +
    // la détection auto effective.
    let mut gaps: Vec<(usize, f32)> = curve
        .windows(2)
        .enumerate()
        .map(|(i, w)| (i, w[0] - w[1]))
        .collect();
    gaps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let n = embs.len();
    let top: Vec<String> = gaps
        .iter()
        .take(5)
        .map(|(i, g)| format!("g={g:.2}→k={}", n - i - 1))
        .collect();
    println!("[diar:debug] creux (top 5 sauts) : {}", top.join(" "));
    println!("[diar:debug] détection auto : {} intervenant(s)", auto_k(embs));

    // Statistiques intra / inter du clustering effectif.
    let k = ids.iter().max().map(|&m| (m + 1) as u32).unwrap_or(0);
    println!("[diar:debug] clustering effectif : {k} clusters (mode {mode:?})");
    let mut sizes = Vec::with_capacity(k as usize);
    for c in 0..k {
        let s = ids.iter().filter(|&&a| a == c).count();
        sizes.push(s);
    }
    println!("[diar:debug] tailles par cluster : {sizes:?}");
    // Attribution par segment (ordre temporel) + segments de chaque cluster.
    let chars: Vec<char> = ids
        .iter()
        .map(|&c| (b'a' + (c % 26) as u8) as char)
        .collect();
    println!("[diar:debug] attribution (segments) : {}", chars.iter().collect::<String>());
    let mut by: Vec<Vec<usize>> = vec![vec![]; k as usize];
    for (i, &c) in ids.iter().enumerate() {
        by[c as usize].push(i);
    }
    for (ci, m) in by.iter().enumerate() {
        println!(
            "[diar:debug] cluster {ci} ({} segments) : {}",
            m.len(),
            m.iter().map(|&i| i.to_string()).collect::<Vec<_>>().join(" ")
        );
    }
    if k >= 2 {
        let mems: Vec<Vec<usize>> = (0..k)
            .map(|c| (0..n).filter(|&i| ids[i] == c).collect::<Vec<_>>())
            .collect();
        let mut intra: Vec<f32> = Vec::new();
        for m in &mems {
            for x in 0..m.len() {
                for y in (x + 1)..m.len() {
                    intra.push(cosine(&embs[m[x]], &embs[m[y]]));
                }
            }
        }
        intra.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q = |v: &Vec<f32>, p: f32| {
            let i = ((p * (v.len() - 1) as f32)).round() as usize;
            v[i.min(v.len() - 1)]
        };
        // Entre centroïdes : la vraie mesure de séparation.
        let cents: Vec<Vec<f32>> = mems.iter().map(|m| centroid(embs, m)).collect();
        let mut inter: Vec<f32> = Vec::new();
        for c in 0..k {
            for d in (c + 1)..k {
                inter.push(cosine(&cents[c as usize], &cents[d as usize]));
            }
        }
        inter.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "[diar:debug] intra : min {:.3} méd {:.3} max {:.3} | inter : min {:.3} méd {:.3} max {:.3}",
            intra[0],
            q(&intra, 0.5),
            intra[intra.len() - 1],
            inter[0],
            q(&inter, 0.5),
            inter[inter.len() - 1]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cosinus de cas triviaux (aligné, orthogonal, nul).
    #[test]
    fn cosine_trivial() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    /// Deux « voix » orthogonales : `Auto` les sépare, la paire se regroupe.
    #[test]
    fn cluster_auto_splits_distinct() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let embs = vec![a.clone(), b.clone(), a.clone(), b.clone()];
        let ids = cluster(&embs, &ClusterMode::Auto);
        assert_ne!(ids[0], ids[1], "voix distinctes non séparées");
        assert_eq!(ids[0], ids[2], "même voix, même cluster");
        assert_eq!(ids[1], ids[3], "même voix, même cluster");
    }

    /// Détection auto (`auto_k`) : deux voix nettes (creux de courbe
    /// marqué) → exactement 2 clusters, par ordre d'apparition.
    #[test]
    fn auto_detects_two_voices() {
        // Trois embeddings par voix, quasi-orthogonales : cosinés intra
        // ≈ 0,997–0,999, inter ≈ 0,05 — la coupe est nette.
        let embs = vec![
            vec![1.0f32, 0.0, 0.0],
            vec![1.0f32, 0.05, 0.0],
            vec![1.0f32, 0.0, 0.05],
            vec![0.0f32, 1.0, 0.0],
            vec![0.05f32, 1.0, 0.0],
            vec![0.0f32, 1.0, 0.05],
        ];
        let ids = cluster(&embs, &ClusterMode::Auto);
        let k = ids.iter().max().map(|&m| (m + 1) as u32).unwrap_or(0);
        assert_eq!(k, 2, "auto doit détecter 2 voix");
        assert_eq!(ids[0], ids[1], "voix A regroupée");
        assert_eq!(ids[1], ids[2], "voix A regroupée");
        assert_eq!(ids[3], ids[4], "voix B regroupée");
        assert_eq!(ids[4], ids[5], "voix B regroupée");
        assert_ne!(ids[0], ids[3], "voix distinctes séparées");
    }

    /// Voix unique : courbe lisse (aucun saut ≥ `MIN_GAP`) → 1 cluster.
    #[test]
    fn auto_single_speaker() {
        let a = vec![1.0f32, 0.0, 0.0];
        let embs = vec![a.clone(), a.clone(), a.clone()];
        let ids = cluster(&embs, &ClusterMode::Auto);
        assert!(
            ids.iter().all(|&x| x == ids[0]),
            "courbe lisse : voix unique"
        );
    }

    /// `Count { n: 1 }` : tout est fusionné en un seul cluster.
    #[test]
    fn cluster_count_forces_one() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let embs = vec![a.clone(), b.clone(), a.clone(), b.clone()];
        let ids = cluster(&embs, &ClusterMode::Count { n: 1 });
        assert!(ids.iter().all(|&x| x == ids[0]), "forçage 1 : un seul cluster");
    }

    /// `Count { n }` : **exactement** n clusters — le choix explicite de
    /// l'utilisateur fait foi (il connaît son fichier).
    #[test]
    fn cluster_count_exactly_n() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let embs = vec![a.clone(), b.clone(), a.clone(), b.clone()];
        let ids = cluster(&embs, &ClusterMode::Count { n: 2 });
        let k = ids.iter().max().map(|&m| (m + 1) as u32).unwrap_or(0);
        assert_eq!(k, 2, "N forcé : exactement N clusters");
        assert_eq!(ids[0], ids[2], "voix A regroupée");
        assert_eq!(ids[1], ids[3], "voix B regroupée");
        assert_ne!(ids[0], ids[1], "voix distinctes séparées");
    }
    /// Bench F12 — 1 **voix réelle** (JFK, 4 chunks disjoints de 2,4 s) :
    /// le saut seul surestimait (`auto_k` = 3 — courbe 0,775→0,471, saut
    /// 0,20) ; l'effondrement (0,471/0,775 = 0,608 > `COLLAPSE_RATIO`)
    /// refuse la coupe → **1 voix**. `Count { n: 2 }` fait foi :
    /// exactement 2 clusters.
    #[test]
    fn auto_single_real_speaker_no_oversplit() {
        // Embeddings ECAPA réels (golden du bench F12 — dump vérifié,
        // contrôle cos Python↔Rust = 1).
        let e0: Vec<f32> = vec![5.457487702e-02f32, 4.877920747e-01f32, 2.642247677e-01f32, -1.935653836e-01f32, 4.103277624e-01f32, -5.569075942e-01f32, 1.778003573e-02f32, -8.038040996e-02f32, -2.387327552e-01f32, 5.482563376e-01f32, -4.406931996e-01f32, -2.203699946e-02f32, 1.541628242e-01f32, -8.926775306e-02f32, -6.979013681e-01f32, 2.373817563e-01f32, -2.220471203e-01f32, 1.534647942e-01f32, 3.069072664e-01f32, 2.514746785e-02f32, -1.216515601e-01f32, 4.854549170e-01f32, -1.671553850e-01f32, 7.248952985e-01f32, -3.232385516e-01f32, -1.955255270e-01f32, 4.793965816e-01f32, -3.230686188e-01f32, -4.521090537e-03f32, 1.001728624e-01f32, 2.351537943e-01f32, 1.596091688e-01f32, -4.586305320e-01f32, 4.365780950e-02f32, -4.867818579e-02f32, 1.117102057e-01f32, -3.371816278e-01f32, 2.857602537e-01f32, -1.330676079e-01f32, -2.932720482e-01f32, 1.321935803e-01f32, -2.148631960e-01f32, -1.207258552e-02f32, -5.602605641e-02f32, -2.525022328e-01f32, -1.959880292e-01f32, 3.243241012e-01f32, -7.840734906e-03f32, -4.504400492e-02f32, -2.617162652e-02f32, -1.848746836e-02f32, 5.993182063e-01f32, 6.843709946e-02f32, 3.976908028e-01f32, 6.760887057e-02f32, 1.943180561e-01f32, -5.778312683e-01f32, 2.248362154e-01f32, -5.112022758e-01f32, 3.742480278e-01f32, -1.228759885e+00f32, 1.102762520e-01f32, 2.365776747e-01f32, -1.672707498e-02f32, -2.468377650e-01f32, 8.557060361e-02f32, 3.757284880e-01f32, -5.306817293e-01f32, -3.121102154e-01f32, 4.792168736e-01f32, -7.018702030e-01f32, -1.880560517e-01f32, -4.762751758e-01f32, -5.270491838e-01f32, 2.361693829e-01f32, -2.453612536e-01f32, 3.370262384e-01f32, 2.603980005e-01f32, 2.456128597e-03f32, -5.677011609e-02f32, -2.114518359e-02f32, 3.353660703e-01f32, 2.025909573e-01f32, 1.086973175e-01f32, 6.600064784e-02f32, 1.091343313e-01f32, 1.692713946e-01f32, -2.856256366e-01f32, 4.634626955e-02f32, -2.766087055e-01f32, -2.974293232e-01f32, 1.813959479e-01f32, -2.464674860e-01f32, 8.463019133e-02f32, 2.389383763e-01f32, 4.364731312e-01f32, -4.110485315e-02f32, 3.219180703e-01f32, 3.074357212e-01f32, 3.195748329e-01f32, 9.153925180e-01f32, -4.355616570e-01f32, 4.623509645e-01f32, 7.367593050e-02f32, 4.348108172e-01f32, 9.581433982e-02f32, -1.759456694e-01f32, -5.900774002e-01f32, -4.950720966e-01f32, 6.534399837e-02f32, 8.227796108e-02f32, -3.928069174e-01f32, -3.553696349e-02f32, 1.413080245e-01f32, 2.919260859e-01f32, -2.401005849e-02f32, 1.502002180e-01f32, 1.478987932e-01f32, -3.802697062e-01f32, -4.654176831e-01f32, 1.866051406e-01f32, 7.595315576e-02f32, -2.269393951e-01f32, -3.289660811e-01f32, 3.385816813e-01f32, 3.684868664e-02f32, 1.043819189e-01f32, 2.146797180e-01f32, -5.712094158e-02f32, 3.673479557e-01f32, -5.712112412e-02f32, 1.732221991e-01f32, 6.196255609e-02f32, 3.920091987e-01f32, -3.114933670e-01f32, -1.688286811e-01f32, 2.333992422e-01f32, 2.182988077e-01f32, -4.643957615e-01f32, -2.807536349e-02f32, -3.401270509e-01f32, -4.313756526e-02f32, -7.880777121e-04f32, 2.679190338e-01f32, -1.346066594e-03f32, -7.125439048e-01f32, 1.238986775e-01f32, -4.365525842e-01f32, -1.200059950e-01f32, -5.958403945e-01f32, 5.290699005e-02f32, -2.461937070e-01f32, 1.720341146e-01f32, -6.403350830e-01f32, -5.068416595e-01f32, -2.561643124e-01f32, 8.854087442e-02f32, -5.910358205e-02f32, -1.834378541e-01f32, -3.238259852e-01f32, -2.094182000e-02f32, 3.120740056e-01f32, 5.662237108e-02f32, -4.239292443e-02f32, -6.442137063e-03f32, 4.205999076e-01f32, 1.904942095e-01f32, -1.230053380e-01f32, -1.385693848e-01f32, -5.198252201e-01f32, 1.112944335e-01f32, 3.857703805e-01f32, 2.859458029e-01f32, -2.736856416e-02f32, -7.058758289e-02f32, 1.261317134e-01f32, -2.563188672e-01f32, 2.980603874e-01f32, -4.139436185e-01f32, 1.684696972e-02f32, -1.264112145e-01f32, 1.331886947e-01f32, -2.080730945e-01f32, -3.073067069e-01f32, -5.419062376e-01f32, 2.382941544e-01f32, 2.421419919e-01f32, -8.446682990e-02f32, 1.538355798e-01f32, -2.767209709e-01f32, 1.878169179e-01f32, 1.934697330e-01f32];
        let e1: Vec<f32> = vec![4.951402545e-02f32, 6.890252233e-02f32, 4.953548908e-01f32, -2.361764014e-01f32, 8.996474147e-01f32, -3.223206997e-01f32, -1.063622236e+00f32, -4.945406690e-03f32, -3.315361142e-01f32, 2.085062861e-01f32, -2.944306731e-01f32, -9.795100987e-02f32, 3.985145390e-01f32, 3.051997423e-01f32, -7.521250248e-01f32, -3.852547407e-01f32, -1.926949322e-01f32, 2.127245069e-01f32, 4.231645167e-02f32, 1.527578533e-01f32, 1.945400238e-01f32, 2.262512445e-01f32, 5.220651627e-01f32, -1.049607322e-01f32, 3.140655160e-02f32, -2.719470561e-01f32, 8.650258183e-02f32, -3.579927683e-01f32, 5.016325712e-01f32, 1.443522330e-02f32, -9.400671721e-02f32, 1.050844714e-01f32, -5.515109748e-02f32, 2.229516208e-01f32, 5.748631433e-02f32, 8.517707884e-02f32, -1.076619774e-01f32, 9.059299529e-02f32, 2.666692436e-02f32, -7.072302699e-01f32, -4.013552666e-01f32, -3.679038882e-01f32, -1.988856494e-01f32, -4.731228054e-01f32, -2.353497148e-01f32, -7.424481213e-02f32, 5.156841278e-01f32, -3.190314770e-01f32, 1.589417458e-03f32, 1.858328283e-01f32, -3.702461720e-02f32, -7.844095826e-01f32, 3.715890646e-01f32, -1.240879744e-01f32, 2.367052622e-02f32, 1.364340782e-01f32, -7.258604765e-01f32, 1.583749056e-02f32, -1.558422297e-01f32, 1.292408258e-01f32, -6.921404600e-01f32, -4.317422509e-01f32, 2.172903419e-01f32, 7.803560793e-02f32, -3.795289695e-01f32, -2.516558170e-01f32, 6.027672291e-01f32, -5.937068462e-01f32, -8.126986027e-01f32, 4.378774464e-01f32, -1.468918324e-01f32, 4.437043145e-02f32, -3.135157526e-01f32, 4.025018215e-01f32, 6.096411943e-01f32, 4.199860096e-01f32, -2.576715350e-01f32, -5.351861715e-01f32, 1.789496690e-01f32, -4.758096933e-01f32, -4.751421511e-01f32, 1.332175434e-01f32, -3.427323103e-01f32, -3.228153586e-01f32, 4.395894706e-03f32, 2.392799556e-01f32, -1.750683039e-02f32, -4.014875293e-01f32, -1.257074475e-01f32, 4.538545012e-02f32, -9.136074781e-02f32, 4.763005972e-01f32, -5.114789009e-01f32, -7.107616663e-01f32, 3.501024246e-01f32, -2.270909101e-01f32, -6.297317147e-02f32, 4.315476865e-02f32, -2.867511213e-01f32, -4.634923041e-01f32, 2.786581516e-01f32, -3.440377861e-02f32, 6.721684337e-02f32, -5.609004498e-01f32, 3.705103397e-01f32, 3.002575934e-01f32, 1.652787998e-02f32, -1.434073895e-01f32, -2.932579219e-01f32, -5.426537395e-01f32, -1.267689168e-01f32, -4.565419853e-01f32, 2.303545177e-01f32, 3.480330408e-01f32, -3.238373250e-02f32, -1.117431000e-01f32, 1.755702645e-01f32, 2.579858899e-01f32, 1.438655555e-01f32, 2.712278664e-01f32, -3.050695956e-01f32, -2.765803635e-01f32, 1.223730072e-01f32, -1.906220019e-01f32, 1.938438118e-01f32, 9.058494121e-03f32, 4.754351974e-01f32, 1.386730969e-01f32, -3.563938439e-01f32, -3.109063953e-02f32, -4.965554476e-01f32, -7.543495297e-02f32, -1.405524760e-01f32, 4.211401343e-01f32, -4.485661685e-01f32, -4.917743430e-02f32, 2.641018629e-01f32, 2.457949221e-01f32, 9.215168655e-03f32, -1.127840057e-01f32, -4.367246628e-01f32, -3.382371664e-01f32, -7.188545913e-02f32, 2.339695245e-01f32, -2.096326202e-01f32, -5.007766485e-01f32, -2.453159988e-01f32, -6.171672344e-01f32, 9.262830019e-03f32, -4.746297002e-01f32, 1.552672088e-01f32, -4.295622110e-01f32, 1.539006829e-01f32, -7.195425034e-02f32, 1.860387623e-03f32, -4.091564417e-01f32, 1.411269307e-01f32, 3.174765706e-01f32, -2.901482284e-01f32, 3.464474082e-01f32, 3.445941210e-01f32, 7.769379616e-01f32, -2.896163166e-01f32, -9.509725869e-02f32, -3.014784455e-01f32, -6.158803403e-02f32, -2.003852427e-01f32, -2.269941568e-01f32, -1.447439641e-01f32, -9.674297273e-02f32, -9.268023074e-03f32, -4.667195082e-01f32, 4.854240119e-01f32, -3.122671843e-01f32, -4.107048213e-01f32, 5.752629638e-01f32, -4.966954887e-02f32, 6.654031277e-01f32, -3.062918782e-02f32, -3.414629996e-01f32, -2.884694934e-01f32, -5.720949918e-02f32, 2.406062931e-01f32, -3.215743303e-01f32, -1.218878925e-01f32, 4.977655411e-02f32, 2.242453396e-01f32, -2.691839337e-01f32, 9.783655405e-02f32, -2.566053271e-01f32, -2.801658809e-01f32, -2.897842228e-02f32];
        let e2: Vec<f32> = vec![-2.652386427e-01f32, 2.029758394e-01f32, 4.965024590e-01f32, -2.517841458e-01f32, 5.115237832e-01f32, -3.693605661e-01f32, -3.622689247e-01f32, -6.477098912e-02f32, -1.951338500e-01f32, 2.503305972e-01f32, -2.065497190e-01f32, -1.767718196e-01f32, 1.799243391e-01f32, -2.391985953e-01f32, -8.671075106e-01f32, -1.572080106e-01f32, -1.557509601e-02f32, 2.850208580e-01f32, 5.641323924e-01f32, 3.128597140e-02f32, -1.796408147e-01f32, 4.410116673e-01f32, 4.614285231e-01f32, 1.900771260e-01f32, -4.327701330e-01f32, 3.470928967e-01f32, -1.636062264e-01f32, 1.682276875e-01f32, 6.404519826e-02f32, 5.227701664e-01f32, -1.237143949e-02f32, 1.322744489e-01f32, -2.230097950e-01f32, 6.899530441e-02f32, 2.129791677e-01f32, 6.353078038e-02f32, -3.670884669e-01f32, -2.217022479e-01f32, -4.321392179e-01f32, -4.355650246e-01f32, -2.799499631e-01f32, -4.300229549e-01f32, 3.786661476e-02f32, 1.781374812e-01f32, -3.675950170e-01f32, -5.814746022e-01f32, -3.217706084e-02f32, -4.500031173e-01f32, -3.437267244e-01f32, -2.858122252e-02f32, 6.699826568e-02f32, -4.658968449e-01f32, 1.084137261e-01f32, 4.521109462e-01f32, 5.017019510e-01f32, -1.914422959e-02f32, -4.944809675e-01f32, 1.952141672e-01f32, -2.674606740e-01f32, 1.500551701e-01f32, -9.976527095e-01f32, -3.007131219e-01f32, 3.572785556e-01f32, 3.044607937e-01f32, -1.696845591e-01f32, 1.039267182e-01f32, 1.103246063e-01f32, -4.775063992e-01f32, -4.235636890e-01f32, 5.482873917e-01f32, -4.331872463e-01f32, 5.468731746e-02f32, -1.706076413e-01f32, -1.069850177e-01f32, 4.030620158e-01f32, -1.455156058e-01f32, -2.884583175e-02f32, 1.977100670e-01f32, 3.163758516e-01f32, -1.569689661e-01f32, 3.006662726e-01f32, 4.085283875e-01f32, -2.397291660e-01f32, -6.563521624e-01f32, -1.649530530e-01f32, 2.197349072e-01f32, -2.383114249e-01f32, 7.052518427e-03f32, -6.040787324e-02f32, -1.687504351e-03f32, 2.578964829e-01f32, 3.896865845e-01f32, -2.316329181e-01f32, -5.973066688e-01f32, 1.285597682e-01f32, -5.160980821e-01f32, -3.958398849e-02f32, -3.742561638e-01f32, 2.362206727e-01f32, -4.331568480e-01f32, 5.230499506e-01f32, -4.473487735e-01f32, 2.096292377e-01f32, -1.643271744e-01f32, 2.134449929e-01f32, -1.690937951e-02f32, -1.372167617e-01f32, -1.664668918e-01f32, -5.843630433e-01f32, -2.771180272e-01f32, -1.674593538e-01f32, -5.360096693e-01f32, 4.052561522e-02f32, -2.970245481e-02f32, 2.609072626e-01f32, 8.643268049e-02f32, 2.675676346e-01f32, 5.175523758e-01f32, -1.701520979e-01f32, 3.616271168e-02f32, -1.948457360e-01f32, 1.859019250e-01f32, -4.407073855e-01f32, -2.714960575e-01f32, 3.852568939e-02f32, -3.032509685e-01f32, 2.161537558e-01f32, 1.397668123e-01f32, 1.311291456e-01f32, -1.059729829e-01f32, -1.347168684e-01f32, 1.729208976e-02f32, 3.626652062e-03f32, 3.779624701e-01f32, -1.382025033e-01f32, 1.142139733e-01f32, -9.722951055e-02f32, 1.757735759e-01f32, -6.307035685e-01f32, -1.964195371e-01f32, -2.967134118e-02f32, 2.421888858e-01f32, -1.025149226e-01f32, 3.203218281e-01f32, -1.244867668e-01f32, -4.579393566e-02f32, -1.791369319e-01f32, -3.516882658e-01f32, -1.251487434e-01f32, -2.624189109e-02f32, -4.609125108e-02f32, -1.562152654e-01f32, 5.025017858e-01f32, -3.181951940e-01f32, -1.346817613e-01f32, -6.760429740e-01f32, 2.980788648e-01f32, 1.926472932e-01f32, -5.939254761e-01f32, -1.750949621e-01f32, 2.405916601e-01f32, 3.761878610e-01f32, -4.623023421e-02f32, -1.368485391e-03f32, -1.215627193e-01f32, 4.141568542e-01f32, -1.484793723e-01f32, -4.792299122e-02f32, 1.340131313e-01f32, -4.471710920e-01f32, -1.381010264e-01f32, -1.263743192e-01f32, 4.402558804e-01f32, -5.693396926e-02f32, -4.682199657e-01f32, 4.518522918e-01f32, 2.336153686e-01f32, -7.192241400e-02f32, -4.023026526e-01f32, 3.451476097e-01f32, -5.807794631e-02f32, 4.217429757e-01f32, 4.713859260e-01f32, 9.664249420e-02f32, -2.195110470e-01f32, -2.069850266e-01f32, 5.301043019e-02f32, -1.752903014e-01f32, 1.073639095e-01f32, 1.261663288e-01f32, -3.170252740e-01f32, -3.428097069e-02f32];
        let e3: Vec<f32> = vec![2.578305081e-02f32, 3.540064692e-01f32, 3.598140478e-01f32, -3.014943004e-01f32, 3.407507539e-01f32, -2.910610437e-01f32, -5.682792068e-01f32, -9.213516116e-02f32, -2.896021605e-01f32, -8.860960603e-03f32, -2.624670565e-01f32, -7.056628168e-02f32, 1.828658432e-01f32, 4.371302724e-01f32, -9.255007505e-01f32, -2.661258876e-01f32, -3.866288066e-02f32, 5.926319957e-01f32, 3.312396109e-01f32, 2.435932755e-01f32, -3.551877737e-01f32, 5.278401971e-01f32, 8.226748705e-01f32, 2.575235367e-01f32, -3.463771343e-01f32, 2.970237732e-01f32, -1.228721142e-01f32, 2.668800354e-01f32, 3.015804887e-01f32, 5.584295392e-01f32, 2.869555354e-01f32, 2.911379039e-01f32, -3.454959765e-02f32, -1.978718936e-01f32, 4.713928998e-01f32, -2.699607313e-01f32, -5.842289925e-01f32, -2.105276585e-01f32, -2.228059620e-01f32, -4.984081388e-01f32, -5.422814488e-01f32, -3.389214277e-01f32, 2.718717754e-01f32, -4.161239862e-01f32, -2.267196029e-01f32, -1.472171545e-01f32, -1.349860877e-01f32, -7.311145961e-02f32, -1.966653764e-01f32, 4.640599191e-01f32, -2.254910022e-01f32, -4.381744266e-01f32, -1.626479775e-01f32, 7.491744161e-01f32, 1.984857619e-01f32, 2.377599478e-02f32, -5.333417654e-01f32, 5.421911180e-02f32, -4.745439887e-01f32, 3.504642844e-01f32, -1.351364493e+00f32, 2.512048231e-03f32, 5.124248266e-01f32, -2.073966265e-01f32, -2.376692891e-01f32, -1.501942575e-01f32, 2.995394766e-01f32, -1.458786428e-01f32, -2.670520544e-01f32, 6.812965274e-01f32, -5.015953779e-01f32, 9.165196121e-02f32, -4.392836392e-01f32, -5.441461504e-02f32, -7.539826632e-02f32, -2.357500792e-02f32, -2.264612466e-01f32, -5.598377809e-02f32, 3.538084626e-01f32, -2.725983560e-01f32, 1.682253927e-01f32, 1.822246015e-01f32, -2.498421669e-01f32, -3.670570254e-01f32, -2.764105797e-01f32, 3.857014775e-01f32, -2.500661314e-01f32, -5.320800543e-01f32, -8.779124916e-02f32, -1.696619540e-01f32, -2.549444884e-02f32, 6.460813284e-01f32, -2.810841799e-01f32, -6.073966622e-01f32, 2.320652008e-01f32, -3.554529548e-01f32, -4.014860466e-02f32, -4.796072245e-01f32, 4.728190303e-01f32, -2.342002690e-01f32, 7.566549778e-01f32, -4.726538956e-01f32, 2.449290454e-01f32, -7.148987800e-02f32, 1.606096327e-01f32, 2.810783312e-02f32, -6.239145994e-02f32, -2.304302007e-01f32, -3.519980907e-01f32, -4.976155758e-01f32, -3.327513486e-02f32, -8.638954163e-01f32, -7.622748613e-02f32, -1.780227274e-01f32, 3.993807137e-01f32, 5.105079710e-02f32, 1.824773699e-01f32, 3.867591023e-01f32, 1.985053718e-02f32, -1.792325526e-01f32, 5.800926685e-02f32, -1.857459545e-03f32, -3.974040747e-01f32, -4.661305249e-01f32, 2.401766777e-01f32, -3.703581095e-01f32, -5.809566006e-03f32, 2.619618177e-01f32, -1.177012846e-01f32, -4.264681786e-02f32, -4.302168638e-02f32, 2.024984956e-01f32, -2.339333296e-03f32, 5.703827739e-01f32, -6.331031919e-01f32, -2.778681219e-01f32, -8.062465489e-02f32, 2.160632759e-01f32, -2.459504902e-01f32, -1.572696120e-01f32, -5.789704248e-02f32, 7.874861360e-02f32, -3.309554458e-01f32, 4.646336734e-01f32, -1.900795698e-01f32, 2.697838843e-02f32, -1.652778089e-01f32, -5.456081033e-01f32, -1.799649745e-01f32, -7.012130022e-01f32, 9.851739556e-02f32, -3.371407688e-01f32, 6.738438010e-01f32, -7.325247526e-01f32, -5.047546029e-01f32, -6.960186362e-01f32, 6.818057299e-01f32, -1.629564166e-01f32, -4.420388639e-01f32, 3.355541080e-02f32, 2.510231137e-01f32, 7.244497538e-01f32, -1.836672872e-01f32, -1.400058717e-01f32, -4.247792065e-01f32, -1.134004518e-01f32, -1.625246257e-01f32, -1.397622526e-01f32, -2.031441331e-01f32, -8.892275393e-02f32, -2.093616575e-01f32, -2.121143937e-01f32, 4.366600215e-01f32, 1.026009619e-01f32, 2.022801340e-02f32, 2.026515156e-01f32, 3.194294274e-01f32, 1.271365136e-01f32, 1.621344313e-02f32, -2.691446841e-01f32, -2.293227613e-02f32, 5.651188493e-01f32, 4.372547269e-01f32, -6.590098143e-02f32, -2.465232611e-01f32, -1.028006002e-01f32, -2.228363156e-01f32, -1.677548587e-01f32, 3.557830453e-01f32, 3.986654282e-01f32, -6.179150939e-01f32, -1.066954583e-01f32];
        let embs = vec![e0, e1, e2, e3];
        assert_eq!(auto_k(&embs), 1, "1 voix réelle : pas de surestimation");
        let ids = cluster(&embs, &ClusterMode::Auto);
        assert!(
            ids.iter().all(|&x| x == ids[0]),
            "auto : un seul cluster"
        );
        let ids = cluster(&embs, &ClusterMode::Count { n: 2 });
        let k = ids.iter().max().map(|&m| (m + 1) as u32).unwrap_or(0);
        assert_eq!(k, 2, "N forcé : exactement 2 clusters");
    }
}
