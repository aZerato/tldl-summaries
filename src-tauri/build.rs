//! Build script : compile whisper.cpp (lib statiques `whisper` + `ggml` via CMake)
//! et le shim C (`c/tldl_whisper_shim.c`), puis llama.cpp (b10884 : lib statique
//! `llama` + ses archives `ggml*` via un wrapper CMake, et le shim
//! `c/tldl_llama_shim.c`).
//!
//! Les directives de *lien* (quelles archives finissent dans quel binaire)
//! ne sont PAS ici : un build script s'applique à tous les cibles du paquet,
//! et les deux moteurs partagent le même namespace de symboles C `ggml_*` —
//! l'app lie le ggml de whisper.cpp (`src/asr/mod.rs`), le binaire `tldl-llm`
//! le ggml de llama.cpp (`src/tldl_llm_bin.rs`), chacun par `#[link]`.
//!
//! Prérequis (voir docs/architecture.md §10) :
//!   - CMake sur le PATH (ou `TLDL_CMAKE`),
//!   - un toolchain C++ :
//!     a) MSVC (Build Tools, workload « C++ »), ou
//!     b) MinGW/WinLibs (GCC) — ex. winget `BrechtSanders.WinLibs.POSIX.MSVCRT`,
//!        le binaire `g++.exe` du toolchain sur le PATH, ou `TLDL_MINGW_DIR`
//!        pointant sur son dossier `bin`, ou
//!     c) Apple (macOS, build sur le Mac — C2) : Xcode Command Line Tools
//!        (`xcode-select --install`) — `/usr/bin/c++` (clang++) ; CMake via
//!        `brew install cmake`.
//!
//! Rust : cible `x86_64-pc-windows-gnu` (toolchain `stable-x86_64-pc-windows-gnu`,
//! `rustup override set` dans ce dossier) — ou `x86_64-pc-windows-msvc` si MSVC.

use std::env;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, PartialEq)]
enum Toolchain {
    /// MSVC : `cl.exe` sur le PATH (ou VS Build Tools en place).
    Msvc,
    /// MinGW/WinLibs : le dossier `bin` du toolchain (gcc, g++, ninja…).
    Gnu,
    /// Apple (macOS) : `c++` système (`/usr/bin/c++`, clang++) — Xcode
    /// Command Line Tools.
    Apple,
}

/// Gate de build du backend GPU (F10, C3 « GPU optionnel, CPU viable ») :
/// variable `TLDL_GPU` — `cpu` (défaut) | `cuda` | `metal` | `vulkan`.
/// `cuda` : toolkit CUDA (nvcc) requis ; `vulkan` : Vulkan SDK (glslc) +
/// SPIRV-Headers requis ; `metal` : macOS uniquement.
#[derive(Clone, Copy, PartialEq)]
enum Gpu {
    Cpu,
    Cuda,
    Metal,
    Vulkan,
}

fn gpu_from_env() -> Result<Gpu, String> {
    let v = env::var("TLDL_GPU").unwrap_or_default().to_lowercase();
    match v.as_str() {
        "" | "cpu" => Ok(Gpu::Cpu),
        "cuda" => Ok(Gpu::Cuda),
        "vulkan" => Ok(Gpu::Vulkan),
        "metal" => {
            // Metal n'existe que sur les hôtes Apple (C2) ; la build est
            // croisée mais l'hébergement est ici le poste de build.
            if env::consts::OS != "macos" {
                return Err(format!(
                    "TLDL_GPU=metal : Metal est macOS uniquement — \
                     relancez la build sur un poste macOS (l'host est {:?}).",
                    env::consts::OS
                ));
            }
            Ok(Gpu::Metal)
        }
        other => Err(format!(
            "TLDL_GPU inattendu : {other:?} (valeurs : cpu, cuda, metal, vulkan)"
        )),
    }
}

fn main() {
    // Contexte Tauri (icônes, capabilities, ...) — à laisser en place.
    tauri_build::build();

    // F10 : gate du backend GPU (variable `TLDL_GPU`, défaut `cpu`).
    let gpu = gpu_from_env().expect("TLDL_GPU invalide");
    println!("cargo:rerun-if-env-changed=TLDL_GPU");
    // Le cfg pilote les `#[link]` des archives des backends GPU (src/asr/
    // mod.rs, src/tldl_llm_bin.rs). Émis même sans toolchain C++ : le
    // `cargo check` ne lie pas, et la vraie build l'appliquera si le
    // toolchain est en place.
    match gpu {
        Gpu::Cuda => println!("cargo:rustc-cfg=tldl_gpu_cuda"),
        Gpu::Metal => println!("cargo:rustc-cfg=tldl_gpu_metal"),
        Gpu::Vulkan => println!("cargo:rustc-cfg=tldl_gpu_vulkan"),
        Gpu::Cpu => {}
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest = Path::new(&manifest_dir);
    // `vendor/` vit à la racine du repo (pas dans src-tauri/).
    let repo_root = manifest.parent().unwrap();
    let src = repo_root.join("vendor/whisper.cpp");
    let out = Path::new(&out_dir);
    let build_dir = out.join("whisper-build");

    // Rebuild du build-script si les sources C / la config CMake bougent.
    println!("cargo:rerun-if-changed={}", manifest.join("c/tldl_whisper_shim.c").display());
    println!("cargo:rerun-if-changed={}", src.join("include/whisper.h").display());
    println!("cargo:rerun-if-changed={}", src.join("CMakeLists.txt").display());
    let llama_src = repo_root.join("vendor/llama.cpp");
    println!("cargo:rerun-if-changed={}", manifest.join("c/tldl_llama_shim.c").display());
    println!("cargo:rerun-if-changed={}", llama_src.join("include/llama.h").display());
    println!("cargo:rerun-if-changed={}", llama_src.join("CMakeLists.txt").display());

    // Pas de toolchain C++ (CMake + cl.exe ou g++) ? On ne *compile* pas
    // whisper.cpp, mais on laisse `cargo check` type-vérifier le Rust.
    // La vraie build se fait sur une machine outillée.
    let toolchain = find_toolchain();
    if toolchain.is_none() || find_cmake().is_none() {
        println!("cargo:warning=toolchain C++ absente (CMake et/ou cl.exe / g++ / c++) : compilation de whisper.cpp ignorée (cargo check uniquement).");
        return;
    }
    let toolchain = toolchain.unwrap();

    build_whisper(&src, &build_dir, &toolchain, gpu).expect("build CMake de whisper.cpp en échec");

    // Le shim C (API plate pour Rust) — compilé par `cc`.
    let mut b = cc::Build::new();
    b.file(manifest.join("c/tldl_whisper_shim.c"));
    b.include(src.join("include"));
    // whisper.h inclut ggml.h.
    b.include(src.join("ggml/include"));
    b.warnings(false);
    if let Some(gcc) = gnu_gcc() {
        b.compiler(gcc);
    }
    b.try_compile("tldl_whisper_shim")
        .expect("compilation du shim C échouée");

    // Archives produites par CMake : whisper (src/), ggml + ggml-base +
    // ggml-cpu (ggml/src/). Sur GCC elles sortent SANS préfixe `lib`
    // (ggml.a, ggml-base.a, …), alors que GNU ld ne cherche que
    // `lib<name>.a` : on pose des copies préfixées dans le même dossier,
    // puis on émet l'ordre de lien dépendant → dépendu (single-pass).
    let libs = collect_link_libs(&build_dir);
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    for (_stem, p) in &libs {
        if let Some(d) = p.parent() {
            let dp = d.to_path_buf();
            if !dirs.iter().any(|x| x == &dp) {
                dirs.push(dp);
            }
        }
    }
    for dir in &dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    // Copies préfixées (sans écraser une archive posée par CMake).
    for (_stem, p) in &libs {
        if p.extension().is_some_and(|e| e == "a") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if !stem.starts_with("lib") {
                    let copy = p.with_file_name(format!("lib{}.a", stem));
                    overwrite_copy(p, &copy);
                }
            }
        }
    }
    // Ordre de lien (shim → whisper → ggml) : `#[link]` dans `src/asr/mod.rs`
    // (pas ici — cf. en-tête : un build script s'appliquerait aux deux
    // moteurs, qui ne peuvent pas partager le même binaire).
    // 2) llama.cpp (F4) : wrapper CMake (add_subdirectory vendor/llama.cpp,
    //    LLAMA_STANDALONE=OFF) + shim C. Les archives ggml de llama.cpp sont
    //    renommées `tldl_ggml*` : même noms que celles de whisper.cpp, API
    //    différentes — on les lie sous des noms distincts.
    let llama_build_dir = out.join("llama-build");
    let llama_ggml_libs =
        build_llama(&llama_src, &llama_build_dir, &toolchain, gpu)
            .expect("build CMake de llama.cpp en échec");

    // Rust ne lie pas la stdlib C++ : on la demande sur la cible GNU
    // (`stdc++`) et sur Apple (`c++`, libc++). Valeurs « dylib » :
    // partagées par les deux moteurs, sans risque — les archives
    // statiques n'en tirent les symboles que si référencées.
    if toolchain == Toolchain::Gnu {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
    if toolchain == Toolchain::Apple {
        // Mesure double avec le `#[link(name = "c++")]` de
        // `src/tldl_llm_bin.rs` (binaire `tldl-llm`) ; un `-lc++` en
        // double est inoffensif.
        println!("cargo:rustc-link-lib=dylib=c++");
    }

    // F10 : le dossier `lib` du toolkit CUDA — les `#[link]` des import
    // libs `cudart`/`cublas`/`cuda` (src/asr/mod.rs, src/tldl_llm_bin.rs)
    // résolvent `cudart.lib`/`cublas.lib`/`cuda.lib` (Windows) ou
    // `-lcudart`/`-lcublas`/`-lcuda` (Linux) dans ce dossier.
    if gpu == Gpu::Cuda {
        if let Some(dir) = cuda_lib_dir() {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
    }

    // Chaîne LLM : shim → llama → ggml (llama.cpp) — liée par `#[link]` dans
    // `src/tldl_llm_bin.rs` (les archives `tldl_ggml*` renommées plus haut
    // sont posées *in situ* ; les `#[link]` du binaire `tldl-llm` les
    // résolvent, et les `#[link]` de l'app résolvent le ggml de whisper.cpp
    // — pas de nom ambigu entre les deux arbres).
    let _ = llama_ggml_libs;
}

/// Configure + build de llama.cpp (b10884) en libs statiques, via un wrapper
/// CMake généré dans `OUT_DIR` (add_subdirectory → `LLAMA_STANDALONE=OFF` :
/// pas de tools/tests/common/server). `gpu` : gate F10 (option `GGML_*` du
/// backend GPU sur le ggml de llama.cpp). Retourne les noms de lien des
/// archives `ggml*` renommées (`tldl_ggml`, `tldl_ggml-base`, …).
fn build_llama(
    src: &Path,
    build_dir: &Path,
    tc: &Toolchain,
    gpu: Gpu,
) -> Result<Vec<String>, String> {
    let cmake = find_cmake()
        .ok_or("CMake introuvable. Installez-le (Windows : `winget install Kitware.CMake` ; macOS : `brew install cmake`).")?;
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest = Path::new(&manifest_dir);
    let out = Path::new(&out_dir);

    // Wrapper : projet parent minimal qui ajoute llama.cpp en sous-projet.
    let wrapper = out.join("llama-wrapper");
    std::fs::create_dir_all(&wrapper)
        .map_err(|e| format!("création du wrapper CMake impossible : {e}"))?;
    let f = |p: &Path| p.to_string_lossy().replace('\\', "/");
    // PAS de `CMAKE_C_STANDARD` ici : le `xxhash.c` du vendor/hash (compilé
    // en C) basculerait alors en C11, et le garde de `xxhash.h` inclut
    // `<stdalign.h>` — header C23 absent du SDK Windows < 10.0.22000. Sans
    // flag `-std`, MSVC reste en C par défaut et le garde bascule sur
    // `__declspec(align(n))`. Le ggml de llama.cpp pose lui-même
    // `CMAKE_C_STANDARD 11` dans son propre scope (sa ggml/CMakeLists.txt).
    let mut cml = String::from(
        "cmake_minimum_required(VERSION 3.14)\n\
         project(tldl_llama_wrapper C CXX)\n\
         set(CMAKE_CXX_STANDARD 17)\n\
         set(CMAKE_CXX_STANDARD_REQUIRED ON)\n\
         set(BUILD_SHARED_LIBS OFF)\n\
         if (MSVC)\n\
             set(CMAKE_MSVC_RUNTIME_LIBRARY_FLAGS \"/MT\")\n\
         endif()\n\
         ",
    );
    cml.push_str(&format!(
        "add_subdirectory(\"{}\" \"${{CMAKE_BINARY_DIR}}/llama-excl\")\n",
        f(src)
    ));
    cml.push_str(&format!(
        "add_library(tldl_llama_shim STATIC \"{}\")\n\
         target_link_libraries(tldl_llama_shim PRIVATE llama)\n",
        f(manifest.join("c/tldl_llama_shim.c").as_path())
    ));
    std::fs::write(wrapper.join("CMakeLists.txt"), cml)
        .map_err(|e| format!("écriture du wrapper CMake impossible : {e}"))?;

    let cfg = |name, val| format!("-D{}={}", name, val);
    let mut args: Vec<String> = vec![
        String::from("-S"),
        f(&wrapper),
        String::from("-B"),
        f(build_dir),
        cfg("CMAKE_BUILD_TYPE", "Release"),
        cfg("BUILD_SHARED_LIBS", "OFF"),
        // Non-standalone : ces options sont déjà OFF par défaut, mais on les
        // fige explicitement (pas de tools/tests/server/UI, pas d'OpenSSL).
        cfg("LLAMA_BUILD_COMMON", "OFF"),
        cfg("LLAMA_BUILD_TESTS", "OFF"),
        cfg("LLAMA_BUILD_TOOLS", "OFF"),
        cfg("LLAMA_BUILD_EXAMPLES", "OFF"),
        cfg("LLAMA_BUILD_SERVER", "OFF"),
        cfg("LLAMA_BUILD_APP", "OFF"),
        cfg("LLAMA_BUILD_UI", "OFF"),
        cfg("LLAMA_OPENSSL", "OFF"),
        cfg("LLAMA_SUBPROCESS", "OFF"),
        cfg("LLAMA_BUILD_IS_DEV", "OFF"),
        // ggml (llama.cpp) utilise OpenMP par défaut ; on coupe le runtime
        // OpenMP pour rester 100 % autonome (threadpool interne de ggml),
        // comme pour la build whisper.
        cfg("GGML_OPENMP", "OFF"),
        // ccache (WinLibs) est sur le PATH : ggml le pose en RULE_LAUNCH_COMPILE,
        // ce qui casse le scan de dépendances MSVC (ex. xxhash) et le parsing
        // d'arguments nvcc. Coupé pour un build reproductible, sans cache.
        cfg("GGML_CCACHE", "OFF"),
    ];
    match gpu {
        // F10 : le même gate que la build whisper (le binaire `tldl-llm`
        // embarque le backend GPU de SON ggml, renommé `tldl_ggml*`).
        Gpu::Cuda => {
            args.push(cfg("GGML_CUDA", "ON"));
            if let Some(a) = cuda_cudart_anchor()? {
                args.push(a);
            }
            if *tc == Toolchain::Gnu {
                let gpp = gnu_gpp().ok_or("TLDL_GPU=cuda : g++ du toolchain MinGW introuvable — requis comme compilateur hôte CUDA (CMAKE_CUDA_HOST_COMPILER).")?;
                let gpp_path = gpp.to_string_lossy().into_owned();
                args.push(cfg("CMAKE_CUDA_HOST_COMPILER", &gpp_path));
            }
        }
        Gpu::Metal => args.push(cfg("GGML_METAL", "ON")),
        Gpu::Vulkan => args.push(cfg("GGML_VULKAN", "ON")),
        // Le cache CMake est sticky : sans flag explicite, une config GPU
        // antérieure (GGML_CUDA=ON) persisterait au passage GPU → CPU et
        // casserait le lien (symbole `ggml_backend_cuda_reg` orphelin,
        // l'archive `ggml-cua` n'étant liée qu'au cfg `tldl_gpu_cuda`).
        Gpu::Cpu => {
            args.push(cfg("GGML_CUDA", "OFF"))
        }
    }
    if *tc == Toolchain::Gnu {
        args.push(String::from("-G"));
        args.push(String::from("Ninja"));
    }
    run_cmake(&cmake, &args, "configure llama.cpp", tc)?;

    let bargs: Vec<String> = vec![
        String::from("--build"),
        f(build_dir),
        String::from("--config"),
        String::from("Release"),
    ];
    run_cmake(&cmake, &bargs, "build llama.cpp", tc)?;

    // Renomme les archives ggml* (copies préfixées `tldl_`) et expose tous
    // les dossiers d'archives au lien Rust.
    let renamed = rename_ggml_archives(build_dir);
    for d in archive_dirs(build_dir) {
        println!("cargo:rustc-link-search=native={}", d.to_string_lossy());
    }
    Ok(renamed)
}

/// Tous les dossiers (sous `root`, y compris lui-même) contenant une archive
/// `.a`/`.lib` — chemin de recherche pour `rustc-link-search`.
fn archive_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            entries.push(e.path());
        }
    }
    for p in entries {
        if p.is_dir() {
            dirs.extend(archive_dirs(&p));
        } else if p.extension().is_some_and(|e| e == "a" || e == "lib") {
            if let Some(d) = p.parent() {
                let dp = d.to_path_buf();
                if !dirs.iter().any(|x| x == &dp) {
                    dirs.push(dp);
                }
            }
        }
    }
    dirs
}

/// Copie `src` vers `dst` en l'écrasant : les archives renommées sont posées
/// *in situ* à côté des originales ; un `cmake --build` incrémental recompile
/// les originales sans toucher aux copies — il faut resynchroniser à chaque
/// build, sous peine de lier contre une copie obsolète (ex. encore compilée
/// avec OpenMP : symboles `GOMP_*` / `omp_get_*` orphelins au lien final).
fn overwrite_copy(src: &Path, dst: &Path) {
    let _ = std::fs::remove_file(dst);
    let _ = std::fs::copy(src, dst);
}

/// Sous `dir`, copie chaque archive `ggml*.a`/`.lib` vers une variante
/// préfixée `tldl_` (sans écraser l'originale) et retourne les noms de lien,
/// triés (ordre de lien : `ggml` avant `ggml-base`, avant `ggml-cpu`, …).
fn rename_ggml_archives(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if !dir.exists() {
        return names;
    }
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            entries.push(e.path());
        }
    }
    for p in entries {
        if p.is_dir() {
            names.extend(rename_ggml_archives(&p));
            continue;
        }
        let (Some(stem), Some(ext)) =
            (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|e| e.to_str()))
        else {
            continue;
        };
        let canon = stem.strip_prefix("lib").unwrap_or(stem);
        if !(ext == "a" || ext == "lib") || !canon.starts_with("ggml") {
            continue;
        }
        let prefix = if ext == "a" { "lib" } else { "" };
        let copy = p.with_file_name(format!("{prefix}tldl_{canon}.{ext}"));
        overwrite_copy(&p, &copy);
        names.push(format!("tldl_{canon}"));
    }
    names.sort();
    names.dedup();
    names
}

/// Dossier des import libs CUDA (`cudart.lib`/`cublas.lib`/`cuda.lib` sous
/// `lib/x64` Windows, `libcudart.so`/`libcublas.so`/`libcuda.so` sous
/// `lib64` Linux, `lib` macOS) — `rustc-link-search` pour les `#[link]`
/// `cudart`/`cublas`/`cuda` des backends `ggml-cuda`/`tldl_ggml-cuda`.
fn cuda_lib_dir() -> Option<std::path::PathBuf> {
    let root = env::var_os("CUDA_PATH").or(env::var_os("CUDAToolkit_ROOT"))?;
    let dir = if cfg!(target_os = "windows") {
        Path::new(&root).join("lib/x64")
    } else if cfg!(target_os = "macos") {
        Path::new(&root).join("lib")
    } else {
        Path::new(&root).join("lib64")
    };
    if dir.exists() {
        Some(dir)
    } else {
        None
    }
}

/// Ancrage `CUDA_CUDART` pour `TLDL_GPU=cuda` sous Windows.
///
/// Avec le toolchain GNU (x86_64-pc-windows-gnu), `FindCUDAToolkit`
/// (CMake ≥ 4) ne cherche pas `lib/x64` : l'architecture cible `X8664`
/// n'est pas dans le jeu accepté par le module (`AMD64`/`x64`/`ARM64EC`),
/// donc `cudart.lib` est « introuvable » et le toolkit déclaré absent —
/// alors même que le toolkit est bien là. Pré-remplir `CUDA_CUDART`
/// (variable de cache `-D`) court-circuite le `find_library`
/// (comportement documenté : aucune recherche si la variable est déjà
/// posée), et le module en dérive tout le reste (`CUDAToolkit_LIBRARY_DIR`,
/// cibles `CUDA::*`). Hors Windows, pas d'ancrage : le module résout
/// `lib64` tout seul.
fn cuda_cudart_anchor() -> Result<Option<String>, String> {
    if !cfg!(target_os = "windows") {
        return Ok(None);
    }
    let root = env::var_os("CUDA_PATH")
        .or(env::var_os("CUDAToolkit_ROOT"))
        .ok_or("TLDL_GPU=cuda : CUDA_PATH (ou CUDAToolkit_ROOT) non défini — pointez-le sur le toolkit CUDA (ex. C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v13.5).")?;
    let cudart = Path::new(&root).join("lib/x64/cudart.lib");
    if !cudart.exists() {
        return Err(format!("TLDL_GPU=cuda : {cudart:?} introuvable — installation CUDA incomplet ?"));
    }
    Ok(Some(format!("-DCUDA_CUDART={}", cudart.to_string_lossy().into_owned())))
}

/// Configure + build de whisper.cpp en lib statique. `gpu` : gate F10
/// (option `GGML_*` du backend GPU sur le ggml de whisper.cpp).
fn build_whisper(
    src: &Path,
    build_dir: &Path,
    tc: &Toolchain,
    gpu: Gpu,
) -> Result<(), String> {
    let cmake = find_cmake()
        .ok_or("CMake introuvable. Installez-le (Windows : `winget install Kitware.CMake` ; macOS : `brew install cmake`) — requis pour compiler whisper.cpp.")?;

    let cfg = |name, val| {
        let mut a = vec![String::from("-D")];
        a.push(format!("{}={}", name, val));
        a.join("")
    };

    // Configure.
    let mut args: Vec<String> = vec![
        String::from("-S"),
        src.to_string_lossy().into_owned(),
        String::from("-B"),
        build_dir.to_string_lossy().into_owned(),
        cfg("CMAKE_BUILD_TYPE", "Release"),
        cfg("BUILD_SHARED_LIBS", "OFF"),
        cfg("WHISPER_BUILD_EXAMPLES", "OFF"),
        cfg("WHISPER_BUILD_TESTS", "OFF"),
        cfg("WHISPER_BUILD_SERVER", "OFF"),
        // Pas de runtime OpenMP à bundler (le CPU backend a sa propre threadpool).
        cfg("GGML_OPENMP", "OFF"),
        // ccache (WinLibs) est sur le PATH : ggml le pose en RULE_LAUNCH_COMPILE,
        // ce qui casse le scan de dépendances MSVC (ex. xxhash) et le parsing
        // d'arguments nvcc. Coupé pour un build reproductible, sans cache.
        cfg("GGML_CCACHE", "OFF"),
    ];
    // F10 : backend GPU activé au build (jamais par défaut — C3).
    match gpu {
        Gpu::Cuda => {
            args.push(cfg("GGML_CUDA", "ON"));
            if let Some(a) = cuda_cudart_anchor()? {
                args.push(a);
            }
            if *tc == Toolchain::Gnu {
                let gpp = gnu_gpp().ok_or("TLDL_GPU=cuda : g++ du toolchain MinGW introuvable — requis comme compilateur hôte CUDA (CMAKE_CUDA_HOST_COMPILER).")?;
                let gpp_path = gpp.to_string_lossy().into_owned();
                args.push(cfg("CMAKE_CUDA_HOST_COMPILER", &gpp_path));
            }
        }
        Gpu::Metal => args.push(cfg("GGML_METAL", "ON")),
        Gpu::Vulkan => args.push(cfg("GGML_VULKAN", "ON")),
        // Le cache CMake est sticky : sans flag explicite, une config GPU
        // antérieure (GGML_CUDA=ON) persisterait au passage GPU → CPU et
        // casserait le lien (symbole `ggml_backend_cuda_reg` orphelin,
        // l'archive `ggml-cua` n'étant liée qu'au cfg `tldl_gpu_cuda`).
        Gpu::Cpu => {
            args.push(cfg("GGML_CUDA", "OFF"))
        }
    }
    if *tc == Toolchain::Gnu {
        // `ninja` est fourni par WinLibs ; CMake le trouve sur le PATH
        // (préfixé ci-dessous par `run_cmake`).
        args.push(String::from("-G"));
        args.push(String::from("Ninja"));
    }
    run_cmake(&cmake, &args, "configure", tc)?;

    // Build (Release ; `--config` ignoré par les générateurs mono-config).
    let bargs: Vec<String> = vec![
        String::from("--build"),
        build_dir.to_string_lossy().into_owned(),
        String::from("--config"),
        String::from("Release"),
        String::from("--target"),
        String::from("whisper"),
    ];
    run_cmake(&cmake, &bargs, "build", tc)?;
    Ok(())
}

fn run_cmake(cmake: &Path, args: &[String], what: &str, tc: &Toolchain) -> Result<(), String> {
    let mut cmd = Command::new(cmake);
    for a in args {
        cmd.arg(a);
    }
    if *tc == Toolchain::Gnu {
        // Le toolchain MinGW doit être visible par CMake, ninja et gcc.
        if let Some(dir) = gnu_dir() {
            let mut p = dir.to_string_lossy().into_owned();
            p.push_str(&format!(";{}", env::var("PATH").unwrap_or_default()));
            cmd.env("PATH", p);
        }
    }
    let out = cmd
        .output()
        .map_err(|e| format!("lancement de cmake ({}) impossible : {}", what, e))?;
    if !out.status.success() {
        return Err(format!(
            "cmake ({}) a échoué :\n{}",
            what,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Séparateur de `PATH` : `;` (Windows), `:` (macOS).
fn path_sep() -> char {
    if env::consts::OS == "macos" {
        ':'
    } else {
        ';'
    }
}

/// CMake : variable `TLDL_CMAKE`, PATH, puis l'emplacement winget standard.
fn find_cmake() -> Option<std::path::PathBuf> {
    if let Ok(p) = env::var("TLDL_CMAKE") {
        let pb = Path::new(&p).to_path_buf();
        if pb.exists() {
            return Some(pb);
        }
    }
    let exe = format!("cmake{}", std::env::consts::EXE_SUFFIX);
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(path_sep()) {
            let c = Path::new(&dir).join(&exe);
            if c.exists() {
                return Some(c);
            }
        }
    }
    let guess = format!(
        "C:\\Program Files\\CMake\\bin\\{}",
        exe
    );
    let gp = Path::new(&guess);
    if gp.exists() {
        return Some(gp.to_path_buf());
    }
    None
}

/// Dossiers `bin` candidats pour un toolchain MinGW/WinLibs.
fn gnu_candidates() -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = env::var("TLDL_MINGW_DIR") {
        v.push(Path::new(&d).to_path_buf());
    }
    let exe = format!("x86_64-w64-mingw32-g++{}", std::env::consts::EXE_SUFFIX);
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(path_sep()) {
            let d = Path::new(&dir).to_path_buf();
            if d.join(&exe).exists() {
                v.push(d);
            }
        }
    }
    // Emplacement courant de ce poste (junction vers le paquet winget WinLibs).
    let guess = Path::new(r"C:\Users\jeanp\tools\mingw64");
    if guess.join(&exe).exists()
        || guess.join(format!("g++{}", std::env::consts::EXE_SUFFIX)).exists()
    {
        v.push(guess.to_path_buf());
    }
    v
}

fn gnu_dir() -> Option<std::path::PathBuf> {
    gnu_candidates().into_iter().find(|d| d.exists())
}

/// Compilateur C (gcc) du toolchain MinGW, s'il existe.
fn gnu_gcc() -> Option<std::path::PathBuf> {
    let d = gnu_dir()?;
    let gcc_exe = format!("gcc{}", std::env::consts::EXE_SUFFIX);
    let names = [
        "gcc",
        "x86_64-w64-mingw32-gcc",
        gcc_exe.as_str(),
    ];
    for n in names {
        let p = d.join(format!("{}{}", n, std::env::consts::EXE_SUFFIX));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Compilateur C++ (g++) du toolchain MinGW, s'il existe.
/// `nvcc` ne connaît pas le toolchain GNU : sans `CMAKE_CUDA_HOST_COMPILER`,
/// il cherche `cl.exe` (MSVC) et l'identification CUDA échoue.
fn gnu_gpp() -> Option<std::path::PathBuf> {
    let d = gnu_dir()?;
    let gpp_exe = format!("g++{}", std::env::consts::EXE_SUFFIX);
    let names = [
        "g++",
        "x86_64-w64-mingw32-g++",
        gpp_exe.as_str(),
    ];
    for n in names {
        let p = d.join(format!("{}{}", n, std::env::consts::EXE_SUFFIX));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Un toolchain C++ utilisable existe-t-il (Apple, MSVC ou MinGW) ?
fn find_toolchain() -> Option<Toolchain> {
    // Apple (macOS) : le compilateur système `/usr/bin/c++` (clang++) —
    // Xcode Command Line Tools. La build se fait sur le Mac (C2) ; pas de
    // Ninja (le générateur CMake par défaut, Unix Makefiles, suffit).
    if env::consts::OS == "macos" {
        if Path::new("/usr/bin/c++").exists() {
            return Some(Toolchain::Apple);
        }
        return None;
    }
    // MSVC : cl.exe sur le PATH (Developer Command Prompt) ou une MSVC v14x en place.
    let cl = format!("cl{}", std::env::consts::EXE_SUFFIX);
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(path_sep()) {
            if Path::new(&dir).join(&cl).exists() {
                return Some(Toolchain::Msvc);
            }
        }
    }
    let roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
        r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC",
        r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC",
    ];
    for root in roots {
        if let Ok(rd) = std::fs::read_dir(root) {
            for _e in rd.flatten() {
                return Some(Toolchain::Msvc);
            }
        }
    }
    // MinGW / WinLibs.
    if gnu_dir().is_some() {
        return Some(Toolchain::Gnu);
    }
    None
}

/// Collecte sous `dir` les archives CMake (whisper, ggml, ggml-base,
/// ggml-cpu). Retourne (stem canonique, chemin) — préfixe `lib` retiré.
fn collect_link_libs(dir: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    walk_collect(dir, &mut out);
    out
}

fn walk_collect(dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            entries.push(e.path());
        }
    }
    for p in entries {
        if p.is_dir() {
            walk_collect(&p, out);
        } else if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            let canon = stem.strip_prefix("lib").unwrap_or(stem);
            // F10 : les stems GPU ne sont trouvés que si `TLDL_GPU` a fait
            // compiler l'archive correspondante (sinon absente du build).
            if matches!(
                canon,
                "whisper" | "ggml" | "ggml-base" | "ggml-cpu"
                    | "ggml-cuda" | "ggml-metal" | "ggml-vulkan"
            )
            && p.extension().is_some_and(|e| e == "a" || e == "lib")
            {
                out.push((canon.to_string(), p));
            }
        }
    }
}
