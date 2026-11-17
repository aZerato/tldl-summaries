//! Moteur LLM (llama.cpp b10884) — FFI vers le shim C
//! (`c/tldl_llama_shim.c`).
//!
//! Compilé **uniquement** par le binaire `tldl-llm`
//! (`src/tldl_llm_bin.rs`) : le LLM tourne dans un processus dédié, pas
//! dans l'app — le ggml de whisper.cpp et le ggml de llama.cpp ne
//! peuvent pas partager un même processus (cf. `tldl_llm_bin.rs`).
//!
//! CPU-only (C2) : `n_gpu_layers = 0`, OpenMP désactivé.

use std::os::raw::{c_char, c_int, c_void};

/// Taille de contexte (tokens) : prompt + résumé.
pub const N_CTX: c_int = 8192;
/// Top-p de la chaîne de sampling.
const TOP_P: f32 = 0.95;
/// Graine fixe : résumé reproductible à prompt identique.
const SEED: u32 = 0;

/// Moteur llama.cpp (modèle + contexte) — libéré via `Drop`.
pub struct Engine {
    raw: *mut ffi::TldlLlama,
}

impl Engine {
    /// Charge un GGUF. `model_path` : chemin du fichier `.gguf`.
    pub fn load(model_path: &str) -> Result<Self, String> {
        unsafe {
            let cpath = std::ffi::CString::new(model_path).map_err(|e| e.to_string())?;
            let mut e: *mut ffi::TldlLlama = std::ptr::null_mut();
            let mut err = [0u8; 512];
            let rc = ffi::tldl_llama_load(
                cpath.as_ptr(),
                N_CTX,
                &mut e,
                err.as_mut_ptr() as *mut c_char,
                err.len() as c_int,
            );
            if rc != 0 {
                let msg = String::from_utf8_lossy(&err).trim_end().to_string();
                return Err(if msg.is_empty() {
                    "llama.cpp : chargement du modèle échoué".into()
                } else {
                    msg
                });
            }
            Ok(Self { raw: e })
        }
    }

    /// Génère depuis un prompt UTF-8 ; `progress` : (tokens produits, max).
    pub fn generate(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: u32,
        progress: &mut dyn FnMut(u32, u32),
    ) -> Result<String, String> {
        let bytes = prompt.as_bytes();
        let out_max = (max_tokens as usize) * 32;
        let mut out = vec![0u8; out_max];
        // Le callback C reçoive le pointeur (gras) du closure par adresse :
        // il faut garder `fat` en vie pendant tout l'appel.
        let mut fat: *mut dyn FnMut(u32, u32) = progress as *mut dyn FnMut(u32, u32);
        let n = unsafe {
            ffi::tldl_llama_generate(
                self.raw,
                bytes.as_ptr() as *const c_char,
                bytes.len() as c_int,
                max_tokens as c_int,
                temperature,
                TOP_P,
                SEED,
                out.as_mut_ptr() as *mut c_char,
                out_max as c_int,
                Some(progress_cb),
                &mut fat as *mut *mut dyn FnMut(u32, u32) as *mut c_void,
            )
        };
        if n < 0 {
            return Err("llama.cpp : génération échouée".into());
        }
        if n == 0 {
            return Err(
                "le modèle n'a produit aucun token (dépassement de contexte ?)".into(),
            );
        }
        Ok(String::from_utf8_lossy(&out[..n as usize]).trim_end().to_string())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            ffi::tldl_llama_free(self.raw);
        }
        self.raw = std::ptr::null_mut();
    }
}

/// Callback de progression (ABI C) : `user` est l'adresse d'un
/// `*mut dyn FnMut(u32, u32)` stocké (pointeur « gras ») ; on le relit pour
/// reconstituer la closure Rust.
extern "C" fn progress_cb(produced: c_int, max: c_int, user: *mut c_void) {
    unsafe {
        let fp = std::ptr::read(user as *const *mut dyn FnMut(u32, u32));
        let f: &mut dyn FnMut(u32, u32) = &mut *fp;
        f(produced as u32, max as u32);
    }
}

/// Déclarations FFI (shim `c/tldl_llama_shim.c`).
mod ffi {
    use std::os::raw::{c_char, c_int, c_void};

    /// Opaque : moteur llama.cpp (modèle + contexte + vocabulaire).
    pub struct TldlLlama;

    extern "C" {
        pub fn tldl_llama_load(
            path: *const c_char,
            n_ctx: c_int,
            out: *mut *mut TldlLlama,
            err: *mut c_char,
            err_len: c_int,
        ) -> c_int;
        pub fn tldl_llama_free(e: *mut TldlLlama);
        pub fn tldl_llama_generate(
            e: *mut TldlLlama,
            prompt: *const c_char,
            prompt_len: c_int,
            max_tokens: c_int,
            temperature: f32,
            top_p: f32,
            seed: u32,
            out: *mut c_char,
            out_max: c_int,
            cb: Option<extern "C" fn(c_int, c_int, *mut c_void)>,
            user: *mut c_void,
        ) -> c_int;
    }
}
