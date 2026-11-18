//! Accélération GPU (F10) : détection des devices ggml + résolution
//! CPU/GPU avec bascule CPU si le GPU demandé est absent.
//!
//! C3 (« Sobre mais performant : GPU optionnel, CPU viable ») : le backend
//! GPU est activé au *build* (variable `TLDL_GPU` — cf. `build.rs`), jamais
//! par défaut. La détection ci-dessous interroge le registre de devices
//! du ggml de whisper.cpp, lié dans l'app (`src/asr/mod.rs`) : les symboles
//! `ggml_backend_dev_*` y sont résolus. Sur une build CPU-only (défaut),
//! seul le device CPU est présent et la résolution retombe toujours sur le
//! CPU — le même binaire reste viable sans GPU.

use serde::Serialize;
use std::os::raw::c_char;

/// Un device ggml disponible (CPU, GPU, iGPU…).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Nom lisible (ex. « CPU », « NVIDIA GeForce RTX 4060 »).
    pub name: String,
    /// « cpu » | « gpu » | « igpu » | « accel ».
    pub kind: String,
}

/// Résultat de la résolution du device pour un pipeline.
#[derive(Debug, Clone)]
pub struct DeviceResolution {
    /// Nom du device résolu (ex. « CPU », « NVIDIA … »).
    pub name: String,
    /// « cpu » | « gpu ».
    pub kind: String,
    /// ASR (whisper `use_gpu`) : `true` si GPU.
    pub use_gpu: bool,
    /// LLM (llama `n_gpu_layers`) : `-1` = toutes les couches, `0` = CPU.
    pub n_gpu_layers: i32,
    /// `true` si le GPU était demandé mais absent (bascule CPU — C3).
    pub fallback: bool,
}

/// Liste des devices ggml disponibles (exposée à l'UI — F10).
pub fn list_devices() -> Vec<DeviceInfo> {
    let n = unsafe { ffi::ggml_backend_dev_count() };
    let mut out = Vec::new();
    for i in 0..n {
        let d = unsafe { ffi::ggml_backend_dev_get(i) };
        if d.is_null() {
            continue;
        }
        let t = unsafe { ffi::ggml_backend_dev_type(d) };
        let kind = match t {
            0 => "cpu",
            1 => "gpu",
            2 => "igpu",
            3 => "accel",
            _ => "autre",
        };
        let name = cstr(unsafe { ffi::ggml_backend_dev_name(d) });
        out.push(DeviceInfo {
            name: if name.is_empty() {
                format!("{kind} {i}")
            } else {
                name
            },
            kind: kind.to_string(),
        });
    }
    out
}

/// Résout le device pour un pipeline (F10) :
/// - `use_gpu = false` → CPU forcé ;
/// - `use_gpu = true` → le premier device GPU/iGPU détecté, sinon bascule CPU
///   (`fallback = true`) si la build ou la machine n'en a pas.
pub fn resolve(use_gpu: bool) -> DeviceResolution {
    if !use_gpu {
        return DeviceResolution {
            name: "CPU".to_string(),
            kind: "cpu".into(),
            use_gpu: false,
            n_gpu_layers: 0,
            fallback: false,
        };
    }
    for d in list_devices() {
        if d.kind == "gpu" || d.kind == "igpu" {
            return DeviceResolution {
                name: d.name,
                kind: d.kind,
                use_gpu: true,
                n_gpu_layers: -1,
                fallback: false,
            };
        }
    }
    // GPU demandé mais absent (build CPU-only ou pas de device) : bascule CPU.
    DeviceResolution {
        name: "CPU".to_string(),
        kind: "cpu".into(),
        use_gpu: false,
        n_gpu_layers: 0,
        fallback: true,
    }
}

/// Lit une `const char*` nul-terminée en `String` (vide si `NULL`).
fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
}

/// Déclarations FFI du registre de devices ggml (whisper.cpp, lié dans l'app).
///
/// Le type device est opaque (pointeur) : on n'a besoin que de fonctions
/// scalaires + `const char*`, pas de layout de struct — pas de shim C requis.
mod ffi {
    use std::os::raw::{c_char, c_int, c_void};

    #[link(name = "ggml", kind = "static")]
    extern "C" {
        pub fn ggml_backend_dev_count() -> usize;
        pub fn ggml_backend_dev_get(index: usize) -> *const c_void;
        pub fn ggml_backend_dev_name(d: *const c_void) -> *const c_char;
        pub fn ggml_backend_dev_type(d: *const c_void) -> c_int;
    }
}
