# Fonctionnalités

**MVP = F1 → F6 + F11** — fait. Post-MVP : F7 → F19.

Statuts : ✅ fait · 🔧 en cours · 📋 à faire · ⏳ à valider · 💡 idée

## MVP

### F1 — Import d'un fichier audio ou vidéo ✅

- Glisser-déposer + sélecteur de fichier.
- Audio : wav, mp3, m4a, flac, ogg, opus — Vidéo : mp4, mov, mkv, webm,
  avi.
- **Validation à l'import** : l'extension est vérifiée dès le
  drop/sélecteur — format non pris en charge → message clair (« Format
  non pris en charge (md). Formats acceptés : … ») + bouton « Lancer la
  synthèse » désactivé, pas de `ffprobe` sur un non-média.
- Un fichier à la fois (file d'attente = 💡 post-MVP).
- **Glisser-déposer** : événement natif Tauri 2 `onDragDropEvent`
  (chemins absolus, `dragDropEnabled: true`) — dans le webview Tauri 2,
  les `drop` DOM ne se déclenchent pas (interception native) et
  `File.path` n'existe pas ; le sélecteur passe par le plugin `dialog`.
- **Coexistence F1/F19** : `dragDropEnabled: true` est requis pour ce
  dépôt OS ; en contrepartie, le drag & drop **interne** (F19) ne peut
  pas être HTML5 (la cible native le dispute) — il est piloté par
  **pointeur** (cf. F19).
- **Validé dans l'app** : drop/sélecteur + 11 extensions au quotidien,
  sonde (durée + format) OK, **fichier ≥ 2 Go** OK, format inconnu (`.md`)
  → message clair sans plantage.

### F2 — Conversion vidéo → audio (dans l'app) ✅

- FFmpeg embarqué → WAV PCM 16 kHz mono (format natif de whisper.cpp).
- Fichier temporaire auto-éffacé après traitement.
- Progression affichée (position extraite / durée totale).
- **Critères** : aucun prérequis externe ; conversion annulable ; zéro
  résidu dans les dossiers temp après coupure.

### F3 — Transcription locale 🔧

- whisper.cpp (GGUF) ; langue auto (biais FR) ou forcée.
- Segments + horodatages (au mot si dispo).
- Annulable, temps restant estimé.
- **Critères** : aucun byte ne quitte la machine ; FR correct sur 3
  profils de débit/bruit (à mesurer — F12).
- **Bench F12 (CPU + GPU, 21/11/2026)** : ASR medium ≈ **2,5× temps réel**
  CPU (RTF 0,39), **≈ 55–70× temps réel** GPU (RTF 0,01–0,02 — models.md
  §6) ; la **qualité FR sur 3 profils de débit/bruit** reste à valider
  (l'audio de bench est EN — JFK).

### F4 — Résumé local (cœur de l'app) ✅

- LLM 3B–8B quantifié Q4_K_M ; prompt « rencontre » (architecture §5.1).
- Moteur : llama.cpp b10884 dans le binaire dédié `tldl-llm` (§5.2) —
  résumé Markdown (Synthèse / Points clés / Décisions / Actions à
  venir).
- Régénérable (bouton « recommencer ») ; température réglable.
- **Format de dialogue** (fix carry LLM) : le prompt est formaté comme
  message `user` dans le template d'entraînement du modèle, **avant
  tokenisation** (shim C — architecture §5.2) : le modèle *répond* au
  lieu de compléter — pas de méta-préambule, l'EOS sort naturellement,
  la contrainte « 10 phrases maximum » tient. Passe réelle E2E
  `tldl-e2e llm` (vrai modèle, CPU, transcription **longue** ≈ 700
  tokens) : EOS naturel à 190/1024 tokens, 10 phrases, sans
  méta-préambule.
- **Prompts longs** : `n_batch` posé à `n_ctx` (§5.2) — validé dans
  l'app (résumé CPU d'un fichier de 15 min).
- **Critères** : hors ligne ; < 60 s pour 10 min d'audio sur profil
  « Équilibré » (GPU si dispo).
- **Bench F12 (CPU + GPU, 21/11/2026 — models.md §6)** : 10 min d'audio
  traitée en **12,7 s sur GPU** (RTX 4070 Ti SUPER — ASR 9,2 s + LLM
  3,5 s) et **246,5 s sur CPU** (≈ 2,4× temps réel) — la cible 60 s
  (10× temps réel) est **atteinte sur GPU** (4,7× de marge), non
  atteinte sur CPU. Gain CPU→GPU total : 14,9× / 19,4× / 24,1×
  (5 / 10 / 30 min).

### F5 — Double-vue : Transcription + Résumé ✅

- Deux onglets/panneaux, **les deux toujours accessibles** (C6).
- Transcription : segments horodatés ; clic sur un segment = saut
  (lecteur audio = F13).
- Résumé : rendu Markdown, bouton copier (export = F8).
- **Critères** : rechargement après redémarrage de l'app (via F7).

### F6 — Page « Configuration » ✅

- ASR : modèle, quantification, device CPU/GPU.
- LLM : modèle, quantification, température, contexte.
- Téléchargements opt-in (taille affichée avant, progression,
  annulable).
- Suppression d'un modèle local (« Supprimer » — le fichier quitte
  `models/`).
- Estimation RAM/VRAM par configuration + test de charge.
- Profils : Léger / Équilibré / Puissant (cf. models.md).
- **Critères** : changement de modèle sans redémarrer l'app ;
  estimation conforme à la mesure (±20 %).

### F11 — Récupération des modèles (opt-in) ✅

- LLM : téléchargement GGUF intégré (catalogue de l'app) — ou dépôt
  manuel.
- ASR : GGUF whisper.cpp (Hugging Face), une seule fois.
- Mécanique : source HF par modèle (models.md §5) ; écriture `.part` +
  renommage atomique (jamais un modèle à moitié) ; **taille affichée
  avant**, progression reçu/total, **annulable**, 1 téléchargement à
  la fois.
- **Suppression** : bouton « Supprimer » sur un modèle local — le
  fichier quitte `models/`, le modèle redevient « à télécharger ».
- **Hors ligne** : dépôt manuel du fichier dans `models/` (supporté +
  documenté).
- **Critères** : après la 1ʳᵉ mise en place, l'app est 100 % utilisable
  hors ligne.
- **Statut** : code en place (backend `reqwest` blocking + rustls, UI
  avec barre + Annuler) ; **moteur validé E2E** (`tldl-e2e download` :
  HTTP local + HTTPS réel ; `tldl-e2e delete`) ; **téléchargement HF réel
  confirmé dans l'app** (Gemma 3 4B 2,5 Go, whisper-medium 1,5 Go).
## Post-MVP

### F7 — Historique local (SQLite) ✅

Liste des traitements (fichier, date, durée, modèles utilisés), recherche
plein texte, réouverture d'un résultat, suppression d'une entrée.

- **Groupé par fichier** (UI) : même transcription = un groupe par
  fichier, lignes = variantes de résumé (contexte + date — F15/F16).
- **Onglet « Historique »** : liste **groupée par fichier** (lignes =
  variantes de résumé : contexte + date + LLM + device, extrait du
  résumé, suppression par ligne), recherche plein texte (fichier /
  transcription / résumé, debounce 300 ms, groupée), réouverture en
  double-vue (F5), **suppression d'une entrée** (bouton « Supprimer » +
  boîte de confirmation in-app — le `confirm` natif n'est pas fiable
  sous le webview Tauri 2 ; commande `delete_history` →
  `storage::delete_record`).
- Migration SQLite idempotente (`transcription_json`) ; E2E
  `tldl-e2e history` (va-et-retour factice dans un dossier temporaire,
  jamais l'historique réel).

### F8 — Export 📋

Transcription : txt / md / srt (horodatée). Résumé : md / txt.
Package JSON complet (traçabilité : modèles, versions, date).

### F9 — Séparation des intervenants (diarisation) 💡

Plus lourde (pyannote / 3D-Speaker) ; à évaluer après le MVP.

### F10 — Accélération GPU ✅

CUDA / Metal / Vulkan (whisper.cpp + LLM), détection auto, bascule CPU
si absent (C3 « GPU optionnel, CPU viable »).

- **Build** : la variable `TLDL_GPU` (`cpu` défaut | `cuda` | `metal` |
  `vulkan`) pilote dans `build.rs` les options CMake `GGML_CUDA` /
  `GGML_METAL` / `GGML_VULKAN` des **deux** ggml (whisper.cpp et
  llama.cpp) et les cfg `tldl_gpu_*` qui conditionnent les `#[link]`
  des archives GPU (app/E2E : `ggml-cuda`… ; `tldl-llm` :
  `tldl_ggml-cuda`…). Sous Windows, `cuda` exige en plus la cible
  **MSVC** et `CUDA_PATH` (nvcc : hôte MSVC uniquement — cf.
  architecture.md §10.1).
- **Détection** : `src/gpu.rs` interroge le registre de devices du
  ggml de whisper.cpp (lié dans l'app) — le device CPU est toujours
  présent, un GPU/iGPU n'apparaît que si la build a activé le backend
  et que la machine en possède un. Nom affiché/consigné :
  `ggml_backend_dev_description` (le **vrai nom** — processeur pour le
  CPU, ex. « Intel(R) Core(TM) i9-14900K » ; GPU pour un CUDA, ex. «
  NVIDIA GeForce RTX 4070 Ti SUPER »), repli sur
  `ggml_backend_dev_name` (le slug « CPU » / « CUDA0 ») si vide.
- **Bascule** : `gpu::resolve(use_gpu)` → premier GPU/iGPU détecté si
  demandé, sinon CPU + note (`DeviceResolution { name, kind, use_gpu,
  n_gpu_layers, fallback }`) ; `n_gpu_layers` (-1 = GPU toutes les
  couches, 0 = CPU) transmis au binaire `tldl-llm` (5ᵉ argument,
  défaut 0).
- **UI** : section « Accélération » de la page Réglages (toggle GPU +
  devices détectés + alerte si GPU demandé mais absent) ; le device
  utilisé est affiché sur le résultat et consigné dans l'historique
  (`records.device`, migration idempotente).
- **Statut** : code en place (build.rs + `gpu.rs` + `n_gpu_layers` LLM +
  UI + E2E `tldl-e2e devices`) ; runtime validé sur la machine (RTX 4070
  Ti SUPER, compute 8.9, 16 Go — build CUDA) : `tldl-e2e devices` → GPU
  détecté, `resolve(true)` → GPU sans bascule, `tldl-llm`
  `n_gpu_layers = -1` → 29/29 couches offloadées ; **validé dans
  l'app** : Réglages → « Accélération » — la 4070 listée sous son
  **vrai nom**, le CPU sous son vrai processeur, toggle ON → pipeline
  sur le GPU, device affiché et consigné (confirmé par l'utilisateur).

### F12 — Benchmark & cibles perf ✅

Matrice modèle × device × durée (5 / 10 / 30 min) ; les résultats
mesurés alimentent models.md (remplacent les estimations).

- **Mode E2E** `tldl-e2e bench` : audios JFK bouclés 300 / 600 / 1800 s
  (FFmpeg embarqué, générés si absents) ; ASR seul (pipeline, LLM
  absent) chronométré sur l'étape `asr` (RTF) ; LLM = `re_summary` sur
  la **vraie transcription** (étape LLM seule, chargement inclus) —
  « chargement+prompt » jusqu'au 1er token, tok/s sur la génération.
- **Mesuré (CPU + GPU, 21/11/2026 — models.md §6)** : ASR medium + LLM
  qwen25-7b-q4 (défauts « Équilibré ») × {5, 10, 30 min} — **CPU**
  (i5-14600KF) + **GPU** (RTX 4070 Ti SUPER, build CUDA `TLDL_GPU=cuda`)
  ; run propre (Qwen arrêté) relancé par script utilisateur — CPU puis
  GPU, un rapport par device.
- **Non mesuré** : les autres modèles (absents — pas de téléchargement
  surprise, F11 opt-in).

### F13 — Lecteur audio intégré ✅

Lecteur audio intégré à la page Résultat, synchronisé aux segments.

- **Lecture de la source** : le lecteur lit le fichier *source*
  (l'intermédiaire WAV étant supprimé après le pipeline) via le
  **asset protocol** Tauri — `convertFileSrc` → `asset://`,
  `app.security.assetProtocol {enable:true, scope:["**"]}` dans
  `tauri.conf.json`, feature `protocol-asset` (Cargo.toml).
- **Champ `file`** : `PipelineOutput.file: String` (serde camelCase)
  porte le chemin du fichier source, posé par `pipeline.rs`.
- **Sync segments** : `ResultPage.tsx` — `<audio controls>` +
  `onTimeUpdate` → `activeIndex` ; clic sur un segment = `seekTo` +
  play.
- **SegmentList** : segments cliquables + surlignage du segment actif.
- **Statut** : code en place (Rust `file` + asset-protocol +
  player/sync frontend) ; frontend `tsc`+`vite` vert ; validé dans
  l'app (lecteur du source synchronisé aux segments — confirmé par
  l'utilisateur).
### F14 — Travail par section ✅

À l'import, travailler soit sur le fichier complet, soit sur une
section (pré-écoute + coupe du début et de la fin par sliders) — la
coupe est **optionnelle** (défaut = fichier complet).

- **UI à l'import** : après sonde, carte « Pré-écoute » — lecteur du
  média source (protocole asset, cf. F13 — `convertFileSrc`) +
  **2 sliders** (Début / Fin, pas 0,1 s, bornés l'un à l'autre :
  début ≤ fin), affichage de la section (`mm:ss → mm:ss` + durée
  traitée) + bouton « Tout le fichier » (repli défaut).
- **Traitement (pas seulement l'affichage)** : FFmpeg ne convertit que
  `[début, fin]` (`-ss début` + `-t fin-début`) en WAV 16 kHz mono —
  ASR et LLM ne voient que la section.
- **Horodatages** : les segments, émis relatifs à la section (0-based),
  sont **décalés de `début`** par le pipeline pour rester alignés sur le
  fichier source — le lecteur (F13) saute au bon endroit sans
  modification.
- **Consignation** : `records.section_start` / `section_end` (SQLite,
  migration idempotente ; lignes pré-migration = `0` / `duration_secs`
  = fichier complet), `PipelineOutput.sectionStart` / `sectionEnd`
  (camelCase) ; le re-résumé (F16) **préserve** la section d'origine ;
  pied de page Résultat : « section mm:ss → mm:ss » ou « fichier
  complet ».
- **Backend** : `run_pipeline` prend `section_start` / `section_end`
  (`Option<f64>`), bornés à la durée sonde (`effective_section`) —
  sentinelle `end = 0` = « jusqu'à la fin », section vide (fin ≤
  début) = erreur claire.
- **Statut** : code en place (Rust + frontend + E2E `tldl-e2e section`
  vert : section [20 %, 80 %] sur `jfk.wav` — bornes reportées,
  segments dans la section, texte plus court que le complet) ;
  **validé dans l'app** : pré-écoute + sliders → coupe — la
  transcription ne porte que sur la section (l'ASR — donc le LLM — ne
  voit que ce qui a été converti dans `[début, fin]`).

### F15 — Contextes de prompt ✅

L'utilisateur choisit ou crée un contexte de résumé avant de demander
une transcription ; favoris pour la réutilisation rapide ; page dédiée
à la création / gestion.

- **Choix à l'import** : sélecteur « Contexte de résumé » (valeur par
  défaut = prompt « rencontre » de l'architecture §5.1) + création
  rapide (nom + instructions) sans quitter la page.
- **Page « Contextes »** : création, modification, suppression
  (confirmée), favoris ★ épinglés en tête de liste.
- **Injection** : le contexte **remplace le format par défaut**
  (Synthèse / Points clés / Décisions / Actions) dans le prompt LLM —
  cf. architecture.md §5.1 ; le binaire `tldl-llm` n'est pas modifié
  (il lit le prompt complet depuis son fichier).
- **Portée** : le contexte n'agit que sur le **résumé** (étape LLM) —
  la transcription reste fidèle à l'audio ; rappelé dans le code
  (commentaires `commands` / `pipeline` / `llm::build_prompt`).
- **Traçabilité** : le nom du contexte est consigné dans l'historique
  (`records.context_name`), affiché sur la ligne d'historique, dans la
  liste « Résumés de ce fichier » de la page Résultat et son pied de
  page.
- **Stockage** : table SQLite `contexts` (même base que l'historique —
  F7).
- **Statut** : code en place (storage + prompt + UI + E2E
  `tldl-e2e contexts`) ; validé dans l'app (sélecteur à l'import + page
  Contextes — confirmé par l'utilisateur).

### F16 — Re-résumer sans re-transcrire ✅

Si un fichier a déjà été analysé, l'utilisateur est averti au choix du
chemin : il peut **re-résumer sans re-transcrire**, forcer une nouvelle
analyse, ou être redirigé vers le résultat existant (plusieurs résumés
peuvent coexister pour un même fichier — transcription unique,
résumés selon les contextes).

- **Commande** : `history_for_file` (Rust) → `storage::find_by_file`
  (`WHERE file=?1 ORDER BY id DESC`, le plus récent d'abord).
- **Flux ImportPage** : au choix du chemin → `historyForFile` ; si
  trouvé → carte « **Résumer à nouveau sans re-transcrire** » /
  « Ouvrir le résultat existant » (redirection `toOutput`) / « Forcer
  une nouvelle analyse » (pipeline complet, re-transcription incluse) ;
  bouton « Lancer la synthèse » désactivé tant que l'utilisateur n'a
  pas tranché.
- **Re-résumé sans re-transcrire** : cette option relance **seule
  l'étape LLM** sur la transcription stockée — commande
  `re_summarize` (Rust) : relit `records.transcription_json` de
  l'enregistrement le plus récent (repli texte seul pour les lignes
  pré-migration), `pipeline::re_summary` (pas de sonde, pas de
  conversion, pas d'ASR) avec le **contexte sélectionné**, et consigne
  un **nouvel** enregistrement (même transcription, nouveau résumé) ;
  en cas d'erreur LLM, aucun enregistrement n'est créé (l'ancien reste
  intact). Modèle/device : config courante (F10). Progression
  réutilisée (`pipeline-progress`, étapes `llm` → `done`).
- **Re-résumé depuis le résultat** : même action sur la page Résultat —
  sélecteur de contexte + bouton « Relancer le résumé » (LLM seul) ;
  l'enregistrement ouvert est identifié par son id (traçé dans
  `App`) ; le résultat affiché est remplacé par le nouvel
  enregistrement (transcription et lecteur conservés).
- **Résumés par contexte (page Résultat)** : tous les résumés d'un même
  fichier sont conservés — chaque re-résumé en consigne un **nouvel**
  enregistrement ; la page les liste (« Résumés de ce fichier » :
  contexte, date, LLM, device — le résultat affiché est repéré
  `current`, clic = l'ouvrir sans recalcul via `get_history`), bouton
  « Supprimer » par ligne (confirmation in-app, `delete_history`) ;
  le pied de page indique le contexte + la date du résumé affiché.
- **`toOutput` partagé** : `toOutput(r: HistoryRecord): PipelineOutput`
  dans `src/api/tauri.ts` (réutilisé par l'historique F7 + la
  redirection F16), inclut `file: r.file`.
- **Statut** : code en place (commande + test `find_by_file` + UI ;
  re-résumé : `re_summarize` + `pipeline::re_summary` + test de
  relecture `transcription_json` + E2E `tldl-e2e resummarize` — LLM
  réel, GPU) ; validé dans l'app (confirmé par l'utilisateur).

### F17 — Mode avancé : connecter des modèles via URL/IP 📋

Pointer un provider IA local ou web (à ses risques).

### F18 — Télécharger un modèle hors catalogue via une URL 📋

Dans le mode avancé : l'utilisateur fournit l'URL d'un modèle autre
que ceux conseillés.

### F19 — Ranger les résultats dans des dossiers ✅

Dossiers créables par l'utilisateur pour trier les résultats ;
**drag & drop** d'un résultat (ligne d'historique) vers un dossier, et
d'un dossier vers un autre.

- **Stockage (SQLite)** : table `folders` (nom, horodatage) + colonne
  `records.folder_id` — **au plus un** dossier par enregistrement,
  `NULL` = « Sans dossier » (état par défaut) ; migration idempotente à
  l'ouverture ; **supprimer un dossier ne supprime aucun résultat**
  (ses enregistrements repassent « Sans dossier ») ; le re-résumé
  (F16) range le nouveau résumé dans le dossier d'origine.
- **Commandes** : `create_folder`, `list_folders` (+ compte
  d'enregistrements par dossier), `delete_folder`, `move_record`
  (enregistrement → dossier, ou « Sans dossier » — erreur si
  l'enregistrement ou le dossier est introuvable),
  `move_file_to_folder` (rangement de **groupe** : tous les
  enregistrements d'un même fichier — même transcription — d'un seul
  `UPDATE`, **atomique** : le groupe n'est jamais partagé entre deux
  dossiers ; erreur si le fichier ou le dossier est introuvable),
  `list_history_folder` (vue d'un dossier, plus récents en premier).
- **UI (historique)** : panneau latéral « Dossiers » — « Sans
  dossier » + un item par dossier (compte de lignes, « Suppr. » avec
  confirmation in-app — ses résultats repassent « Sans dossier »),
  création rapide (le nouveau dossier s'ouvre) ; la liste affiche le
  contenu du dossier ouvert (défaut : « Sans dossier ») ; les lignes
  sont **glissables** (drag & drop par **pointeur** —
  `pointerdown/pointermove/pointerup` + hit-test `elementFromPoint` +
  ghost ; **pas** de drag & drop HTML5, disputé par la cible native de
  wry dès que `dragDropEnabled: true`, cf. F1 — les deux UX
  coexistent) vers un dossier ou « Sans dossier » (cible surlignée au
  survol) ; sélecteur « ranger dans un dossier » par ligne
  (alternative au glisser-déposer) ; l'en-tête d'un groupe (tous les
  résumés d'une même transcription) est glissable lui aussi — le
  groupe se déplace entier, jamais à moitié — + sélecteur « ranger le
  groupe » (si le groupe est partagé entre dossiers, l'état « Groupe
  partagé… » s'affiche jusqu'au prochain rangement) ; la recherche
  plein texte (F7) reste **globale** (tous dossiers).
- **UI (résultat)** : sélecteur « Rangement » du résultat affiché (même
  commande `move_record` ; l'enregistrement est identifié par son id
  traçé dans `App`).
- **Statut** : code en place (storage + test `roundtrip_folders` +
  commandes + UI + E2E `tldl-e2e folders`) ; frontend vert ; validé
  dans l'app (confirmé par l'utilisateur).
