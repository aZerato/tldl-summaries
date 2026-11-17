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

## 5. Pipeline

```
fichier audio/vidéo
   │  import + probe (ffprobe : format, durée, validité)
   ▼
WAV PCM 16 kHz mono          ← FFmpeg (C5 : vidéo → audio dans l'app)
   │                            fichier temporaire, supprimé après traitement
   ▼
Transcription                ← whisper.cpp (GGUF ; langue auto/FR ;
   │                             segments + horodatages)
   ▼
Résumé structuré             ← LLM local (llama.cpp), prompt « rencontre »
   │                            Synthèse / Points clés / Décisions / Actions
   ▼
UI : double-vue (Transcription | Résumé) + copie + historique (SQLite)
```

Remarque : ASR et LLM ne s'exécutent **pas en même temps** → la RAM se
dimensionne au **maximum** des deux, pas à leur somme
(dimensionnement, cf. [models.md](models.md)).

### 5.1 Prompt LLM (à affiner — cf. F4)

- Rôle : assistant de synthèse de rencontre.
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
  textes transcription + résumé)
- **Zéro télémétrie**, zéro crash report, zéro analytique
- **Fichiers source non copiés** : l'app ne travaille que sur la piste audio
  extraite ; le média original reste chez l'utilisateur
- **Temporaires** : WAV intermédiaire systématiquement effacé
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
  d'adresses.
- Le shim C remplit `whisper_context_params` / `whisper_full_params` côté C
  (structs internes non stables) et n'expose à Rust qu'une API plate :
  init / transcribe (PCM s16 16 kHz mono) / progression 0..100 / segments
  (texte + t0/t1 en centièmes de seconde) / langue détectée.
- Audio : `whisper_full()` ne prend que du f32 → conversion s16→f32 dans le
  shim, libérée après l'appel.
- Build CPU-only par défaut ; le flag `use_gpu` est transmis (sans effet sans
  CUDA) — l'accélération GPU propre est F10.
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
  passent sans précaution.
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
```

Le `dossier_ffmpeg` est **optionnel** : `media.rs` s'auto-localise
(`TLDL_FFMPEG_DIR` → bundle `<exe>/_up_/assets/ffmpeg/` → remontée du repo
`assets/ffmpeg/` → PATH). `TLDL_E2E_LLM` est l'**identifiant** du GGUF
(`models/llm-<id>.gguf`).

Vérifié (whisper **small**, `samples/jfk.wav`, LLM = Qwen2.5-1.5B Q4_K_M
`smoke-test`) : sonde → WAV 16 kHz → transcription (langue auto « en »,
segments + horodatages) → **résumé réel** : `tldl-llm` (processus dédié, §5.2)
génère le Markdown. Sans GGUF déposé, **dégradation gracie** : transcription
conservée + bannière « modèle LLM absent » (F4/F11).

Mode `download` (F11) : passe le vrai moteur `models::download` sur une URL
quelconque (serveur local `HttpListener` 127.0.0.1, ou HTTPS réel) et
assertionne `taille disque = progression = Content-Length`, sans résidu
`.part` en cas d'échec. Vérifié : HTTP local 3 MiB (OK), 404 (erreur nette,
aucun `.part`), **HTTPS réel** (`.crate` crates.io, 83 669 o — TLS rustls +
provider `aws-lc-rs` sur la cible MinGW, redirection suivie).

> Outil annexe `tldl-manifest` (`src/manifest_tool.rs`) : diagnostic/patch du
> manifest PE d'un binaire Windows (vérifie le v6, `dump` de l'arborescence
> `.rsrc`). Hors du chemin critique E2E — ne pas l'invoquer sur un binaire
> fonctionnel (le mode `patch` a démontré ses limites sur la cible GNU).
