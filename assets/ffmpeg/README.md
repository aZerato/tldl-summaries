# FFmpeg embarqué (tâche à venir)

Contrainte C5 : la vidéo est convertie en audio **dans** l'app.
À l'étape « bundling », on posera ici les binaires statiques par plateforme
(`ffmpeg` + `ffprobe`) :

```
assets/ffmpeg/
├─ x86_64-pc-windows-msvc/ffmpeg.exe
├─ x86_64-pc-windows-msvc/ffprobe.exe
├─ aarch64-apple-darwin/ffmpeg
└─ x86_64-unknown-linux-gnu/ffmpeg
```

En attendant, le noyau utilise l'env `TLDL_FFMPEG_DIR` (dossier de binaires)
ou le `PATH` système (cf. `src-tauri/src/media.rs`).
