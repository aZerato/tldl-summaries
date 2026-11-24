# Modèles IA & profils matériels

Principe (C3) : **petit mais bon, quantifié, GPU optionnel**.
Règle de base du MVP : **ASR `small` + LLM 7B–8B Q4_K_M**.
Les deux moteurs ne se chevauchent pas dans le pipeline
(ASR d'abord, puis LLM) → la RAM se dimensionne au **maximum**,
pas à la somme.

## 1. ASR — whisper.cpp (fichiers GGUF, f16)

| Modèle | Fichier | RAM (est.) | Vitesse CPU récent* | Usage |
|---|---|---|---|---|
| tiny | ~75 Mo | ~0,5 Go | ~5× temps réel | test / ultra-léger |
| base | ~150 Mo | ~1 Go | ~3–4× temps réel | low-end, FR correct |
| **small** ⭐ | ~480 Mo | ~1,5 Go | ~1,5–2× temps réel | **défaut recommandé** |
| medium | ~1,55 Go | ~3 Go | **2,5× temps réel** (mesuré — F12) | FR plus fin, CPU fort |
| large-v3-turbo | ~1,6 Go | ~3,5 Go | GPU conseillé | top qualité / rapidité |

\* Estimatif, sauf **medium** (mesuré — F12). Quantisations Q5_K_M / Q8_0 possibles
(−10 à −20 % de RAM, perte négligeable).
Langue : détection auto avec biais FR ; option « forcer FR ».

## 2. LLM résumé — llama.cpp (GGUF, Q4_K_M par défaut)

| Famille | Fichier | RAM/VRAM (est.) | Remarque |
|---|---|---|---|
| Qwen2.5 1.5B | ~1 Go | ~2 Go | ultra-léger / tests (résumés simples) |
| Qwen2.5 3B | ~2,1 Go | ~3,5 Go | profil **Léger**, bon FR |
| Gemma 3 4B | ~2,7 Go | ~4 Go | excellent rapport/qualité |
| **Qwen2.5 7B / Llama 3.1 8B** ⭐ | ~4,7 Go | ~6–7 Go | **défaut Équilibré** |
| Gemma 3 12B | ~7,5 Go | ~9–10 Go | profil **Puissant** |

- Température : **0,3** (résumé = déterministe) — réglable (F6)
- Contexte : **8 k tokens** (10 min d'audio ≈ 1,5–2 k mots → largement couvert)
- Si la transcription dépasse le contexte : résumé par morceaux puis
  fusion (mécanique à préciser)
- Modèles gérés **directement par l'app** (catalogue GGUF, F6/F11) :
  pas de démon externe (ADR-003)

## 3. Profils (préréglages de la page « Configuration » — F6)

| Profil | Machine cible | ASR | LLM | Device | RAM min |
|---|---|---|---|---|---|
| **Léger** | 8 Go, CPU seul | base | Qwen2.5 3B Q4 | CPU | 8 Go |
| **Équilibré** ⭐ | 16 Go, GPU 6–8 Go ou CPU récent | small | 7B–8B Q4_K_M | auto (GPU si dispo) | 16 Go |
| **Puissant** | GPU 12–16 Go, 32 Go | medium / large-v3-turbo | 12B Q4_K_M | GPU | 32 Go |

## 4. Règles de gestion

- Téléchargement **opt-in** (F11) ; hors ligne = dépôt manuel dans `models/`
- **Suppression** d'un modèle local (F11) : bouton « Supprimer » sur un
  modèle présent — le fichier est retiré de `models/`, le modèle redevient
  « à télécharger »
- Estimation affichée **avant** chaque téléchargement (F6)
- Un seul LLM chargé à la fois (chargement/déchargement géré par l'app — F6)
- Benchmarks (F12) : les valeurs mesurées remplacent les estimations ici

## 5. Sources de téléchargement (F11)

Hugging Face, une URL par modèle (catalogue : `src-tauri/src/models.rs`) :

| Modèle (id) | Repo Hugging Face | Fichier |
|---|---|---|
| tiny / base / small / medium / large-v3-turbo (ASR) | `ggerganov/whisper.cpp` | `ggml-{id}.bin` (f16) |
| qwen25-1.5b-q4 | `bartowski/Qwen2.5-1.5B-Instruct-GGUF` | `Qwen2.5-1.5B-Instruct-Q4_K_M.gguf` |
| qwen25-3b-q4 | `bartowski/Qwen2.5-3B-Instruct-GGUF` | `Qwen2.5-3B-Instruct-Q4_K_M.gguf` |
| gemma3-4b-q4 | `ggml-org/gemma-3-4b-it-GGUF` | `gemma-3-4b-it-Q4_K_M.gguf` |
| qwen25-7b-q4 ⭐ | `bartowski/Qwen2.5-7B-Instruct-GGUF` | `Qwen2.5-7B-Instruct-Q4_K_M.gguf` |
| llama31-8b-q4 ⭐ | `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF` | `Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf` |
| gemma3-12b-q4 | `ggml-org/gemma-3-12b-it-GGUF` | `gemma-3-12b-it-Q4_K_M.gguf` |

Règles du catalogue :

- **Noms de fichiers en MAJUSCULES** pour la quant (`-Q4_K_M.gguf`) — le
  dépôt officiel `Qwen/Qwen2.5-7B-Instruct-GGUF` est **shardé**
  (`…-00001-of-00002.gguf`) → on ne le référence pas ; `bartowski` /
  `ggml-org` fournissent le fichier unique.
- Les repos `ggml-org/Qwen2.5-*-GGUF` et
  `ggml-org/Meta-Llama-3.1-8B-Instruct-GGUF` **n'existent pas** (HF
  renvoie 401 pour un repo absent) → Qwen/Llama servis par
  `bartowski/*` ; seules les Gemma sont chez `ggml-org`.
- Le repo est résolu insensible à la casse (redirect HF) ; le **fichier** est
  sensible à la casse — d'où ce tableau comme source de vérité. Chaque URL est
  vérifiée via l'API HF (`/api/models/{repo}/tree/main`) avant mise en
  catalogue.
- Écriture : `.part` + **renommage atomique** dans `models/` — jamais un
  modèle à moitié ; annulable à tout moment, taille connue avant (déjà
  documenté §4).
- Moteur validé E2E (`tldl-e2e download <id> <url>` — HTTP local + HTTPS réel)
  et **téléchargement HF réel confirmé dans l'app** (Gemma 3 4B, 2 489 757 856
  o, taille conforme au repo).

## 6. Mesures — bench F12 (21/11/2026)

Audio : JFK (EN) bouclé 5 / 10 / 30 min (~120 mots/min). Modèles :
**whisper-medium** (ASR) + **qwen25-7b-q4** (LLM) — les défauts
« Équilibré ». Deux devices mesurés : **CPU** (Intel Core i5-14600KF,
20 threads) et **GPU** (NVIDIA GeForce RTX 4070 Ti SUPER, 16 Go, compute
8.9 — build CUDA `TLDL_GPU=cuda`). Run propre (Qwen arrêté — pas de
contention), relancé par script : CPU puis GPU, un rapport par device.

**Vue par device (ASR + LLM, total) :**

| Durée | CPU — ASR | CPU — total | GPU — ASR | GPU — total | gain total |
|---|---|---|---|---|---|
| 5 min | 122,1 s | 135,8 s | 5,4 s | 9,1 s | 14,9× |
| 10 min | 232,7 s | 246,5 s | 9,2 s | 12,7 s | 19,4× |
| 30 min | 695,0 s | 719,2 s | 25,3 s | 29,8 s | 24,1× |

**CPU** (i5-14600KF) :

| Durée | ASR medium | RTF | LLM 7B Q4 | charg.+prompt | gén. | t/s |
|---|---|---|---|---|---|---|
| 5 min | 122,1 s | 0,41 | 13,6 s | 10,4 s | 3,3 s | 30,0 |
| 10 min | 232,7 s | 0,39 | 13,8 s | 10,5 s | 3,3 s | 28,4 |
| 30 min | 695,0 s | 0,39 | 24,2 s | 18,5 s | 5,7 s | 23,4 |

**GPU** (RTX 4070 Ti SUPER) :

| Durée | ASR medium | RTF | LLM 7B Q4 | charg.+prompt | gén. | t/s |
|---|---|---|---|---|---|---|
| 5 min | 5,4 s | 0,02 | 3,7 s | 1,7 s | 2,1 s | 53,1 |
| 10 min | 9,2 s | 0,02 | 3,5 s | 1,4 s | 2,1 s | 40,7 |
| 30 min | 25,3 s | 0,01 | 4,5 s | 2,0 s | 2,5 s | 52,7 |

- **ASR medium** : CPU ≈ **2,5× temps réel** (RTF 0,39) — corrige
  l'estimation « ~1× » du §1 ; **GPU ≈ 55–70× temps réel** (RTF
  0,01–0,02).
- **LLM 7B Q4** : le « chargement+prompt » suit la taille de la
  transcription (10,4 → 10,5 → 18,5 s sur CPU ; ~1,5–2 s sur GPU —
  chargement VRAM quasi nul) ; la génération tient **~23–30 t/s sur
  CPU** (léger repli quand le prompt grossit — KV cache) et
  **~41–53 t/s sur GPU** (flash attention).
- **Critère F4 (< 60 s / 10 min)** : **atteint sur GPU** — 10 min
  traitée en **12,7 s** (ASR 9,2 s + LLM 3,5 s, 4,7× de marge) ;
  **non atteint sur CPU** (246,5 s, ≈ 2,4× temps réel). La cible est
  une cible GPU — la 4070 Ti la tient.
- **Non mesuré** : tiny / base / small / large-v3-turbo (ASR) et les
  autres LLM — modèles absents (pas de téléchargement surprise — F11
  opt-in).

## 7. Intervenants — diarisation (F9) ✅ modèle arrêté

Format **ONNX** (pas de GGUF) : exécuté par ONNX Runtime **chargé au
runtime** (feature `load-dynamic` de `ort` — dlopen du prébuild embarqué
`assets/onnxruntime/<cible>/`, pattern FFmpeg de `media.rs`), ADR-008.
**F11 opt-in** — téléchargement ou dépôt manuel dans `models/`
(même règle `.part` + renommage atomique que §5). Le modèle est le seul
« kind » `diar` du catalogue (`diar-<id>.onnx`).

| Modèle | Remarque |
|---|---|
| **Wespeaker ECAPA-TDNN 512** ✅ | export ONNX `voxceleb_ECAPA512.onnx` (embedding 192-d) — le format lu par le pipeline |
| CAM++ (Wespeaker) | `voxceleb_CAM++.onnx` — **écarté au bench F12** (ci-dessous) |
| ~~3D-Speaker / pyannote~~ | exclues (Axera NPU w8a16 sans ORT CPU / runtime Python — ADR-008) |

- **Choix arrêté (bench F12 fait) — ECAPA-TDNN 512, CAM++ écarté** :
  le bench a comparé les deux par segment de transcription (pas de
  fenêtre arbitraire) sur boulangerie (2 voix FR), JFK (1 voix EN) et
  synthétiques (2/3 voix). Contrôle : cosinus Python↔Rust = 1,00000
  (mêmes embeddings dans les deux langages). Résultats :

  | Régime | ECAPA | CAM++ |
  |---|---|---|
  | boulangerie (2 voix FR) | auto_k=2, 1/13, intra méd. 0,56/0,51, inter méd. 0,550 | auto_k=2, 1/13, intra 0,63/0,55, inter méd. 0,567 |
  | JFK 1-voix (fenêtres 1,6 s) | cos méd. 0,645 | cos méd. **0,198** |
  | synthétique 2 voix | intra 0,82/0,99, inter **0,449** (séparées) | intra **0,19/0,11**, inter 0,619 (dégénéré) |

  - **CAM++ écarté** : ses embeddings de **voix réelle** sont presque
    orthogonaux entre eux (JFK cos 0,198 ; synth. intra 0,19/0,11) —
    le modèle ne produit pas de similarité stable pour la même
    voix : aucun clustering ne peut en tirer de structure. Sur
    boulangerie il est *pire* qu'ECAPA (inter 0,567 vs 0,550 — le
    nuage y est même plus confus).
  - **ECAPA conservé** — mais **limite du modèle, pas du clustering** :
    les 14 embeddings boulangerie forment un **nuage inter-correlé**
    (intra 0,56/0,51 ≈ inter 0,550) : les deux voix du dialogue ne
    sont pas séparées par ce modèle (aucune partition ne le peut) —
    le 1ʳ segment (intro) est seul identifié comme à part. C'est le
    meilleur comportement disponible dans le périmètre ADR-008
    (ONNX CPU, pas de pyannote) ; les deux modèles Wespeaker échouent
    sur le même échantillon → c'est la limite **modèle/données**
    (ECAPA/CAM++ entraînés sur VoxCeleb) face à une conversation FR
    continue, pas un bug.
- **Détection du nombre d'intervenants (F12 — règle arrêtée)** :
  `diar::auto_k` — le **creux** de la courbe de fusion (plus grand
  saut ≥ `MIN_GAP` = **0,05**) **et** la **condition d'effondrement** :
  la coupe n'est acceptée que si la **dernière** fusion est ≤
  `COLLAPSE_RATIO` = **0,55** × la **première** (la courbe *effondre*
  quand il y a plusieurs voix — ratio ≤ 0,50 : boulangerie 0,254,
  synth. 2/3 voix 0,50 — et dérive doucement en 1 voix — ratio ≥ 0,57 :
  JFK 1,6 s×6 0,574, 2,4 s×4 0,610, 3,2 s×3 0,829, 4,8 s×2 1,0).
  **8/8 régimes du bench classés** (marge au seuil 0,024–0,296).
  Sans effondrement ou sans saut ≥ `MIN_GAP` → voix unique. Le saut
  **seul** surestimait les fichiers 1-voix (JFK 2,4 s×4 → 3, 1,6 s×6 →
  5, 3,2 s×3 → 2) — la dérive d'une voix produit des sauts intra qui
  chevauchent les sauts inter-voix : aucun seuil de saut ne
  discrimine, seul le rapport final/initial le fait.
  **`Count { n }` = exactement N** (`kmeans(n)`) — le choix explicite
  de l'utilisateur fait foi (il connaît son fichier), plus de
  `min(auto_k, N)`.
- **Taille / RAM** : 24 Mo (f32), RAM négligeable devant ASR/LLM.
- **Coût CPU (mesuré, F12)** : **7 ms/segment** ORT seul (boulangerie,
  14 segments de 2–10 s, i5-14600KF) — fbank en sus, ≪ temps réel
  (cible C3 tenue) : **un embedding par segment** (les frames fbank
  de l'utterance entière — entrée ONNX `feats` `[B,T,80]`, `T` libre :
  le TDNN est conv + pool global, validé sur T = 150…800 frames).
- **Source** : `Wespeaker/wespeaker-voxceleb-ecapa-tdnn512` (HF) —
  `voxceleb_ECAPA512.onnx` (24 861 931 o), URL vérifiée via l'API HF
  (`/api/models/{repo}/tree/main`) avant mise en catalogue (règle §5).
  Seuls les fichiers `.onnx` sont au catalogue (pas le `.pt` PyTorch du
  repo — ADR-008).
