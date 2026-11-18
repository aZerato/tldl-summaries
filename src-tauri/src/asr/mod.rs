//! ASR — transcription (whisper.cpp, ADR-002).
//!
//! Branchement : shim C (`c/tldl_whisper_shim.c`) + libs statiques `whisper`
//! et `ggml` compilées par CMake dans `build.rs`.
//!
//! Audio d'entrée : WAV PCM s16 16 kHz mono, produit par `media::to_wav_16k`.

use serde::Serialize;
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Début du segment, en secondes
    pub start: f64,
    /// Fin du segment, en secondes
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcription {
    /// Texte intégral (segments concaténés)
    pub text: String,
    /// Segments horodatés
    pub segments: Vec<Segment>,
    /// Langue détectée (code ISO, "fr", "en", ...)
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct AsrConfig {
    /// Identifiant du modèle (catalogue : models.rs)
    pub model: String,
    /// "auto" | "fr" | ...
    pub language: String,
}

mod ffi {
    use std::os::raw::{c_char, c_int};

    /// Poignée opaque (côté shim C).
    pub struct TldlWhisper;

    // Lien par `#[link]` (et non `build.rs`) : les directives du build script
    // s'appliqueraient à *tous* les cibles du paquet, et le binaire
    // `tldl-llm` hériterait alors du ggml 0.29.0 de whisper.cpp — or ses
    // symboles `ggml_*` doivent être résolus contre le ggml de llama.cpp
    // (cf. `src/tldl_llm_bin.rs`). Ordre : shim → whisper → ggml.
    #[link(name = "tldl_whisper_shim", kind = "static")]
    #[link(name = "whisper", kind = "static")]
    #[link(name = "ggml", kind = "static")]
    #[link(name = "ggml-base", kind = "static")]
    #[link(name = "ggml-cpu", kind = "static")]
    extern "C" {
        pub fn tldl_whisper_init(
            model_path: *const c_char,
            use_gpu: c_int,
            n_threads: c_int,
        ) -> *const TldlWhisper;
        pub fn tldl_whisper_free(w: *const TldlWhisper);
        pub fn tldl_whisper_transcribe_s16(
            w: *const TldlWhisper,
            samples: *const i16,
            n_samples: c_int,
            language: *const c_char,
        ) -> c_int;
        pub fn tldl_whisper_get_progress() -> c_int;
        pub fn tldl_whisper_n_segments(w: *const TldlWhisper) -> c_int;
        pub fn tldl_whisper_get_segment_time(
            w: *const TldlWhisper,
            i: c_int,
            t0_cs: *mut i64,
            t1_cs: *mut i64,
        );
        pub fn tldl_whisper_get_segment_text(
            w: *const TldlWhisper,
            i: c_int,
            buf: *mut c_char,
            buf_len: c_int,
        ) -> c_int;
        pub fn tldl_whisper_lang_str(w: *const TldlWhisper, buf: *mut c_char, buf_len: c_int) -> c_int;
    }
}
use ffi::TldlWhisper;

// F10 : archives des backends GPU (whisper.cpp), liées seulement si la
// build les a compilées (variable `TLDL_GPU` — cf. build.rs). Le registre
// du `ggml` ci-contre les référence (`GGML_USE_*`) : l'archive doit suivre
// `ggml`/`ggml-base` dans l'ordre de lien, d'où ces modules *après* `ffi`.
#[cfg(tldl_gpu_cuda)]
#[link(name = "ggml-cuda", kind = "static")]
mod _ggml_backend_cuda {}
#[cfg(tldl_gpu_vulkan)]
#[link(name = "ggml-vulkan", kind = "static")]
mod _ggml_backend_vulkan {}
#[cfg(tldl_gpu_metal)]
#[link(name = "ggml-metal", kind = "static")]
mod _ggml_backend_metal {}

/// Moteur whisper (poignée RAII ; le C++ est libéré au drop).
struct Engine {
    ptr: *const TldlWhisper,
}

impl Engine {
    fn load(model: &Path, use_gpu: bool) -> Result<Self, String> {
        // Chemin null-terminé : le C s'attend à une `char*`, pas à un slice.
        let cpath = std::ffi::CString::new(model.to_string_lossy().into_owned())
            .map_err(|e| format!("chemin du modèle invalide : {e}"))?;
        let ptr = unsafe {
            ffi::tldl_whisper_init(cpath.as_ptr(), use_gpu as c_int, 0)
        };
        if ptr.is_null() {
            Err(format!(
                "whisper : chargement du modèle impossible — {}",
                model.display()
            ))
        } else {
            Ok(Self { ptr })
        }
    }

    /// Lancer la transcription (bloquant — à appeler sur un thread dédié).
    fn transcribe(&self, samples: &[i16], language: &str) -> Result<(), String> {
        // whisper_full() compte les échantillons en int32 : ~41 h à 16 kHz.
        if samples.len() > i32::MAX as usize {
            return Err("audio trop longue pour une passe unique (> 41 h) — \
                         le morcellement est post-MVP"
                .into());
        }
        // Langue null-terminée (le C s'attend à une `char*`).
        let clang = std::ffi::CString::new(language.to_string())
            .map_err(|e| format!("langue invalide : {e}"))?;
        let code = unsafe {
            ffi::tldl_whisper_transcribe_s16(
                self.ptr,
                samples.as_ptr(),
                samples.len() as c_int,
                clang.as_ptr(),
            )
        };
        if code != 0 {
            Err(format!("whisper_full a échoué (code {code})"))
        } else {
            Ok(())
        }
    }

    /// Segments + langue + texte intégral, une fois `transcribe` terminé.
    fn segments(&self) -> Result<(Vec<Segment>, String, String), String> {
        let n = unsafe { ffi::tldl_whisper_n_segments(self.ptr) };
        if n < 0 {
            return Err("whisper n'a produit aucun segment (audio vide ?)".into());
        }
        let mut segments = Vec::with_capacity(n as usize);
        let mut full = String::new();
        for i in 0..n {
            let (t0, t1) = {
                let mut a: i64 = 0;
                let mut b: i64 = 0;
                unsafe {
                    ffi::tldl_whisper_get_segment_time(self.ptr, i, &mut a, &mut b)
                }
                (a, b)
            };
            let mut buf = vec![0u8; 8192];
            let len = unsafe {
                ffi::tldl_whisper_get_segment_text(
                    self.ptr,
                    i,
                    buf.as_mut_ptr() as *mut c_char,
                    buf.len() as c_int,
                )
            };
            if len < 0 {
                return Err("lecture du segment impossible".into());
            }
            let text = String::from_utf8_lossy(&buf[..len as usize]).trim().to_string();
            if !text.is_empty() {
                if !full.is_empty() {
                    full.push(' ');
                }
                full.push_str(&text);
            }
            segments.push(Segment {
                start: t0 as f64 / 100.0,
                end: t1 as f64 / 100.0,
                text,
            });
        }

        let mut lang = [0u8; 16];
        let ll = unsafe {
            ffi::tldl_whisper_lang_str(
                self.ptr,
                lang.as_mut_ptr() as *mut c_char,
                lang.len() as c_int,
            )
        };
        let language = if ll > 0 {
            String::from_utf8_lossy(&lang[..ll as usize]).to_string()
        } else {
            "auto".to_string()
        };
        Ok((segments, language, full.trim().to_string()))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { ffi::tldl_whisper_free(self.ptr) }
    }
}

/// Transcrit un WAV PCM s16 16 kHz mono.
///
/// `progress` est appelé périodiquement avec (position, durée) en secondes ;
/// la transcription elle-même tourne sur un thread dédié (whisper_full est
/// bloquant), la progression est lue via la variable globale du shim.
pub fn transcribe(
    wav: &Path,
    cfg: &AsrConfig,
    use_gpu: bool,
    progress: &mut dyn FnMut(f64, f64),
) -> Result<Transcription, String> {
    let model_path = crate::models::model_path(crate::models::ModelKind::Asr, &cfg.model);
    if !model_path.exists() {
        return Err(format!(
            "modèle ASR introuvable : {} — déposez le fichier .bin dans \
             models/ (F11)",
            model_path.display()
        ));
    }
    let (samples, rate) = read_wav(wav)
        .map_err(|e| format!("WAV illisible ({}) : {e}", wav.display()))?;
    if rate != 16_000 {
        return Err(format!(
            "fréquence d'échantillonnage non prise en charge : {rate} Hz (16000 attendu)"
        ));
    }
    let dur = samples.len() as f64 / 16_000.0;

    let (tx, rx) = mpsc::channel();
    let worker_language = cfg.language.clone();
    let worker = thread::spawn(move || {
        let res = run(&model_path, samples, &worker_language, use_gpu);
        let _ = tx.send(res);
    });

    loop {
        let p = unsafe { ffi::tldl_whisper_get_progress() };
        let p = if p < 0 { 0 } else if p > 100 { 100 } else { p };
        progress(dur * p as f64 / 100.0, dur);
        match rx.try_recv() {
            Ok(r) => {
                let _ = worker.join();
                return r;
            }
            Err(mpsc::TryRecvError::Empty) => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let _ = worker.join();
                return Err("thread ASR interrompu".into());
            }
        }
    }
}

/// Chargement du modèle + transcription + lecture des segments.
fn run(
    model: &Path,
    samples: Vec<i16>,
    language: &str,
    use_gpu: bool,
) -> Result<Transcription, String> {
    let engine = Engine::load(model, use_gpu)?;
    engine.transcribe(&samples, language)?;
    let (segments, language, text) = engine.segments()?;
    Ok(Transcription {
        text,
        segments,
        language,
    })
}

/// Lit un fichier WAV RIFF : retourne (échantillons s16 LE, fréquence Hz).
/// Contrainte : PCM 16-bit mono (sinon erreur).
fn read_wav(path: &Path) -> Result<(Vec<i16>, u32), String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("fichier non WAV (en-tête RIFF/WAVE manquant)".into());
    }
    let mut pos: usize = 12;
    let mut rate: u32 = 0;
    let mut fmt_ok = false;
    loop {
        if pos + 8 > b.len() {
            break;
        }
        let cid = &b[pos..pos + 4];
        let csz = u32::from_le_bytes([b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]]) as usize;
        pos += 8;
        let body = &b[pos..(pos + csz).min(b.len())];
        pos += csz;
        if cid == b"fmt " {
            // Chunk fmt PCM : 16 o minima (wFormatTag, nChannels,
            // dwSamplesPerSec, dwAvgBytesPerSec, wBlockAlign,
            // wBitsPerSample). Le lecteur s'arrête à l'index 15 (bits).
            if body.len() < 16 {
                return Err("chunk fmt tronqué".into());
            }
            let audio = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            if audio != 1 {
                return Err("audio non PCM (PCM attendu)".into());
            }
            if channels != 1 {
                return Err("stéréo non pris en charge (mono attendu)".into());
            }
            if bits != 16 {
                return Err("profondeur non prise en charge (16 bits attendu)".into());
            }
            fmt_ok = true;
        } else if cid == b"data" {
            if !fmt_ok {
                return Err("chunk data avant fmt".into());
            }
            let n = body.len() / 2;
            let mut samples = vec![0i16; n];
            for (i, s) in samples.iter_mut().enumerate() {
                *s = i16::from_le_bytes([body[2 * i], body[2 * i + 1]]);
            }
            return Ok((samples, rate));
        }
    }
    Err("aucun chunk data dans le WAV".into())
}
