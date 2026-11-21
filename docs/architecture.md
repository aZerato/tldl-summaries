# Architecture

## 1. Objectif

Transformer un enregistrement (rencontre, entretien, réunion, podcast…) en :

1. une **transcription** fidèle (parole → texte, segments + horodatages),
2. un **résumé structuré** (synthèse, points clés, décisions, actions à venir),

les deux **accessibles dans l'interface** (contrainte C6).
Tout s'exécute **localement** : pas d'Internet, pas de cloud.

## 2. Contraintes

| # | Contrainte |
|---|---|
| C1 | **100 % local** à l'usage : aucune donnée ne quitte la machine (zéro télémétrie, zéro cloud) |
| C2 | **Crossplateforme** : Windows, macOS, Linux |
| C3 | **Sobre mais performant** : modèles quantifiés, GPU optionnel, CPU viable |
| C4 | **Hors ligne** : aucun réseau requis (hors téléchargement initial des modèles, opt-in) |
| C5 | **Vidéo gérée nativement** : conversion vidéo → audio **dans** l'application (FFmpeg) |
| C6 | **Interface** : import de fichier, double-vue (transcription / résumé), menu de configuration des modèles |

## 3. Stack

_Recommandations — les choix marqués ⏳ sont à valider, cf. §8._

| Brique | Choix | Rôle |
|---|---|---|
| Shell / UI | **Tauri 2.x** (Rust) + **React / TypeScript** (Vite) | App native légère (surcharge ~10 Mo) |
| Core | **Rust** | Pipeline média / ASR / LLM, config, stockage |
| ASR | **whisper.cpp** (modèles GGUF) | Transcription multilingue, horodatages |
| LLM | **llama.cpp** (binaire dédié `tldl-llm`, cf. §5.2) | Résumé — modèles 3B → 8B quantifiés Q4_K_M |
| Média | **FFmpeg** (build statique embarqué) | Sonde + conversion → WAV 16 kHz mono |
| Stockage | **SQLite** (rusqlite) | Historique local des traitements |
| Packaging | Tauri : NSIS/MSI · DMG/ZIP · AppImage/deb | Un installateur par plateforme |

> Pourquoi pas Electron : ~150 Mo + 2× Chromium en RAM, en tension directe
> avec C3 quand un LLM 7B tourne à côté. Pourquoi pas Flutter : l'intégration
> des moteurs C (whisper.cpp / llama.cpp) passe par du FDI plus pénible.
> ADR-001/003 arrêtés : Tauri 2 + Rust, LLM via llama.cpp (binaire dédié
> `tldl-llm` — cf. §5.2 : les deux moteurs ne peuvent pas partager un même
> binaire).

## 4. Structure du dépôt

_Projection Tauri (s'adapte si l'ADR-001 change) :_

```
tldl-summaries/
├─ src-tauri/                 # noyau Rust
│  ├─ Cargo.toml
│  ├─ build.rs                 # CMake (whisper.cpp + llama.cpp) + shims C
│  ├─ c/                       # shims C (whisper.cpp + llama.cpp, API plate)
│  ├─ tauri.conf.json
│  └─ src/
│     ├─ main.rs
│     ├─ tldl_llm_bin.rs       # binaire tldl-llm : moteur LLM, processus dédié (§5.2)
│     ├─ engine.rs             # moteur llama.cpp (FFI shim) — compilé par tldl-llm seul
│     ├─ tldl_e2e.rs           # binaire E2E hors app (F3) : pipeline seul, sans Tauri/rfd
│     ├─ manifest_tool.rs      # binaire tldl-manifest : diagnostic PE (manifest v6, 0xC0000139)
│     ├─ commands.rs           # API exposée au frontend (Tauri commands)
│     ├─ pipeline.rs           # orchestration : probe → wav → ASR → LLM
│     ├─ media.rs              # ffprobe + conversion WAV 16 kHz mono
│     ├─ asr/                  # intégration whisper.cpp (bindings Rust)
│     ├─ llm/                  # lancement du processus tldl-llm + prompt
│     ├─ config.rs             # config app (profil, modèles, langue, T)
│     ├─ models.rs             # catalogue local, téléchargement, statuts
│     └─ storage.rs            # SQLite : historique
├─ src/                       # frontend (React + TS, Vite)
│  ├─ App.tsx
│  ├─ pages/
│  │  ├─ ImportPage.tsx        # glisser-déposer + validation
│  │  ├─ ResultPage.tsx       # onglets Transcription | Résumé
│  │  └─ SettingsPage.tsx     # menu « Modèles » (F6)
│  ├─ components/             # FileDrop, SegmentList, SummaryView, ModelPicker…
│  └─ api/                    # appels aux Tauri commands
├─ models/                    # modèles GGUF téléchargés (hors git)
├─ assets/ffmpeg/             # binaires FFmpeg par OS
├─ vendor/whisper.cpp/        # whisper.cpp v1.9.5 (vendée, consommé tel quel)
└─ docs/                      # ← ces documents
```

> **Convention de fil frontend ↔ Rust** : les *valeurs de retour* des
> commandes suivent les attributs serde des structs (`rename_all =
> "camelCase"`, cf. `src/types.ts`) — Tauri 2 ne convertit que les
> *arguments* des commandes (camelCase JS → snake_case Rust), pas les retours.
> Garde-fou : test `pipeline_output_keys_are_camel_case`
> (`src-tauri/src/pipeline.rs`).

## 5. Pipeline

```
fichier audio/vidéo
   │  import + probe (ffprobe : format, durée, validité)
   │  F14 : section [début, fin] choisie à l'import (pré-écoute +
   │  sliders) — défaut = fichier complet
   ▼
WAV PCM 16 kHz mono          ← FFmpeg : **la section** [début, fin] seule
   │                            (-ss début + -t fin-début) ; C5 : vidéo →
   │                            audio dans l'app ; fichier temporaire,
   │                            supprimé après traitement
   ▼
Transcription                ← whisper.cpp (GGUF ; langue auto/FR ;
   │                             segments + horodatages)
   │  F14 : segments décalés de `début` (relatifs à la section → alignés
   │  sur le fichier source, cf. F13)
   ▼
Résumé structuré             ← LLM local (llama.cpp), prompt « rencontre »
   │                            Synthèse / Points clés / Décisions / Actions
   ▼
UI : double-vue (Transcription | Résumé) + copie + historique (SQLite)
   │  F14 : section consignée (records.section_start / section_end)
```

Remarque : ASR et LLM ne s'exécutent **pas en même temps** → la RAM se
dimensionne au **maximum** des deux, pas à leur somme
(dimensionnement, cf. [models.md](models.md)).

**Re-résumé sans re-transcription (F15/F16)** : si un fichier a déjà été
analysé, seule l'étape **LLM** est relancée sur la transcription stockée
(commande `re_summarize` → `pipeline::re_summary` — pas de sonde, pas de
conversion, pas d'ASR) ; un nouvel enregistrement est consigné (même
transcription, nouveau résumé). Le contexte de prompt (F15) s'applique au
résumé, comme au pipeline complet.

**Travail par section (F14)** : à l'import, l'utilisateur peut pré-écouter
le média et couper début/fin (sliders ; défaut = fichier complet). FFmpeg
ne convertit que la section en WAV 16 kHz mono (`-ss début` +
`-t fin-début`) — ASR et LLM ne traitent que cette section. Les segments,
émis relatifs à la section (0-based), sont **décalés de `début`** par le
pipeline : ils restent alignés sur le fichier source (le lecteur de la
page Résultat, F13, qui lit le source, saute au bon endroit sans
modification). La section est consignée (`records.section_start` /
`section_end`, bornes absolues du fichier source) et **préservée** par le
re-résumé (F16). `run_pipeline` reçoit `section_start` / `section_end`
(`Option<f64>` ; `None`/`0` = valeur par défaut) ; le pipeline borne à la
durée sonde (`effective_section`) : sentinelle `end = 0` = « jusqu'à la
fin », section vide (fin ≤ début) = erreur claire.

### 5.1 Prompt LLM (à affiner — cf. F4)

- Rôle : assistant de synthèse de rencontre.
- **Contexte utilisateur (F15)** : si un contexte de prompt est sélectionné
  à l'import, il **remplace le format par défaut** (Synthèse / Points clés /
  Décisions / Actions à venir) : le prompt ne contient que la transcription
  + l'instruction « Résume la transcription en suivant strictement ces
  instructions » suivie des instructions du contexte. À défaut (aucun
  contexte), prompt « rencontre » par défaut (format structuré ci-dessous).
  Il **oriente uniquement le résumé** (l'étape LLM) : la **transcription**
  (ASR) n'est **jamais** modifiée, elle reste fidèle à l'audio — on pilote
  seulement la *manière* dont la transcription est résumée. Le nom du
  contexte est consigné dans l'historique.
- Entrée : transcription horodatée (tronquée / fusionnée si > contexte).
- Sortie Markdown : **Synthèse** (1 paragraphe, 3–5 phrases) / **Points clés**
  (puces) / **Décisions** / **Actions à venir** (qui · quoi · échéance).
- Règles : langue = langue de la transcription ; jamais d'invention ;
  incertitude formulée explicitement ; « rien à signaler » accepté comme réponse.

### 5.2 LLM : binaire dédié `tldl-llm` (processus séparé)

Le LLM tourne dans un **processus dédié** `tldl-llm`, pas dans l'app.
Raison : le ggml de **whisper.cpp** et le ggml de **llama.cpp** exportent les
mêmes symboles C (`ggml_*`, 242/242 en double) ; lier les deux dans un même
binaire provoque de l'**interposition** et un **SIGSEGV** au premier
`ggml_compute` (reproduit, cible GNU). Renommer/prefixer les symboles
exigerait de forker les deux moteurs ; la séparation par processus est la
seule option propre, et elle colle au fait qu'**ASR et LLM ne sont jamais
concurrents** (§5).

Mécanique :

- L'**app** (`tldl.exe`) et `tldl-e2e` lient **uniquement** le ggml de
  whisper.cpp (C pur) → pas de `stdc++`, pas de `advapi32`.
- `tldl-llm` lie **uniquement** le ggml de llama.cpp (objets C++) →
  `stdc++` (ABI C++ : `__gxx_personality_seh0`, `__cxa_*`, …) et
  `advapi32` (ggml sonde le nom de la CPU dans le registre
  `HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0`). Ces deux liens sont
  épinglés par `#[link]` dans `tldl_llm_bin.rs` (cible GNU) : l'émission du
  build script n'atteint pas la ligne de lien de *ce* binaire.
- `llm/mod.rs` **spawne** `tldl-llm` : prompt dans un fichier temporaire,
  **progression** sur stderr (lignes `tldl-progress <produit> <max>`),
  **sortie** sur stdout. Le binaire cherche `tldl-llm.exe` à côté de
  l'exécutable courant (ou `TLDL_LLM_BIN`).
- Toute erreur LLM (démarrage, chargement GGUF, dépassement de contexte,
  crash du moteur) ne fait pas perdre la transcription → le résumé passe en
  bannière (`PipelineOutput.summary_error`).
- **Sampling** (shim C, chaîne `temp → top_p → penalties → dist`) :
  température 0.3, top_p 0.95, graine fixe 0 (résumés reproductibles), et
  **pénalité de répétition 1.2** (fenêtre 64 tokens ; freq/present à 0 —
  pas de pénalité sur toute la séquence, qui biaiserait les mots de liaison
  du français). Sans elle, le modèle dégenère en boucle : il re-récite le
  même bloc jusqu'à `max_tokens` sans EOS (ex. « résume en 10 phrases »).
  La pénalité casse la boucle *mot à mot* ; la boucle **paraphrasée**
  (les tokens diffèrent à chaque cycle) est coupée *a posteriori* par
  `truncate_repetition` (`llm/mod.rs` — similarité de paragraphes) ; et le
  fix **chat template** (ci-après) traite la cause racine.
- **Chat template** (shim C, fix carry LLM) : le prompt est le message
  `user` d'un dialogue à 1 message ; il est formaté **avant
  tokenisation** par `llama_chat_apply_template` (add_ass) dans le format
  d'entraînement du modèle (famille ` im_start`/` im_end` — liste
  prédéfinie de b10884, pas de Jinja) : le modèle *répond* au lieu de
  *compléter*, et l'EOS sort naturellement. Template : celui du GGUF
  (`tokenizer.chat_template`) s'il y en a un ; sinon, si le vocabulaire
  connaît le token ` im_start` comme 1 seul token (type ChatML — cas
  Qwen2.5, GGUF sans template), le template prédéfini `"chatml"` ; sinon
  prompt tel quel (chemin legacy). Buffer 2× le message (reco. du header) ;
  si le résultat le déborde : réallocation + re-apply.
- **`n_batch` du contexte LLM** (shim C) : posé à `n_ctx` — le prompt
  entier doit tenir en un seul `llama_decode` ; une transcription longue
  (ex. 15 min ≈ 3-4k tokens) déborde le défaut 512 et provoquait
  `GGML_ASSERT(n_tokens_all <= n_batch)` (plant du binaire `tldl-llm` sur
  un fichier de 15 min dans l'app). Validé dans l'app : résumé CPU d'un
  fichier de 15 min passe.

Vérifié (cible GNU, Qwen2.5-1.5B Q4_K_M) : ASR small → transcription, puis
`tldl-llm` génère le Markdown (Synthèse / Points clés / Décisions / Actions
à venir) sans crash ; le SIGSEGV d'interposition est parti.

## 6. Menu « Modèles » (rappel — spec complète : features.md F6)

- **ASR** : modèle (base / small / medium…), quantification, device (CPU/GPU)
- **LLM** : modèle (famille / taille), quantification, température, contexte
- **Téléchargements** : opt-in, taille affichée avant, progression, annulable,
  hors ligne par dépôt manuel dans `models/`
- **Estimation de ressource** (RAM/VRAM) par configuration + test de charge
- **Profils** : Léger / Équilibré / Puissant (cf. [models.md](models.md))

## 7. Stockage & confidentialité

- **Historique** : SQLite local (fichier, date, durée, modèles utilisés,
  textes transcription + résumé) — consignation **auto après chaque
  pipeline réussi et chaque re-résumé (F15/F16)** (texte + segments
  horodatés en JSON), recherche plein texte, réouverture du résultat (F7)
- **Contextes de prompt (F15)** : table `contexts` de la même base SQLite
  (nom, instructions, favori, horodatage) ; le nom du contexte utilisé est
  consigné dans `records` (`context_name`) ; migration idempotente à
  l'ouverture (colonne `context_name`)
- **Dossiers de rangement (F19)** : table `folders` de la même base
  SQLite (nom, horodatage) + colonne `records.folder_id` — au plus un
  dossier par enregistrement, `NULL` = « Sans dossier » ; la suppression
  d'un dossier ne supprime aucun résultat (ses enregistrements repassent
  « Sans dossier ») ; migration idempotente à l'ouverture (colonne
  `folder_id`)
- **Zéro télémétrie**, zéro crash report, zéro analytique
- **Fichiers source non copiés** : l'app ne travaille que sur la piste audio
  extraite ; le média original reste chez l'utilisateur
- **Temporaires** : WAV intermédiaire systématiquement effacé
- **Lecteur audio (F13)** : lit le fichier **source** (l'intermédiaire WAV
  étant effacé) via le **asset protocol** Tauri (`asset://`,
  `app.security.assetProtocol` + feature `protocol-asset`) ; dépend du média
  original qui reste chez l'utilisateur (non copié, voir ci-dessus)
- **Seule exception réseau** : téléchargement des modèles (opt-in, traçable
  dans les logs locaux)

## 8. Décisions (ADR)

| ID | Décision | Statut |
|----|----------|--------|
| ADR-001 | Shell : **Tauri 2 + Rust** (alt. Electron, Flutter) | ✅ décidé |
| ADR-002 | ASR : **whisper.cpp** (alt. faster-whisper/Python, WhisperKit) | ✅ décidé (v1.9.5 vendée) |
| ADR-003 | LLM : **llama.cpp intégré** (alt. Ollama) — processus unique, portable | ✅ décidé |
| ADR-004 | Média : **FFmpeg embarqué**, vidéo → audio dans l'app | ✅ (C5) |
| ADR-005 | Frontend : **React + TS** (Vite) | ✅ décidé (stack Tauri 2) |
| ADR-006 | Historique : **SQLite** local (rusqlite `bundled`, compilé dans l'app) | ✅ décidé |
| ADR-007 | Zéro télémétrie, zéro cloud | ✅ (C1) |

## 9. Alternatives écartées (pour mémoire)

- **Electron** : écosystème le plus mûr, mais 2× Chromium + ~150 Mo — en
  tension avec C3
- **faster-whisper (CTranslate2/Python)** : perf excellente, mais embarque un
  runtime Python (packaging cross-plate plus lourd que le C pur de whisper.cpp)
- **WhisperKit** : macOS/iOS uniquement
- **Ollama** : gestion des modèles très pratique (pull, swap à chaud), mais
  c'est un démon séparé — écarté au profit d'un processus unique 100 % portable
  (ADR-003)
- **LLM ≥ 13B** : hors C3, sauf profil « Puissant » (cf. models.md)

## 10. Prérequis de build (développeur)

L'app ne charge que des **modèles** (opt-in, F11) ; la *build* quant à elle
exige une toolchain C/C++ (les moteurs ASR/LLM sont vendés et compilés
localement) :

| Outil | Rôle |
|---|---|
| **CMake** ≥ 3.16 | configure/build de `vendor/whisper.cpp` (libs statiques `whisper` + `ggml`, cible `whisper`, `BUILD_SHARED_LIBS=OFF`, sans exemples/tests) |
| **Compilateur C++** | MSVC (Build Tools « C++ », v143) sur Windows · `clang++`/`gcc` sur macOS · `g++` sur Linux |
| **Rust** (stable, cible `x86_64-pc-windows-msvc` sur Windows) | noyau Tauri |

Détails d'intégration (F3, ADR-002) :

- `src-tauri/build.rs` : lance CMake (lib statiques sous
  `target/<OUT_DIR>/whisper-build` **et** `llama-build`), compile les deux
  shims `src-tauri/c/tldl_{whisper,llama}_shim.c` (crate `cc`), émet les
  directives de lien. Sans CMake/compilateur, `cargo check` passe mais le
  binaire final manquera les symboles (échec de lien explicite — pas
  d'échec silencieux).
- **Lien par cible** (§5.2) : le build script ne lie les archives ggml que
  via les `#[link]` de chaque binaire — l'app/`tldl-e2e` lient le ggml de
  whisper.cpp seul, `tldl-llm` le ggml de llama.cpp seul (+ `stdc++` +
  `advapi32`). Les deux ggml ne partagent donc jamais un même espace
  d'adresses. Les archives des backends GPU (`ggml-cuda`…, `tldl_ggml-*`)
  sont liées par `#[cfg(tldl_gpu_*)]`, émises seulement quand `TLDL_GPU`
  les a fait compiler (F10).
- Le shim C remplit `whisper_context_params` / `whisper_full_params` côté C
  (structs internes non stables) et n'expose à Rust qu'une API plate :
  init / transcribe (PCM s16 16 kHz mono) / progression 0..100 / segments
  (texte + t0/t1 en centièmes de seconde) / langue détectée.
- Audio : `whisper_full()` ne prend que du f32 → conversion s16→f32 dans le
  shim, libérée après l'appel.
- **GPU optionnel (C3, F10)** : build CPU-only par défaut ; la variable
  d'environnement `TLDL_GPU` (`cpu` | `cuda` | `metal` | `vulkan`) pilote
  dans `build.rs` les options CMake `GGML_CUDA` / `GGML_METAL` /
  `GGML_VULKAN` des **deux** ggml (whisper.cpp + llama.cpp) et les cfg
  `tldl_gpu_*` des `#[link]` des archives GPU (app/E2E : `ggml-cuda`… ;
  `tldl-llm` : `tldl_ggml-cuda`…). À l'exécution, `src/gpu.rs` détecte les
  devices ggml et bascule CPU si le GPU demandé est absent
   (`gpu::resolve`, note affichée). Sous Windows + toolchain GNU,
   `TLDL_GPU=cuda` exige `CUDA_PATH` (ou `CUDAToolkit_ROOT`) pointant le
   toolkit CUDA, et `build.rs` cale deux choses que CMake/nvcc ne
   résolvent pas tout seuls sous MinGW : `CUDA_CUDART` (le
   `FindCUDAToolkit` de CMake ≥ 4 ne cherche pas `lib/x64` pour
   l'architecture cible GNU `X8664` — la variable de cache court-circuite
   la recherche de `cudart.lib`) et `CMAKE_CUDA_HOST_COMPILER` (g++ du
   toolchain — sans lui `nvcc` cherche `cl.exe`).
- **Lancer le dev GPU (Windows)** : `npm run dev:gpu` → wrapper
  `src-tauri/scripts/dev-gpu.ps1`, qui (1) recharge l'env VS **capturé et
  versionné** (`src-tauri/scripts/vscmd_env.txt` — cl/nmake/INCLUDE/LIB),
  (2) fixe le gate `TLDL_GPU=cuda` + `CUDA_PATH`, (3) met `bin\x64`
  (cublas64/cudart64) sur le PATH **runtime** (DLL chargées au boot de
  l'app), puis `npm run tauri dev`. Un `npm run tauri dev` dans un shell nu
  **échoue** : `build.rs` sans env VS/CUDA (`nmake`/`cl` introuvables →
  build script exit 101) et, une fois l'app lancée, `STATUS_DLL_NOT_FOUND`
  sans le `bin\x64` runtime. `vscmd_env.txt` est un *snapshot* de la
  toolchain VS de la machine (chemins absolus) : à régénérer si le VS Build
  Tools se déplace.
- **Build CPU figé** : `build.rs` pousse `GGML_CUDA=OFF` **explicitement**
  (chemin CPU) — le cache CMake est sticky : sans flag explicite, une
  config `TLDL_GPU=cuda` antérieure (`GGML_CUDA=ON`) persisterait au
  passage GPU → CPU, et le lien casserait (symbole
  `ggml_backend_cuda_reg` orphelin — l'archive `ggml-cua` n'est liée que
  sous le cfg `tldl_gpu_cuda`)
- `rusqlite` est épinglé à **0.38** (dernière version avec la feature
  `bundled`) : SQLite est compilé dans l'app, aucune dépendance système
  (portabilité C1/C2).
- **Seule dépendance réseau** (C4) : `reqwest` **blocking** + **rustls**
  (TLS pur Rust, provider crypto `aws-lc-rs` — C compilé localement, aucune
  dépendance système : OK cible GNU) ; utilisée uniquement par F11
  (téléchargement opt-in). Aucun autre réseau dans l'app.

### 10.1 Cible Windows : MSVC ou MinGW (GNU)

- **MSVC** (`x86_64-pc-windows-msvc`) : le manifest **Common-Controls v6** est
  émis automatiquement par le linker MSVC — `cargo build` **et** `cargo test`
  passent sans précaution. C'est la **seule** cible Windows utilisable pour
  `TLDL_GPU=cuda` : `nvcc` n'accepte que `cl.exe` comme compilateur hôte
  (MinGW est rejeté — « Host compiler targets unsupported OS »).
- **MinGW/WinLibs** (`x86_64-pc-windows-gnu`, gcc 16.2) : `build.rs` détecte la
  cible `Gnu` — CMake via **Ninja**, ggml **sans OpenMP** (`GGML_OPENMP=OFF`),
  et des copies `lib`-préfixées des archives ggml (`libggml.a`, exigées par
  le `ld` GNU). `stdc++` et `advapi32` ne sont **pas** émis par le build
  script (l'émission n'atteint pas la ligne de lien de `tldl-llm`) : ils
  sont épinglés par `#[link]` dans `tldl_llm_bin.rs` (cf. §5.2).
  - Le linker GNU ne fusionne pas le manifest **Common-Controls v6** requis par
    `rfd`/`TaskDialogIndirect` (`.rsrc merge failure: multiple non-default
    manifests`). L'**app** (`tldl.exe`) se charge quand même (le v6 y figure —
    vérifié) ; le **binaire de test** `cargo test` (`tldl_lib-*.exe`) meurt au
    chargement avec **0xC0000139** (`STATUS_ENTRYPOINT_NOT_FOUND`). C'est le
    seul binaire affecté.

### 10.2 E2E hors de l'app — binaire `tldl-e2e`

Le binaire de test étant inutilisable sur la cible GNU, la validation E2E (F3)
passe par un **binaire dédié** `tldl-e2e` (`src/tldl_e2e.rs`) : il ne lie que
`tldl_lib::pipeline` (ni le shell Tauri ni `rfd`), ne référence donc pas
`TaskDialogIndirect`, et **se charge sans manifest v6**.

```
tldl-e2e <fichier_média> [dossier_ffmpeg]     # ou TLDL_E2E_INPUT / TLDL_FFMPEG_DIR
TLDL_E2E_LLM=<id>                            # modèle LLM (sinon qwen25-7b-q4)
tldl-e2e download <id> <url>                 # F11 : vrai moteur models::download
tldl-e2e delete <id>                        # F11 : vrai moteur models::delete
tldl-e2e history                            # F7 : storage save/list/get/search
tldl-e2e devices                            # F10 : devices ggml + gpu::resolve
tldl-e2e section [fichier_média] [dossier_ffmpeg]
                                             # F14 : coupe de section (2 passes :
                                             # fichier complet + section [20 %, 80 %])
TLDL_E2E_ASR=<id>                           # id du modèle ASR du mode section (sinon small)
tldl-e2e llm                                # carry LLM : passe réelle « 10 phrases »
```

Le `dossier_ffmpeg` est **optionnel** : `media.rs` s'auto-localise
(`TLDL_FFMPEG_DIR` → bundle `<exe>/_up_/assets/ffmpeg/` → remontée du repo
`assets/ffmpeg/` → PATH). `TLDL_E2E_LLM` est l'**identifiant** du GGUF
(`models/llm-<id>.gguf`).

Vérifié (whisper **small**, `samples/jfk.wav`, LLM = Qwen2.5-1.5B Q4_K_M
`qwen25-1.5b-q4`) : sonde → WAV 16 kHz → transcription (langue auto « en »,
segments + horodatages) → **résumé réel** : `tldl-llm` (processus dédié, §5.2)
génère le Markdown. Sans GGUF déposé, **dégradation gracie** : transcription
conservée + bannière « modèle LLM absent » (F4/F11).

Mode `download` (F11) : passe le vrai moteur `models::download` sur une URL
quelconque (serveur local `HttpListener` 127.0.0.1, ou HTTPS réel) et
assertionne `taille disque = progression = Content-Length`, sans résidu
`.part` en cas d'échec. Vérifié : HTTP local 3 MiB (OK), 404 (erreur nette,
aucun `.part`), **HTTPS réel** (`.crate` crates.io, 83 669 o — TLS rustls +
provider `aws-lc-rs` sur la cible MinGW, redirection suivie).

Mode `delete` (F11) : passe le vrai moteur `models::delete` sur un fichier
factice (id hors catalogue — aucun modèle réel n'est touché) : écriture,
suppression, vérification de la disparition.

Mode `history` (F7) : va-et-retour factice `tldl_lib::storage` dans un
dossier temporaire (jamais l'historique réel de l'app) — save → list →
get → search plein texte, puis nettoyage.

Mode `llm` (carry LLM, fix chat template) : passe **réelle** du LLM — vraie
génération sur le vrai modèle (`TLDL_E2E_LLM`, défaut `qwen25-7b-q4`) avec
le contexte « Résume en 10 phrases maximum. », sur une transcription
**longue** (≈ 700 tokens — au-delà du `n_batch` 512, cf. §5.2) →
`llm::summarize` (spawne le binaire `tldl-llm` réel, même dossier que
celui-ci). Vérifie l'exigence
dure : **EOS naturel** (la génération s'arrête avant `max_tokens`),
**pas de méta-préambule** (« voici le résumé »…), **au plus 10 phrases**
(heuristique sur la ponctuation terminale) — le filet `truncate_repetition`
reste en place (§5.2). Modèle absent → ignorée (exit 0), comme les autres
modes. Vérifié (`qwen25-7b-q4`, CPU) : **EOS naturel à 190/1024 tokens**,
10 phrases (la limite), sans méta-préambule — le fix chat template tient
sur le 7B, et le prompt long (≈ 700 tokens) passe (`n_batch = n_ctx`).

> Outil annexe `tldl-manifest` (`src/manifest_tool.rs`) : diagnostic/patch du
> manifest PE d'un binaire Windows (vérifie le v6, `dump` de l'arborescence
> `.rsrc`). Hors du chemin critique E2E — ne pas l'invoquer sur un binaire
> fonctionnel (le mode `patch` a démontré ses limites sur la cible GNU).

### 10.3 macOS — Apple Silicon (`aarch64-apple-darwin`)

La build macOS se fait **sur** un Mac (C2 — pas de croisé depuis Windows ;
le DMG/ZIP est assemblé sur le poste de build). Prérequis : Xcode Command
Line Tools (`xcode-select --install` — fournit `/usr/bin/c++`, clang++ et
make) et CMake (`brew install cmake`).

Mécanique (`build.rs` + `tldl_llm_bin.rs`, cf. §5.2) :

- `build.rs` détecte `Toolchain::Apple` : `env::consts::OS == "macos"` +
  `/usr/bin/c++` présent. Le séparateur de `PATH` est `:` (fn `path_sep`) —
  les scans `find_cmake` / `gnu_candidates` / `find_toolchain` l'utilisent.
- Pas de Ninja (générateur CMake par défaut : Unix Makefiles), pas de
  préfixe `lib` à fabriquer (les archives CMake Apple sortent déjà
  `lib*.a`), pas d'ancrage CUDA (`TLDL_GPU=cuda` reste Windows/Linux —
  l'ancrage `CUDA_CUDART` est un no-op hors Windows).
- **Runtime C++ libc++** : mesure double — le build script émet
  `cargo:rustc-link-lib=dylib=c++` (l'app), et `tldl_llm_bin.rs` épingle
  `#[cfg(target_os = "macos")] #[link(name = "c++")]` (le binaire
  `tldl-llm — l'émission du build script n'atteint pas sa ligne de lien,
  cf. §5.2) ; un `-lc++` en double est inoffensif.
- **GPU optionnel (C3, F10)** : `TLDL_GPU=metal` refuse un host non-macOS
  (gate dans `build.rs`) ; sur le Mac, `GGML_METAL=ON` des **deux** ggml +
  cfg `tldl_gpu_metal` → `#[link]` `ggml-metal` (app, `asr/mod.rs`) et
  `tldl_ggml-metal` (`tldl-llm`) ; `gpu.rs` détecte le Metal à
  l'exécution et bascule CPU si absent.
