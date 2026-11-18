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
plein texte, réouverture d'un résultat.
- **Statut** : code en place — consignation **auto après chaque pipeline
  réussi** (texte + segments horodatés en JSON, résumé) ; onglet
  « Historique » : liste (fichier, date, durée, modèles, extrait),
  recherche plein texte (fichier / transcription / résumé, debounce 300 ms),
  réouverture en double-vue (F5) ; migration SQLite idempotente
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
  archives GPU (app/E2E : `ggml-cuda`… ; `tldl-llm` : `tldl_ggml-cuda`…)
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
  UI + E2E `tldl-e2e devices` : CPU détecté, `resolve(true)` bascule CPU —
  attendu sur build CPU) ; à valider dans l'app

### F12 — Benchmark & cibles perf 📋
Matrice modèle × device × durée (5 / 10 / 30 min) ; les résultats mesurés
alimentent models.md (remplacent les estimations).

### F13 — Lecteur audio intégré 💡
Player synchronisé aux segments (clic = saut, reprise du segment).

### F14 - Travail par section 🔎
Proposer de travailler soit sur le fichier complet ou sur des sections.

### F15 - Contextes de prompt ⏳
L'utilisateur choisit ou crée un contexte d'analyse / résumé avant de
demander une transcription ; favoris pour la réutilisation rapide ; page
dédicée à la création / gestion.
- **Choix à l'import** : sélecteur « Contexte d'analyse / résumé »
  (valeur par défaut = prompt « rencontre » de l'architecture §5.1) +
  création rapide (nom + instructions) sans quitter la page
- **Page « Contextes »** : création, modification, suppression (confirmée),
  favoris ★ épinglés en tête de liste
- **Injection** : le contexte est inséré dans le prompt LLM (section «
  Contexte d'analyse demandé par l'utilisateur ») — le binaire `tldl-llm`
  n'est pas modifié (il lit le prompt complet depuis son fichier)
- **Traçabilité** : le nom du contexte est consigné dans l'historique
  (colonne `records.context_name`), affiché sur la ligne d'historique
- **Stockage** : table SQLite `contexts` (même base que l'historique — F7)
- **Statut** : code en place (storage + prompt + UI + E2E `tldl-e2e
  contexts`) ; à valider dans l'app

### F16 - Eviter la transcription d'un fichier
Si un fichier à déjà été analysé il faudrait que l'utilisateur soit averti. Veut-il forcer une nouvelle analyse ? Si oui, alors il continue, sinon il est redirigé vers la transcription correspondante. Il peut toutefois, créer plusieurs transcription / résumés selon les contextes de prompt.

### F17 - Permettre d'avoir un mode avancée pour connecter des modèles via URL/IP
Permettre à l'utilisateur de pointer sur un provider local ou web IA (mais à ses risques).

### F18 - Permettre à l'utilisateur de mettre une url pour télécharger un modèle autre que ceux conseillés
Dans le mode avancé également.
