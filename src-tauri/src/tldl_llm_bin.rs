//! `tldl-llm` — moteur LLM (llama.cpp b10884) dans un **processus dédié**.
//!
//! Pourquoi un processus à part : whisper.cpp v1.9.5 embarque son propre
//! `ggml` (0.29.0) et llama.cpp b10884 le sien ; les deux exportent les
//! **mêmes symboles C** (`ggml_*`). Dans un même binaire, le linker ne
//! résout chaque nom que contre une seule des deux versions — le moteur
//! « perdant » s'exécute alors contre l'ABI ggml adverse (SIGSEGV en
//! `ggml_compute_forward_map_custom1`, 242/242 symboles interposés).
//! Renommer les *archives* ne renomme pas les *symboles*. → chaque moteur
//! a son propre binaire (l'ASR et le LLM ne tournent jamais en parallèle
//! de toute façon ; cf. docs/architecture.md §5).
//!
//! Usage :
//!   tldl-llm <gguf> <fichier_prompt> [max_tokens] [temperature] [n_gpu_layers] \
//!             [repeat_penalty] [penalty_window]
//!   n_gpu_layers : -1 = GPU (toutes les couches), 0 = CPU (défaut) — F10.
//!   repeat_penalty / penalty_window : pénalité de répétition du sampling
//!   (cf. engine.rs) ; optionnels, valeurs par défaut = constantes du moteur.
//!
//! Protocole :
//!   - stdout : le Markdown généré (et rien d'autre),
//!   - stderr : lignes `tldl-progress <produit> <max>` (1 ligne/token),
//!     sinon des messages d'erreur libres,
//!   - code de sortie : 0 = OK, 1 = erreur.
//!
//! Lien : les `#[link]` ci-dessous cantonnent les bibliothèques llama.cpp
//! à ce binaire ; le binaire de l'app ne lie que le ggml de whisper.cpp
//! (`src/asr/mod.rs`).

#[link(name = "tldl_llama_shim", kind = "static")]
#[link(name = "llama", kind = "static")]
#[link(name = "tldl_ggml", kind = "static")]
#[link(name = "tldl_ggml-base", kind = "static")]
#[link(name = "tldl_ggml-cpu", kind = "static")]
extern "C" {}

// F10 : archives du backend GPU (ggml de llama.cpp, renommées `tldl_ggml*`
// par build.rs), présentes seulement si `TLDL_GPU` l'a compilée ; le
// registre `tldl_ggml` les référence (`GGML_USE_*`), d'où *après* les liens
// ci-dessus. `#[link]` sur un *module* = ignoré par rustc : blocs `extern`
// vides. Les import libs CUDA (cudart / cublas / pilote `cuda`) suivent :
// référencés par les objets de `tldl_ggml-cuda`.
#[cfg(tldl_gpu_cuda)]
#[link(name = "tldl_ggml-cuda", kind = "static")]
extern "C" {}
#[cfg(tldl_gpu_cuda)]
#[link(name = "cudart", kind = "static")]
extern "C" {}
#[cfg(tldl_gpu_cuda)]
#[link(name = "cublas", kind = "static")]
extern "C" {}
#[cfg(tldl_gpu_cuda)]
#[link(name = "cuda", kind = "static")]
extern "C" {}
#[cfg(tldl_gpu_vulkan)]
#[link(name = "tldl_ggml-vulkan", kind = "static")]
extern "C" {}
#[cfg(tldl_gpu_metal)]
#[link(name = "tldl_ggml-metal", kind = "static")]
extern "C" {}

// Runtime C++ : les objets ggml/llama référencent `operator new`,
// `__gxx_personality_seh0`, … — sur la cible GNU, Rust ne lie pas
// `libstdc++` par défaut. La directive `dylib=stdc++` du build script
// n'atteignait pas la ligne de lien de *ce* binaire (les `#[link]`
// ci-contre, eux, oui) : on la fige ici, dans le même `-Wl,-Bstatic`
// bloc (après les objets → ordre de lien correct).
#[cfg(target_env = "gnu")]
#[link(name = "stdc++")]
extern "C" {}

// ggml-cpu (llama.cpp) sonde le nom de la CPU dans le registre
// (HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0) → advapi32.dll.
#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
extern "C" {}

mod engine;

use std::env;
use std::io::Write;
use std::os::raw::c_int;

fn main() {
    let argv: Vec<String> = env::args().skip(1).map(|s| s.trim().to_string()).collect();
    let (Some(gguf), Some(prompt_file)) = (argv.first().cloned(), argv.get(1).cloned()) else {
        eprintln!("usage : tldl-llm <gguf> <fichier_prompt> [max_tokens] [temperature] [n_gpu_layers] [repeat_penalty] [penalty_window]");
        std::process::exit(2);
    };
    let max_tokens: u32 = argv.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let temperature: f32 = argv.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.3);
    // F10 : offload GPU. -1 = toutes les couches, 0 (défaut) = CPU.
    let n_gpu_layers: i32 = argv.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    // Pénalité de répétition + fenêtre (sampling, cf. engine.rs) : optionnel,
    // valeurs par défaut = les constantes du moteur.
    let repeat_penalty: f32 = argv.get(5).and_then(|s| s.parse().ok()).unwrap_or(engine::REPEAT_PENALTY);
    let penalty_window: c_int = argv.get(6).and_then(|s| s.parse().ok()).unwrap_or(engine::PENALTY_WINDOW);

    let prompt = match std::fs::read_to_string(&prompt_file) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("tldl-llm : lecture du prompt ({prompt_file}) impossible : {e}");
            std::process::exit(1);
        }
    };

    let eng = match engine::Engine::load(&gguf, n_gpu_layers as c_int) {
        Ok(e) => e,
        Err(m) => {
            eprintln!("tldl-llm : {m}");
            std::process::exit(1);
        }
    };

    // Progression : 1 ligne/token sur stderr, lue par le processus parent.
    let out = eng.generate(
        &prompt,
        temperature,
        max_tokens,
        repeat_penalty,
        penalty_window,
        &mut |n, max| {
            let _ = write!(std::io::stderr(), "tldl-progress {n} {max}\n");
        },
    );
    match out {
        Ok(text) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            let _ = std::io::stdout().flush();
        }
        Err(m) => {
            eprintln!("tldl-llm : {m}");
            std::process::exit(1);
        }
    }
}
