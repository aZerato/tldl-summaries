# Fonctionnalités

**MVP = F1 → F6 + F11.** Scaffold posé le 14/11/2026 : les features MVP sont
🔧 **en cours** — UI + noyau en place, ASR branché sur whisper.cpp (v1.9.5
vendue, build CMake + shim C ; **validé E2E** : transcription `jfk.wav`),
LLM **réel** via binaire dédié `tldl-llm` (F4, résumé Markdown généré),
FFmpeg **auto-localisé** (bundle + repo, sans `TLDL_FFMPEG_DIR`),
téléchargement de modèles (F11) **branché** (Hugging Face ; moteur validé
E2E HTTP/HTTPS).

Statuts : ✅ fait · 🔧 en cours · 📋 à faire · ⏳ à valider · 💡 idée

## MVP

### F1 — Import d'un fichier audio ou vidéo 📋
- Glisser-déposer + sélecteur de fichier
- Audio : wav, mp3, m4a, flac, ogg, opus — Vidéo : mp4, mov, mkv, webm, avi
- Validation à l'import (format inconnu → message clair, pas de plant)
- Un fichier à la fois (file d'attente = 💡 post-MVP)
- **Glisser-déposer** : événement natif Tauri 2 `onDragDropEvent` (chemins
  absolus, `dragDropEnabled: true`) — dans le webview Tauri 2, les `drop`
  DOM ne se déclenchent pas (interception native) et `File.path` n'existe
  pas ; le sélecteur passe par le plugin `dialog`
- **Coexistence F1/F19** : `dragDropEnabled: true` est requis pour ce
  dépôt OS ; en contrepartie le drag & drop **interne** (F19, rangement
  dans les dossiers) ne peut pas être HTML5 (la cible native le dispute)
  — il est donc piloté par **pointeur** (cf. F19)
- **Critères** : les 11 extensions ci-dessus acceptées sans crash ;
  durée + format affichés après sonde ; fichiers ≥ 2 Go gérés

### F2 — Conversion vidéo → audio (dans l'app) 📋
- FFmpeg embarqué → WAV PCM 16 kHz mono (format natif de whisper.cpp)
- Fichier temporaire auto-effacé après traitement
- Progression affichée (position extraite / durée totale)
- **Critères** : aucun prérequis externe côté utilisateur ; conversion
  annulable ; zéro résidu dans les dossiers temp après coupure

### F3 — Transcription locale 🔧
- whisper.cpp (GGUF) ; langue auto (biais FR) ou forcée
- Segments + horodatages (au mot si dispo)
- Annulable, temps restant estimé
- **Critères** : aucun byte ne quitte la machine ; FR correct sur 3 profils
  de débit/bruit (à mesurer — F12)

### F4 — Résumé local (cœur de l'app) 🔧
- LLM 3B–8B quantifié Q4_K_M ; prompt « rencontre » (architecture §5.1)
- Moteur : llama.cpp b10884 dans binaire dédié `tldl-llm` (§5.2) — **résumé
  Markdown généré** (Synthèse / Points clés / Décisions / Actions à venir)
- Régénérable (bouton « recommencer ») ; température réglable
- **Critères** : hors ligne ; < 60 s pour 10 min d'audio sur profil
  « Équilibré » (benchmark F12 pour arrêter)

### F5 — Double-vue : Transcription + Résumé 📋
- Deux onglets/panneaux, **les deux toujours accessibles** (C6)
- Transcription : segments horodatés ; clic sur un segment = saut
  (lecteur audio = F13)
- Résumé : rendu Markdown, bouton copier (export = F8)
- **Critères** : rechargement après redémarrage de l'app (via F7)

### F6 — Menu « Modèles » (configuration) 📋
- ASR : modèle, quantification, device CPU/GPU
- LLM : modèle, quantification, température, contexte
- Téléchargements opt-in (taille affichée avant, progression, annulable)
- Suppression d'un modèle local (« Supprimer » — le fichier quitte `models/`)
- Estimation RAM/VRAM par configuration + test de charge
- Profils : Léger / Équilibré / Puissant (cf. models.md)
- **Critères** : changement de modèle sans redémarrer l'app ;
  estimation conforme à la mesure (±20 %)

### F11 — Récupération des modèles (opt-in) ⏳
- LLM : téléchargement GGUF intégré (catalogue de l'app) — ou dépôt manuel
- ASR : GGUF whisper.cpp (Hugging Face), une seule fois
- Mécanique : source HF par modèle (models.md §5) ; écriture `.part` +
  renommage atomique (jamais un modèle à moitié) ; **taille affichée avant**,
  progression reçu/total, **annulable**, 1 téléchargement à la fois
- **Suppression** : bouton « Supprimer » sur un modèle local (fichier retiré
  de `models/`, le modèle redevient « à télécharger »)
- **Hors ligne** : dépôt manuel du fichier dans `models/` (supporté + documenté)
- **Critères** : après la 1ʳᵉ mise en place, l'app est 100 % utilisable hors ligne
- **Statut** : code en place (backend `reqwest` blocking + rustls, UI avec
  barre + Annuler) ; **moteur validé E2E** (`tldl-e2e download` : HTTP local +
  HTTPS réel ; `tldl-e2e delete`) ; **téléchargement HF réel confirmé dans
  l'app** (Gemma 3 4B 2,5 Go, whisper-medium 1,5 Go)

## Post-MVP

### F7 — Historique local (SQLite) 📋
Liste des traitements (fichier, date, durée, modèles utilisés), recherche
plein texte, réouverture d'un résultat, suppression d'une entrée.
**Groupé par fichier** (UI) : même transcription = un groupe par
fichier, lignes = variantes de résumé (contexte + date — F15/F16).
- **Statut** : code en place — consignation **auto après chaque pipeline
  réussi** (texte + segments horodatés en JSON, résumé) ; onglet
  « Historique » : liste **groupée par fichier** (un groupe par fichier —
  même transcription ; lignes = variantes de résumé : contexte + date +
  LLM + device, extrait du résumé, suppression par ligne),
  recherche plein texte (fichier / transcription / résumé, debounce
  300 ms, groupée aussi),
  réouverture en double-vue (F5), **suppression d'une entrée** (bouton
  « Supprimer » + confirmation native `@tauri-apps/plugin-dialog` —
  `window.confirm` n'est pas fiable sous le webview Tauri 2 ; commande
  `delete_history` → `storage::delete_record`) ; migration SQLite idempotente
  (`transcription_json`) ; E2E `tldl-e2e history` (va-et-retour factice dans
  un dossier temporaire, jamais l'historique réel)

### F8 — Export 📋
Transcription : txt / md / srt (horodatée). Résumé : md / txt.
Package JSON complet (traçabilité : modèles, versions, date).

### F9 — Séparation des intervenants (diarisation) 💡
Plus lourde (pyannote / 3D-Speaker) ; à évaluer après le MVP.

### F10 — Accélération GPU ⏳
CUDA / Metal / Vulkan (whisper.cpp + LLM), détection auto,
bascule CPU si absent (C3 « GPU optionnel, CPU viable »).
- **Build** : la variable `TLDL_GPU` (`cpu` défaut | `cuda` | `metal` |
  `vulkan`) pilote dans `build.rs` les options CMake `GGML_CUDA` /
  `GGML_METAL` / `GGML_VULKAN` des **deux** ggml (whisper.cpp et
  llama.cpp) et les cfg `tldl_gpu_*` qui conditionnent les `#[link]` des
  archives GPU (app/E2E : `ggml-cuda`… ; `tldl-llm` : `tldl_ggml-cuda`…).
  Sous Windows, `cuda` exige en plus la cible **MSVC** et `CUDA_PATH`
  (nvcc : hôte MSVC uniquement — cf. architecture.md §10.1).
- **Détection** : `src/gpu.rs` interroge le registre de devices du ggml de
  whisper.cpp (lié dans l'app) — le device CPU est toujours présent, un
  GPU/iGPU n'apparaît que si la build a activé le backend et que la
  machine en possède un
- **Bascule** : `gpu::resolve(use_gpu)` → premier GPU/iGPU détecté si
  demandé, sinon CPU + note (`DeviceResolution { name, kind, use_gpu,
  n_gpu_layers, fallback }`) ; le `n_gpu_layers` (-1 = GPU toutes les
  couches, 0 = CPU) est transmis au binaire `tldl-llm` (5ᵉ argument,
  défaut 0)
- **UI** : section « Accélération » de la page Réglages (toggle GPU +
  devices détectés + alerte si GPU demandé mais absent) ; le device utilisé
  est affiché sur le résultat et consigné dans l'historique (colonne
  `records.device`, migration idempotente)
- **Statut** : code en place (build.rs + `gpu.rs` + `n_gpu_layers` LLM +
  UI + E2E `tldl-e2e devices`) ; **runtime validé sur la machine** (RTX
  4070 Ti SUPER, compute 8.9, 16 Go — build CUDA) : `tldl-e2e devices` →
  GPU détecté (`CUDA0`), `resolve(true)` → GPU sans bascule CPU ;
  `tldl-llm` avec `n_gpu_layers = -1` → 29/29 couches offloadées sur le
  GPU (modèle 4,1 Go + KV + compute sur `CUDA0`, génération CUDA Graph) ;
  reste la validation dans l'app (Réglages → « Accélération » : la 4070
  est listée, toggle ON → pipeline sur le GPU, device affiché sur le
  résultat et consigné `records.device`)

### F12 — Benchmark & cibles perf 📋
Matrice modèle × device × durée (5 / 10 / 30 min) ; les résultats mesurés
alimentent models.md (remplacent les estimations).

### F13 — Lecteur audio intégré ⏳
Lecteur audio intégré à la page Résultat, synchronisé aux segments.
- **Lecture de la source** : le lecteur lit le fichier *source* (l'intermédiaire
  WAV est supprimé après le pipeline) via le **asset protocol** Tauri —
  `convertFileSrc` → `asset://`, `app.security.assetProtocol {enable:true,
  scope:["**"]}` dans `tauri.conf.json`, feature `protocol-asset` (Cargo.toml)
- **Champ `file`** : `PipelineOutput.file: String` (serde camelCase) porte le
  chemin du fichier source, posé par `pipeline.rs` (`file: input.to_string()`)
- **Sync segments** : `ResultPage.tsx` — `<audio controls>` +
  `onTimeUpdate` → `activeIndex` ; clic sur un segment = `seekTo` + play
- **SegmentList** : segments cliquables + surlignage du segment actif
- **Statut** : code en place (Rust `file` + asset-protocol + player/sync
  frontend) ; frontend `tsc`+`vite` vert ; à valider dans l'app

### F14 - Travail par section ⏳
À l'import, travailler soit sur le fichier complet, soit sur une section
(Pré-écoute + coupe du début et de la fin par sliders) — la coupe est
**optionnelle** (défaut = fichier complet).
- **UI à l'import** : après sonde, carte « Pré-écoute » — lecteur du média
  source (protocole asset, cf. F13 — `convertFileSrc`) + **2 sliders**
  (Début / Fin, pas 0,1 s, bornés l'un à l'autre : début ≤ fin), affichage
  de la section (`mm:ss → mm:ss` + durée traitée) + bouton « Tout le
  fichier » (repli défaut)
- **Traitement (pas seulement l'affichage)** : FFmpeg ne convertit que
  `[début, fin]` (entrée `-ss début` + `-t fin-début`) en WAV 16 kHz mono —
  ASR et LLM ne voient que la section ; le LLM ne résume que ce qui a été
  transcrit dedans
- **Horodatages** : les segments émis sont relatifs à la section (0-based,
  WAV intermédiaire) — le pipeline les **décale de `début`** pour rester
  alignés sur le fichier source : le lecteur de la page Résultat (F13, qui
  lit le source) saute au bon endroit sans modification
- **Consignation** : `records.section_start` / `section_end` (SQLite,
  migration idempotente ; lignes pré-migration = `0` / `duration_secs`
  = fichier complet), `PipelineOutput.sectionStart` / `sectionEnd`
  (camelCase) ; le re-résumé (F16) **préserve** la section de
  l'enregistrement d'origine (pas de re-tronçonnage) ; pied de page
  Résultat : « section mm:ss → mm:ss » ou « fichier complet »
- **Backend** : `run_pipeline` prend `section_start` / `section_end`
  (`Option<f64>`) ; le pipeline borne à la durée sonde
  (`effective_section`) — sentinelle `end = 0` = « jusqu'à la fin »,
  section vide (fin ≤ début) = erreur claire
- **Statut** : code en place (Rust + frontend + E2E `tldl-e2e section`
  vert : section [20 %, 80 %] sur `jfk.wav` — bornes reportées, segments
  dans la section, texte plus court que le complet) ; à valider dans
  l'app (pré-écoute + sliders à l'import)

### F15 - Contextes de prompt ⏳
L'utilisateur choisit ou crée un contexte de résumé avant de
demander une transcription ; favoris pour la réutilisation rapide ; page
dédicée à la création / gestion.
- **Choix à l'import** : sélecteur « Contexte de résumé » (valeur par défaut
  = prompt « rencontre » de l'architecture §5.1) + création rapide
  (nom + instructions) sans quitter la page
- **Page « Contextes »** : création, modification, suppression (confirmée),
  favoris ★ épinglés en tête de liste
- **Injection** : le contexte **remplace le format par défaut** (Synthèse /
  Points clés / Décisions / Actions) dans le prompt LLM — cf.
  architecture.md §5.1 ; le binaire `tldl-llm` n'est pas modifié (il lit le
  prompt complet depuis son fichier)
- **Portée** : le contexte n'agit que sur le **résumé** (étape LLM) — la
  transcription (ASR) reste fidèle à l'audio ; affiché dans l'UI (sélecteur
  « Contexte de résumé » à l'import + page Contextes) et rappelé dans le code
  (commentaires `commands` / `pipeline` / `llm::build_prompt`)
- **Traçabilité** : le nom du contexte est consigné dans l’historique
  (colonne `records.context_name`), affiché sur la ligne d’historique,
  dans la liste « Résumés de ce fichier » de la page Résultat et son pied
  de page (contexte + date du résumé affiché)
- **Stockage** : table SQLite `contexts` (même base que l'historique — F7)
- **Statut** : code en place (storage + prompt + UI + E2E `tldl-e2e
  contexts`) ; à valider dans l'app

### F16 - Eviter la transcription d'un fichier ⏳
Si un fichier a déjà été analysé, l'utilisateur est averti au choix du chemin :
il peut **re-résumer sans re-transcrire**, forcer une nouvelle analyse, ou
être redirigé vers le résultat existant (plusieurs résumés peuvent coexister
pour un même fichier — transcription unique, résumés selon les contextes).
- **Commande** : `history_for_file` (Rust) → `storage::find_by_file`
  (`WHERE file=?1 ORDER BY id DESC`, le plus récent d'abord)
- **Flux ImportPage** : au choix du chemin → `historyForFile` ; si trouvé →
  carte d'avertissement « **Résumer à nouveau sans re-transcrire** » (voir
  ci-après) / « Ouvrir le résultat existant » (redirection `toOutput`) /
  « Forcer une nouvelle analyse » (force : pipeline complet, re-transcription
  incluse) ; bouton « Lancer la synthèse » désactivé tant que
  l'utilisateur n'a pas tranché (forcer)
- **Re-résumé sans re-transcrire** : la 3ᵉ option de la carte relance
  **seule l'étape LLM** sur la transcription stockée — commande
  `re_summarize` (Rust) : relit `records.transcription_json` de
  l'enregistrement le plus récent (repli texte seul pour les lignes
  pré-migration), `pipeline::re_summary` (pas de sonde, pas de conversion,
  pas d'ASR) avec le **contexte sélectionné** à l'import, et consigne un
  **nouvel** enregistrement (même transcription, nouveau résumé) ; en cas
  d'erreur LLM, aucun enregistrement n'est créé (l'ancien reste intact).
  Modèle/device : config courante (F10). UI : bouton « Résumer à nouveau
  sans re-transcrire » + note (« le résumé seul est recalculé… ») ;
  progression réutilisée (`pipeline-progress`, étapes `llm` → `done`)
- **Re-résumé depuis le résultat** : la page Résultat propose la même
  action — sélecteur de contexte + bouton « Relancer le résumé » (LLM
  seul) + barre d’avancement réutilisée (`pipeline-progress`, étapes
  `llm` → `done`, émise par `re_summarize`) ; l’enregistrement ouvert y
  est identifié par son id (traçé dans
  `App` : historique F7, redirection F16, synthèse — le plus récent du
  fichier après `run_pipeline`), et le résultat affiché est remplacé par
  le nouvel enregistrement (transcription et lecteur conservés)
- **Résumés par contexte (page Résultat)** : tous les résumés d’un même
  fichier sont conservés — chaque re-résumé en consigne un **nouvel**
  enregistrement (`records.context_name` + horodatage) ; la page les liste
  (« Résumés de ce fichier » : contexte, date, LLM, device — le résultat
  affiché est repéré `current`, clic = l’ouvrir sans recalcul via
  `get_history`), bouton « Supprimer » par ligne (confirmation native,
  `delete_history` — si l’enregistrement supprimé est celui affiché : le
  plus récent restant est ouvert, ou le résultat est gardé en mémoire sans
  enregistrement si aucun ne reste) ; le pied de page indique le contexte +
  la date du résumé affiché
- **`toOutput` partagé** : `toOutput(r: HistoryRecord): PipelineOutput`
  déplacé dans `src/api/tauri.ts` (réutilisé par l'historique F7 + la
  redirection F16), inclut `file: r.file`
- **Statut** : code en place (commande + test `find_by_file` + UI ;
  re-résumé : `re_summarize` + `pipeline::re_summary` + test de relecture
  `transcription_json` + E2E `tldl-e2e resummarize` — LLM réel, GPU) ;
  re-résumé depuis le résultat : UI frontend (id traçé dans `App`, pas
  de commande nouvelle) ; résumés par contexte : liste + pied de page
  (commandes `get_history` / `history_for_file` existantes) ;
  frontend vert ; à valider dans l'app

### F17 - Permettre d'avoir un mode avancée pour connecter des modèles via URL/IP
Permettre à l'utilisateur de pointer sur un provider local ou web IA (mais à ses risques).

### F18 - Permettre à l'utilisateur de mettre une url pour télécharger un modèle autre que ceux conseillés
Dans le mode avancé également.

### F19 - Ranger les résultats dans des dossiers 🔧
Dossiers créables par l'utilisateur pour trier les résultats ;
**drag & drop** d'un résultat (ligne d'historique) vers un dossier, et
d'un dossier vers un autre dossier.
- **Stockage (SQLite)** : table `folders` (nom, horodatage) + colonne
  `records.folder_id` — **au plus un** dossier par enregistrement,
  `NULL` = « Sans dossier » (état par défaut) ; migration idempotente à
  l'ouverture (colonne `folder_id`) ; **supprimer un dossier ne supprime
  aucun résultat** (ses enregistrements repassent « Sans dossier ») ;
  le re-résumé (F16) range le nouveau résumé dans le dossier de
  l'enregistrement d'origine
- **Commandes** : `create_folder`, `list_folders` (+ compte
  d'enregistrements par dossier), `delete_folder`, `move_record`
  (enregistrement → dossier, ou « Sans dossier » — erreur si
  l'enregistrement ou le dossier est introuvable),
  `move_file_to_folder` (rangement de **groupe** : tous les enregistrements
  d'un même fichier — même transcription — d'un seul `UPDATE`, **atomique**
  : le groupe n'est jamais partagé entre deux dossiers ; erreur si le
  fichier ou le dossier est introuvable),
  `list_history_folder` (vue d'un dossier, plus récents en premier ;
  `NULL` = « Sans dossier »)
- **UI (historique)** : panneau latéral « Dossiers » — « Sans dossier »
  + un item par dossier (compte de lignes, « Suppr. » avec confirmation
  native — ses résultats repassent « Sans dossier »), création rapide
  (le nouveau dossier s'ouvre) ; la liste affiche le contenu du dossier
  ouvert (défaut : « Sans dossier ») ; les lignes sont **glissables**
  (drag & drop par **pointeur** — pression + déplacement + relâche,
  `pointerdown/pointermove/pointerup` + hit-test `elementFromPoint` +
  ghost ; **pas** de drag & drop HTML5, qui sous WebView2/Windows est
  disputé par la cible native de wry dès que `dragDropEnabled: true`,
  cf. F1 — les deux UX de glisser-déposer coexistent) vers un dossier
  ou « Sans dossier » (cible surlignée au survol) ; sélecteur « ranger
  dans un dossier » par ligne (alternative au glisser-déposer) ;
  **groupe** : l'en-tête d'un groupe (tous les résumés d'une même
  transcription) est glissable lui aussi — le groupe se déplace entier,
  jamais à moitié — + sélecteur « ranger le groupe » (si le groupe est partagé
  entre dossiers, l'état « Groupe partagé… » s'affiche jusqu'au prochain
  rangement) ; la recherche plein texte (F7) reste **globale** (tous
  dossiers)
- **UI (résultat)** : sélecteur « Rangement » du résultat affiché (même
  commande `move_record` ; l'enregistrement est identifié par son id
  traçé dans `App`)
- **Statut** : code en place (storage + test `roundtrip_folders` +
  commandes + UI + E2E `tldl-e2e folders`) ; frontend vert ; à valider
  dans l'app
