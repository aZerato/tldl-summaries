# TL;DW — tldl-summaries

Synthèse locale d'audio/vidéo : import d'un fichier → **transcription**
(whisper.cpp) + **résumé structuré** (llama.cpp), 100 % hors ligne.

👉 Référence du projet : [docs/](docs/) — architecture, features, modèles.

## Stack

- Coquille : **Tauri 2** (Rust) + **React / TypeScript** (Vite)
- ASR : whisper.cpp v1.9.5 (vendu, build CMake + shim C)
- LLM : llama.cpp b10884 — binaire dédié `tldl-llm` (GGUF, Q4_K_M)
- Média : FFmpeg (sonde + conversion WAV 16 kHz mono), **embarqué**
- Diarisation (F9) : **ONNX Runtime** (chargé au runtime, `load-dynamic`)
  + frontend fbank Wespeaker + **clustering en Rust** — séparation des
  intervenants 100 % locale (ADR-008)
- Stockage : SQLite (rusqlite, bundled)

## Prérequis dev

- **Rust** (MSVC build tools sous Windows) — à installer : `winget install Rust`
- **Node 20+**
- Windows : **WebView2** (préinstallé sur Win 11)
- **Aucun FFmpeg à installer** : les binaires sont embarqués dans le repo
  (`assets/ffmpeg/<cible>/`) et localisés automatiquement (bundle + dev) —
  `TLDL_FFMPEG_DIR` n'est qu'une override E2E/tests

## Lancer (dev)

```
npm install
npm run tauri dev
```

Le premier `tauri dev` compile le crate Rust (long la 1ʳᵉ fois).

Build GPU (CUDA sous Windows) : `npm run dev:gpu` — wrapper qui pose
`TLDL_GPU=cuda` + `CUDA_PATH` et met `bin\x64` CUDA sur le PATH runtime
(cible MSVC exigée ; cf. docs/architecture.md §10.1).

## Construire (prod)

```
npm run tauri build
```

## État

Post-MVP : MVP (F1 → F6 + F11) fait, puis F1/F10/F13/F14/F15/F16/F19
**validés dans l'app** (cf. [docs/README.md](docs/README.md)).

- **ASR** : whisper.cpp v1.9.5 branché (build CMake + shim C, GPU via
  `TLDL_GPU`)
- **LLM** : llama.cpp b10884 branché — binaire dédié `tldl-llm`, résumé
  Markdown
- **FFmpeg** : embarqué (`assets/ffmpeg/<cible>/`), localisé
  automatiquement — ni PATH, ni variable d'environnement requis
- **Diarisation** (F9) : ONNX Runtime au runtime (prébuilds
  `assets/onnxruntime/<cible>/`, pattern FFmpeg), modèle Wespeaker
  ECAPA-TDNN (F11 opt-in), clustering en Rust — séparation des
  intervenants, dégradation gracie si modèle/ORT absent
- **Modèles** : catalogue local + téléchargement Hugging Face opt-in (F11)
- **GPU** (F10) : build CUDA/Metal/Vulkan via `TLDL_GPU` (défaut CPU),
  détection des devices, bascule CPU si GPU absent

Voir [docs/features.md](docs/features.md) pour l'état par feature.
