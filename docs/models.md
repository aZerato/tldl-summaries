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
