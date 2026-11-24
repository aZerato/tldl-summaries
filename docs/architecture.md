# Architecture

## 1. Objectif

Transformer un enregistrement (rencontre, entretien, réunion, podcast…) en :

1. une **transcription** fidèle (segments + horodatages) ;
2. un **résumé structuré** (synthèse, points clés, décisions, actions à
   venir).

Les deux restent accessibles dans l'interface (C6). Tout s'exécute en
local : ni Internet, ni cloud.

## 2. Contraintes

| # | Contrainte |
|---|---|
| C1 | **100 % local** à l'usage : aucune donnée ne quitte la machine (zéro télémétrie) |
| C2 | **Cross-plateforme** : Windows, macOS, Linux |
| C3 | **Sobre mais performant** : modèles quantifiés, GPU optionnel, CPU viable |
| C4 | **Hors ligne** : aucun réseau requis (hors téléchargement des modèles, opt-in) |
| C5 | **Vidéo gérée nativement** : conversion vidéo → audio dans l'application (FFmpeg) |
| C6 | **Interface** : import de fichier, double-vue (transcription / résumé), page « Configuration » |

## 3. Stack

| Brique | Choix | Rôle |
|---|---|---|
| Shell / UI | **Tauri 2** (Rust) + **React / TypeScript** (Vite) | application native légère (~10 Mo de surcharge) |
| Cœur | **Rust** | pipeline média / ASR / LLM, config, stockage |
| ASR | **whisper.cpp** (modèles GGUF) | transcription multilingue, horodatages |
| LLM | **llama.cpp** (binaire dédié `tldl-llm`, §5.2) | résumé — modèles 3B → 8B Q4_K_M |
| Média | **FFmpeg** (embarqué) | sonde + conversion WAV 16 kHz mono |
| Stockage | **SQLite** (rusqlite) | historique local |
| Packaging | Tauri : NSIS/MSI · DMG/ZIP · AppImage/deb | un installateur par plateforme |

> Pourquoi pas Electron : ~150 Mo + 2× Chromium en RAM, en tension avec C3
> quand un LLM 7B tourne à côté. Pourquoi pas Flutter : l'intégration des
> moteurs C passe par du FFI plus pénible. (ADR-001/003 arrêtés.)

## 4. Structure du dépôt

```
tldl-summaries/
├─ src-tauri/                 # noyau Rust
│  ├─ build.rs                # CMake (whisper.cpp + llama.cpp) + shims C
│  ├─ c/                      # shims C (API plate)
│  └─ src/
│     ├─ tldl_llm_bin.rs     # binaire tldl-llm (moteur LLM, §5.2)
│     ├─ engine.rs           # moteur llama.cpp (FFI) — compilé par tldl-llm seul
│     ├─ tldl_e2e.rs         # binaire E2E hors app (§10.2)
│     ├─ manifest_tool.rs    # binaire tldl-manifest (diagnostic PE)
│     ├─ commands.rs         # API frontend (Tauri commands)
│     ├─ pipeline.rs         # orchestration : probe → WAV → ASR → LLM
│     ├─ media.rs            # ffprobe + conversion WAV
│     ├─ asr/                # whisper.cpp (bindings)
│     ├─ llm/                # spawn de tldl-llm + prompt
│     ├─ config.rs           # config (profil, modèles, langue, T)
│     ├─ models.rs           # catalogue, téléchargement, statuts
│     └─ storage.rs          # SQLite : historique, contextes, dossiers
├─ src/                       # frontend (React + TS, Vite)
│  ├─ pages/                  # Import / Result / History / Contexts / Settings
│  ├─ components/             # SegmentList, Progress…
│  └─ api/tauri.ts           # appels aux commands
├─ models/                    # modèles GGUF (hors git)
├─ assets/ffmpeg/             # binaires FFmpeg par OS
├─ vendor/whisper.cpp/        # v1.9.5 (vendue)
└─ docs/
```

> **Convention frontend ↔ Rust** : les *valeurs de retour* des commands
> suivent les attributs serde (`rename_all = "camelCase"`, cf.
> `src/types.ts`) — Tauri 2 ne convertit que les *arguments* des commands.
> Garde-fou : test `pipeline_output_keys_are_camel_case`.

## 5. Pipeline

```
fichier audio/vidéo
   │  import + sonde (ffprobe : format, durée)
   │  F14 : section [début, fin] optionnelle — défaut = fichier complet
   ▼
WAV PCM 16 kHz mono          ← FFmpeg : section seule (-ss début -t fin-début)
   ▼
Transcription                ← whisper.cpp (GGUF) ; langue auto/forcée
   │  F14 : segments décalés de `début` (restés alignés sur le fichier source)
   │  F9 : diarisation (ONNX Runtime dans l'app — ADR-008) :
   │       « Intervenante N » par segment (un embedding par segment)
   ▼
Résumé structuré             ← tldl-llm (llama.cpp) : prompt « rencontre »
   │                            ou contexte F15 (remplace le format par défaut)
   │                            transcription avec intervenants (F9)
   ▼
UI : double-vue (Transcription | Résumé) + historique (SQLite)
   │  F14 : section consignée (records.section_start / section_end)
```

- ASR et LLM ne sont jamais concurrents → la RAM se dimensionne au
  **maximum** des deux, pas à leur somme (cf. [models.md](models.md)).
- **Re-résumé sans re-transcription (F16)** : si le fichier a déjà été
  analysé, seule l'étape LLM est relancée sur la transcription stockée
  (commande `re_summarize` → `pipeline::re_summary`) ; un nouvel
  enregistrement est consigné (même transcription, nouveau résumé).
- **Travail par section (F14)** : FFmpeg ne convertit que `[début, fin]` ;
  les segments, émis 0-based sur la section, sont décalés de `début` pour
  rester alignés sur le fichier source ; la section est consignée et
  préservée au re-résumé. Bornage : `effective_section` (borne = durée
  sonde ; `end = 0` = « jusqu'à la fin » ; fin ≤ début = erreur).

### 5.1 Prompt LLM

- Rôle : assistant de synthèse de rencontre.
- **Contexte utilisateur (F15)** : si sélectionné, il remplace le format
  par défaut ; il n'agit que sur le résumé (jamais sur la transcription).
- Entrée : transcription horodatée.
- Sortie Markdown : Synthèse (1 paragraphe) / Points clés / Décisions /
  Actions à venir (qui · quoi · échéance).
- Règles : langue de la transcription ; rien d'inventé ; incertitude
  formulée explicitement ; « rien à signaler » accepté.

### 5.2 LLM : binaire dédié `tldl-llm`

Le LLM tourne dans un **processus séparé**, pas dans l'app : les ggml de
whisper.cpp et llama.cpp exportent les mêmes symboles C (`ggml_*`) — les
lier dans un même binaire provoque une interposition et un SIGSEGV au
premier `ggml_compute` (reproduit, cible GNU). La séparation par processus
est la seule option propre, et elle colle au fait que ASR et LLM ne sont
jamais concurrents (§5).

- **Lien** : l'app (`tldl.exe`) et `tldl-e2e` lient le ggml de whisper.cpp
  seul (C pur) ; `tldl-llm` lie le ggml de llama.cpp seul (+ `stdc++` +
  `advapi32`, épinglés par `#[link]` dans `tldl_llm_bin.rs` — l'émission
  du build script n'atteint pas la ligne de lien de ce binaire, cible
  GNU).
- **Protocole** : `llm/mod.rs` spawne `tldl-llm` — prompt dans un fichier
  temporaire, progression sur stderr (`tldl-progress <produit> <max>`),
  sortie sur stdout. Le binaire cherche `tldl-llm.exe` à côté de
  l'exécutable (ou `TLDL_LLM_BIN`).
- **Dégradation** : toute erreur LLM (démarrage, chargement, contexte,
  crash) ne fait pas perdre la transcription → bannière
  (`PipelineOutput.summary_error`).
- **Échantillonnage** (shim C) : temperature 0,3, top_p 0,95, graine fixe
  0, **pénalité de répétition 1,2** (fenêtre 64 tokens ; freq/present à
  0). Sans elle, le modèle re-récite le même bloc jusqu'à `max_tokens`
  sans EOS ; la boucle *paraphrasée* est coupée après par
  `truncate_repetition` (similarité de paragraphes, `llm/mod.rs`).
- **Chat template** (shim C) : le prompt est le message `user` d'un
  dialogue à un message, formaté **avant tokenisation** par
  `llama_chat_apply_template` dans le format d'entraînement du modèle —
  le modèle *répond* au lieu de *compléter*, l'EOS sort naturellement.
  Choix du template : celui du GGUF (`tokenizer.chat_template`) s'il y en
  a un ; sinon, si le vocabulaire connaît ` im_start` comme un seul token
  (type ChatML, cas Qwen2.5), le template prédéfini `"chatml"` (liste de
  b10884, pas de Jinja) ; sinon prompt tel quel (chemin legacy). Buffer
  2× le message ; débordement → réallocation + re-apply.
- **`n_batch` = `n_ctx`** (shim C) : le prompt entier doit tenir en un
  seul `llama_decode` ; le défaut 512 faisait planter `tldl-llm` sur une
  transcription longue (≈ 3-4k tokens,
  `GGML_ASSERT(n_tokens_all <= n_batch)`). Validé dans l'app : résumé
  CPU d'un fichier de 15 min.
### 5.3 Diarisation (F9) — ONNX Runtime dans l'app

La séparation des intervenants tourne **dans l'app** (pas de binaire
dedié) : ONNX Runtime n'a pas de collision de symboles `ggml_*` avec
whisper.cpp (§5.2) — l'API C se lie comme les autres libraries natives.
Le modèle d'intervenants (ONNX, F11 opt-in) produit **un embedding par
segment** (l'intera utterance, frames fbank de `[début, fin)` — bien
plus discriminatif que des fenêtres de 1,6 s ; segments < 1 s sans
étiquette) ; le **clustering en Rust** (k-moyennes cosinus — testable
`cargo test`, `diar::tests`) regroupe les embeddings en intervenants ;
le nombre auto = **creux + effondrement** de la courbe de fusion
(`diar::auto_k` : saut ≥ `MIN_GAP` = 0,05 **et** dernière fusion ≤
`COLLAPSE_RATIO` = 0,55 × première — 8/8 régimes du bench F12 classés,
cf. models.md §7) ; N forcé = **exactement N** ;
chaque segment porte l'étiquette de son propre cluster (« Intervenante
1 », « Intervenante 2 »…).

- **Emplacement** : après la transcription (même WAV 16 kHz), avant le
  résumé — ASR, diarisation et LLM ne sont jamais concurrents → la RAM
  se dimensionne au maximum.
- **Nombre** : on/off dans la config (`diarize`, comme le GPU — pas un
  argument de commande) ; le nombre d'intervenants (auto / N) est
  demandé **au moment de la transcription** — argument de commande
  `diar_speakers` (0 = auto, N = **exactement N**, pas persisté).
- **Résumé** : le LLM reçoit la transcription **avec les
  intervenants**, segment par segment
  (`[début–fin] Intervenante 1: texte`) — qui a dit quoi (décisions,
  actions à venir attribuées) ; le contexte F15 n'est pas modifié.
- **Dégradation gracie** : modèle absent (F11 opt-in) → pas
  d'intervenant, aucun impact transcription ni résumé (bannière, même
  pattern que le LLM).
- **Consignation** : champ `speaker` dans le JSON des segments
  (`transcription_json` — JSON libre, pas de migration) ; préservé au
  re-résumé (F16) et à la section (F14).
- **ONNX Runtime** : prébuilds par OS sous `assets/onnxruntime/<os>/`
  (`.dll` / `.dylib` / `.so`), même pattern que FFmpeg
  (`assets/ffmpeg/`) — pas de build par plateforme ; résolution comme
  `media.rs` (`TLDL_ORT_DIR` → à côté de l'exécutable du bundle →
  assets du repo).
- **GPU** : pas de GPU en v1 (CPU viable — C3) ; optionnel plus tard
  (provider CUDA d'ONNX Runtime).

## 6. Page « Configuration »

Spécification : features.md F6. Contenu : ASR (modèle, quantification,
device), LLM (modèle, quantification, température, contexte),
téléchargements opt-in, estimation RAM/VRAM, profils
(Léger / Équilibré / Puissant, cf. [models.md](models.md)).

## 7. Stockage & confidentialité

- **Historique** : SQLite local — transcription (+ segments en JSON),
  résumé, modèles, device, contexte, section ; consignation automatique
  après chaque pipeline réussi et chaque re-résumé ; recherche plein
  texte ; réouverture (F7).
- **Contextes (F15)** : table `contexts` (même base) ; le nom du contexte
  utilisé est consigné dans `records` (`context_name`).
- **Dossiers (F19)** : table `folders` + `records.folder_id` — au plus un
  dossier par enregistrement ; supprimer un dossier ne supprime aucun
  résultat (ses enregistrements repassent « Sans dossier »).
- Migrations idempotentes à l'ouverture de la base.
- **Zéro télémétrie**, zéro crash report.
- **Fichiers source non copiés** : l'app travaille sur la piste audio
  extraite ; le média original reste chez l'utilisateur. Le lecteur
  (F13) lit donc le fichier source via le **asset protocol** Tauri
  (`asset://`).
- Le WAV intermédiaire est systématiquement effacé.
- **Seule exception réseau** : téléchargement des modèles (opt-in).

## 8. Décisions (ADR)

| ID | Décision | Statut |
|----|----------|--------|
| ADR-001 | Shell : **Tauri 2 + Rust** (alt. Electron, Flutter) | ✅ décidé |
| ADR-002 | ASR : **whisper.cpp** (alt. faster-whisper/Python, WhisperKit) | ✅ décidé (v1.9.5 vendue) |
| ADR-003 | LLM : **llama.cpp intégré** (alt. Ollama) — processus unique, portable | ✅ décidé |
| ADR-004 | Média : **FFmpeg embarqué**, vidéo → audio dans l'app | ✅ décidé (C5) |
| ADR-005 | Frontend : **React + TypeScript** (Vite) | ✅ décidé |
| ADR-006 | Historique : **SQLite** local (rusqlite `bundled`) | ✅ décidé |
| ADR-007 | Zéro télémétrie, zéro cloud | ✅ décidé (C1) |
| ADR-008 | Diarisation (F9) : **ONNX Runtime chargé au runtime** (`load-dynamic`, dlopen du prébuild embarqué) + clustering en Rust (alt. pyannote / 3D-Speaker — Python) | ✅ décidé |

## 9. Alternatives écartées (pour mémoire)

| Alternative | Pourquoi non |
|---|---|
| Electron | 2× Chromium + ~150 Mo — en tension avec C3 |
| faster-whisper (Python/CTranslate2) | embarque un runtime Python, packaging plus lourd que le C pur |
| WhisperKit | macOS/iOS uniquement |
| Ollama | un démon séparé — écarté au profit d'un processus unique (ADR-003) |
| pyannote 3.1 / 3D-Speaker (Python/PyTorch) | diarisation (F9) : embarque un runtime Python — même raison que faster-whisper (ADR-008) |
| LLM ≥ 13B | hors C3, sauf profil « Puissant » |

## 10. Prérequis de build (développeur)

L'app ne charge que des **modèles** (opt-in, F11) ; la *build* exige une
toolchain C/C++ (les moteurs sont vendus et compilés localement).

| Outil | Rôle |
|---|---|
| **CMake** ≥ 3.16 | build de `vendor/whisper.cpp` (libs statiques, `BUILD_SHARED_LIBS=OFF`, sans exemples/tests) |
| **Compilateur C++** | MSVC (Build Tools « C++ », v143) sous Windows · `clang++`/`gcc` sous macOS · `g++` sous Linux |
| **Rust** | stable ; cible `x86_64-pc-windows-msvc` sous Windows |

- `build.rs` : lance CMake (libs statiques sous
  `target/<OUT_DIR>/whisper-build` **et** `llama-build`), compile les deux
  shims C (`src-tauri/c/`), émet les directives de lien. Sans
  CMake/compilateur, `cargo check` passe mais le binaire final manquera
  les symboles (échec de lien explicite).
- **Lien par binaire** (§5.2) : chaque binaire lie son ggml via ses
  `#[link]` — les deux ggml ne partagent jamais un même espace
  d'adresses. Les archives GPU (`ggml-cuda`… ; `tldl_ggml-cuda`…) sont
  liées sous `#[cfg(tldl_gpu_*)]`, émises seulement quand `TLDL_GPU`
  les a fait compiler (F10).
- Le shim C remplit `whisper_context_params` / `whisper_full_params`
  côté C (structs internes non stables) et n'expose à Rust qu'une API
  plate : init / transcribe (PCM s16 16 kHz mono) / progression 0..100 /
  segments (texte + t0/t1 en centièmes de seconde) / langue détectée
  (`whisper_full()` ne prend que du f32 → conversion s16→f32 dans le
  shim, libérée après l'appel).
- **GPU optionnel (F10)** : `TLDL_GPU` (`cpu` défaut | `cuda` | `metal` |
  `vulkan`) pilote les options CMake `GGML_CUDA` / `GGML_METAL` /
  `GGML_VULKAN` des deux ggml + les cfg `tldl_gpu_*`. À l'exécution,
  `gpu.rs` détecte les devices ggml et bascule CPU si le GPU demandé est
  absent. Sous Windows, `cuda` exige la cible **MSVC** et `CUDA_PATH`
  (nvcc n'accepte que `cl.exe` en hôte) ; `build.rs` cale
  `CUDA_CUDART` (CMake ≥ 4 ne cherche pas `lib/x64` pour la cible GNU
  `X8664`) et `CMAKE_CUDA_HOST_COMPILER` (g++ du toolchain, sinon nvcc
  cherche `cl.exe`).
- **Lancer le dev GPU (Windows)** : `npm run dev:gpu` → wrapper
  `src-tauri/scripts/dev-gpu.ps1` : recharge l'env VS capturé
  (`vscmd_env.txt`, à régénérer si les Build Tools se déplacent), pose
  `TLDL_GPU=cuda` + `CUDA_PATH`, met `bin\x64` (cublas64/cudart64) sur
  le PATH runtime (DLL chargées au boot), puis `npm run tauri dev`. Un
  `tauri dev` dans un shell nu échoue (build script sans env VS ;
  `STATUS_DLL_NOT_FOUND` sans le runtime CUDA).
- **Build CPU figée** : `build.rs` pousse `GGML_CUDA=OFF`
  explicitement — le cache CMake est sticky ; sans ce flag, une config
  `cuda` antérieure persisterait et casserait le lien (symbole
  `ggml_backend_cuda_reg` orphelin).
- `rusqlite` épinglé à **0.38** (dernière avec la feature `bundled`) :
  SQLite compilé dans l'app, aucune dépendance système (C1/C2).
- **Seule dépendance réseau** (C4) : `reqwest` blocking + rustls (TLS pur
  Rust, provider `aws-lc-rs`) — utilisée uniquement par F11.

### 10.1 Cible Windows : MSVC ou MinGW (GNU)

- **MSVC** (`x86_64-pc-windows-msvc`) : le manifest Common-Controls v6
  est émis automatiquement — `cargo build` **et** `cargo test` passent
  sans précaution. C'est la seule cible utilisable pour
  `TLDL_GPU=cuda`.
- **MinGW/WinLibs** (`x86_64-pc-windows-gnu`) : `build.rs` détecte la
  cible GNU — CMake via **Ninja**, ggml **sans OpenMP**
  (`GGML_OPENMP=OFF`), copies `lib`-préfixées des archives
  (`libggml.a`, exigées par `ld` GNU). Le linker GNU ne fusionne pas le
  manifest Common-Controls v6 requis par `rfd`/`TaskDialogIndirect` :
  l'app se charge (le v6 y figure, vérifié) mais le **binaire de test**
  `cargo test` meurt au chargement avec **0xC0000139** — d'où le
  binaire `tldl-e2e` (§10.2).

### 10.2 E2E hors de l'app — binaire `tldl-e2e`

Le binaire de test étant inutilisable sur la cible GNU (0xC0000139), la
validation E2E passe par un binaire dédié `tldl-e2e` (`src/tldl_e2e.rs`) :
il ne lie que `tldl_lib::pipeline` (ni le shell Tauri ni `rfd`) et se
charge sans manifest v6.

```
tldl-e2e <fichier_média> [dossier_ffmpeg]     # pipeline complet (F3)
TLDL_E2E_LLM=<id>                            # modèle LLM (sinon qwen25-7b-q4)
tldl-e2e download <id> <url>                 # F11 : moteur models::download
tldl-e2e delete <id>                         # F11 : moteur models::delete
tldl-e2e history                             # F7 : storage save/list/get/search
tldl-e2e devices                             # F10 : devices ggml + gpu::resolve
tldl-e2e section [fichier_média]             # F14 : coupe [20 %, 80 %]
TLDL_E2E_ASR=<id>                           # modèle ASR du mode section (sinon small)
tldl-e2e llm                                 # carry LLM : passe réelle « 10 phrases »
tldl-e2e resummarize                         # F16 : re-résumé (LLM seul)
tldl-e2e folders                             # F19 : storage dossiers
tldl-e2e contexts                            # F15 : storage contextes
```

Le `dossier_ffmpeg` est optionnel : `media.rs` s'auto-localise
(`TLDL_FFMPEG_DIR` → bundle `<exe>/_up_/assets/ffmpeg/` → remontée du
repo `assets/ffmpeg/` → PATH). `TLDL_E2E_LLM` est l'identifiant du GGUF
(`models/llm-<id>.gguf`).

Modes :

- **pipeline** : sonde → WAV 16 kHz → transcription → résumé (LLM réel si
  GGUF posé ; sinon dégradation gracie — transcription conservée +
  bannière « modèle LLM absent »).
- **download** : vrai moteur `models::download` (HTTP local
  `HttpListener` ou HTTPS réel) ; assertion `taille disque =
  progression = Content-Length`, pas de résidu `.part` en cas d'échec.
- **delete** : vrai moteur `models::delete` sur un fichier factice
  (aucun modèle réel touché).
- **history** : va-et-retour factice `storage` dans un dossier
  temporaire (jamais l'historique réel).
- **section** : deux passes sur `jfk.wav` (fichier complet + section
  [20 %, 80 %]) — bornes reportées, segments dans la section, texte plus
  court que le complet.
- **llm** : passe réelle du LLM — vraie génération sur le vrai modèle
  (défaut `qwen25-7b-q4`) avec le contexte « Résume en 10 phrases
  maximum. », transcription **longue** (≈ 700 tokens, au-delà de
  l'ancien `n_batch` 512) → vérifie : **EOS naturel** (arrêt avant
  `max_tokens`), **sans méta-préambule** (« voici le résumé… »), **au
  plus 10 phrases** (heuristique sur la ponctuation terminale). Modèle
  absent → ignorée (exit 0), comme les autres modes.
- **resummarize / folders / contexts** : moteurs `re_summary` / dossiers
  / contextes sur des données factices.

> Outil annexe `tldl-manifest` (`src/manifest_tool.rs`) :
> diagnostic/patch du manifest PE d'un binaire Windows (vérifie le v6,
> `dump` de l'arborescence `.rsrc`). Hors du chemin critique — ne pas
> l'invoquer sur un binaire fonctionnel (le mode `patch` a démontré ses
> limites sur la cible GNU).

### 10.3 macOS — Apple Silicon (`aarch64-apple-darwin`)

La build macOS se fait **sur** un Mac (C2 — pas de croisé depuis
Windows ; le DMG/ZIP est assemblé sur le poste de build). Prérequis :
Xcode Command Line Tools (`xcode-select --install`) et CMake
(`brew install cmake`).

Mécanique (`build.rs` + `tldl_llm_bin.rs`, cf. §5.2) :

- `Toolchain::Apple` : `env::consts::OS == "macos"` + `/usr/bin/c++`
  présent ; le séparateur de `PATH` est `:` (fn `path_sep`, utilisé par
  `find_cmake` / `gnu_candidates` / `find_toolchain`).
- Pas de Ninja (générateur par défaut : Unix Makefiles), pas de
  préfixe `lib` à fabriquer (les archives CMake Apple sortent déjà
  `lib*.a`), pas d'ancrage CUDA (no-op hors Windows).
- **Runtime C++ libc++** : mesure double — le build script émet
  `cargo:rustc-link-lib=dylib=c++` (l'app) et `tldl_llm_bin.rs`
  épingle `#[cfg(target_os = "macos")] #[link(name = "c++")]`
  (le binaire `tldl-llm` — l'émission du build script n'atteint pas sa
  ligne de lien) ; un `-lc++` en double est inoffensif.
- **GPU (F10)** : `TLDL_GPU=metal` refuse un hôte non-macOS (gate dans
  `build.rs`) ; sur le Mac, `GGML_METAL=ON` des deux ggml + cfg
  `tldl_gpu_metal` → `#[link]` `ggml-metal` (app, `asr/mod.rs`) et
  `tldl_ggml-metal` (`tldl-llm`) ; `gpu.rs` détecte le Metal à
  l'exécution et bascule CPU si absent.
