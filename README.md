# TL;DW — tldl-summaries

Synthèse locale d'audio/vidéo : import d'un fichier → **transcription**
(whisper.cpp) + **résumé structuré** (llama.cpp), 100 % hors ligne.

👉 Référence du projet : [docs/](docs/) — architecture, features, modèles.

## Stack

- Coquille : **Tauri 2** (Rust) + **React / TypeScript** (Vite)
- ASR : whisper.cpp (GGUF) — ⚠️ stub, moteur à brancher (F3)
- LLM : llama.cpp (GGUF, Q4_K_M) — ⚠️ stub, moteur à brancher (F4)
- Média : FFmpeg (sonde + conversion WAV 16 kHz mono)
- Stockage : SQLite (rusqlite, bundled)

## Prérequis dev

- **Rust** (MSVC build tools sous Windows) — à installer : `winget install Rust`
- **Node 20+**
- Windows : **WebView2** (préinstallé sur Win 11)
- **FFmpeg** sur le PATH (ou `TLDL_FFMPEG_DIR` pointant vers les
  binaires) — l'embarquement est une tâche à venir (`assets/ffmpeg/`)

## Lancer (dev)

```
npm install
npm run tauri dev
```

Le premier `tauri dev` compile le crate Rust (long la 1ʳᵉ fois).

## Construire (prod)

```
npm run tauri build
```

## État

Scaffold initial posé le 14/11/2026 :

- UI : pages Import / Résultat (double-vue Transcription | Résumé) / Modèles
- Noyau : pipeline orchestré (probe → WAV → ASR → LLM), catalogue de
  modèles, historique SQLite, config persistée
- **ASR/LLM = stubs** : interfaces prêtes (types, progress, chemins des
  modèles), moteurs à brancher
- FFmpeg : résolu via `TLDL_FFMPEG_DIR` ou PATH (embarquement à venir)
- Modèles : dépôt manuel dans `<données-app>/models/` (téléchargement F11
  à brancher)

Voir [docs/features.md](docs/features.md) pour l'état par feature.
