//! Média : sonde (ffprobe) + conversion WAV PCM 16 kHz mono (ffmpeg).
//!
//! C5 : la vidéo est convertie en audio **dans** l'app.
//! Embarquement des binaires FFmpeg : `assets/ffmpeg/<cible>/`, résolus sans
//! variable d'env (bundle Tauri en release, repo en dev) — cf. `resolve`.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub duration_secs: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub format: String,
}

/// Résolution du binaire FFmpeg, sans dépendre d'une variable d'environnement :
/// 1) `TLDL_FFMPEG_DIR` (override — E2E / tests),
/// 2) bundle release : `<exe>/_up_/assets/ffmpeg/` (Tauri `resources`),
/// 3) dev : `assets/ffmpeg/` du repo (recherche par remontée depuis l'exécutable),
/// 4) sinon le PATH système.
pub fn resolve(name: &str) -> String {
    let bin = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(dir) = std::env::var("TLDL_FFMPEG_DIR") {
        let p = Path::new(&dir).join(&bin);
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }
    for base in local_dirs() {
        if let Some(p) = find_in(&base, &bin) {
            return p.to_string_lossy().into_owned();
        }
    }
    bin
}

/// Cherche `bin` directement dans `base`, sinon dans l'un de ses sous-dossiers
/// (les binaires FFmpeg sont posés sous `assets/ffmpeg/<cible>/`).
fn find_in(base: &Path, bin: &str) -> Option<PathBuf> {
    let direct = base.join(bin);
    if direct.exists() {
        return Some(direct);
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for e in entries.flatten() {
            if e.file_type().ok()?.is_dir() {
                let p = e.path().join(bin);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Dossiers candidats, du plus spécifique au plus général.
fn local_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // 2) Bundle release : `../assets/ffmpeg/` embarqué par Tauri, posé
            //    sous `<exe>/_up_/assets/ffmpeg/` (règle `_up_` des resources).
            dirs.push(exe_dir.join("_up_").join("assets").join("ffmpeg"));
            // 3) Dev : remontée depuis l'exécutable jusqu'à la racine du repo,
            //    en cherchant `assets/ffmpeg` à chaque niveau.
            let mut cur: Option<&Path> = Some(exe_dir);
            while let Some(d) = cur {
                dirs.push(d.join("assets").join("ffmpeg"));
                cur = d.parent();
            }
        }
    }
    dirs
}

fn run(cmd: &mut Command) -> Result<String, String> {
    match cmd.output() {
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => Err(format!(
            "{} a échoué : {}",
            cmd.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
            "FFmpeg introuvable : posez les binaires embarqués sous \
             assets/ffmpeg/ (ou installez-le sur le PATH)."
                .into(),
        ),
        Err(e) => Err(e.to_string()),
    }
}

/// Sonde du média : format, durée, présence des pistes vidéo/audio.
pub fn probe(path: &str) -> Result<MediaInfo, String> {
    let mut cmd = Command::new(resolve("ffprobe"));
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"]);
    cmd.arg(path);
    let out = run(&mut cmd)?;

    let v: serde_json::Value = serde_json::from_str(&out)
        .map_err(|e| format!("ffprobe : réponse illisible ({e})"))?;
    let format = v["format"]["format_name"]
        .as_str()
        .unwrap_or("inconnu")
        .to_string();
    let duration_secs = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    let mut has_video = false;
    let mut has_audio = false;
    if let Some(streams) = v["streams"].as_array() {
        for s in streams {
            match s.get("codec_type").and_then(|t| t.as_str()) {
                Some("video") => has_video = true,
                Some("audio") => has_audio = true,
                _ => {}
            }
        }
    }

    Ok(MediaInfo {
        path: path.to_string(),
        duration_secs,
        has_video,
        has_audio,
        format,
    })
}

/// Convertit n'importe quel média (audio ou vidéo) en WAV PCM 16 kHz mono —
/// le format natif de whisper.cpp.
pub fn to_wav_16k(input: &Path, output: &Path) -> Result<(), String> {
    let mut cmd = Command::new(resolve("ffmpeg"));
    cmd.args(["-hide_banner", "-nostdin", "-i"]);
    cmd.arg(input);
    cmd.args(["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-y"]);
    cmd.arg(output);
    run(&mut cmd)?;
    Ok(())
}
