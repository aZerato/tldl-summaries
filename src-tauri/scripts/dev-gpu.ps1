# dev-gpu.ps1 — lance l'app (dev) sur la build GPU (CUDA) sans re-saisir l'env.
#
# Pourquoi ce wrapper : `npm run tauri dev` relance un `cargo build` interne
# (le binaire `tldl`) qui compile whisper.cpp/llama.cpp via CMake — il a
# besoin de l'env MSVC (cl.exe/nmake, INCLUDE/LIB) + TLDL_GPU/CUDA_PATH.
# Un shell nu ne les a pas → CMake « nmake not found » (build script exit 101).
#
# Ce script :
#   1. recharge l'env VS capturé (`vscmd_env.txt`, à côté du script) :
#      cl/nmake/INCLUDE/LIB,
#   2. fixe le gate F10 (TLDL_GPU=cuda, CUDA_PATH),
#   3. met le `bin\x64` CUDA (cublas64_13/cudart64_13) sur le PATH *runtime*
#      — sinon STATUS_DLL_NOT_FOUND au boot de l'app,
#   4. démarre `npm run tauri dev`.
#
# Usage :  npm run dev:gpu      (ou : powershell -File dev-gpu.ps1)
$ErrorActionPreference = 'Stop'

$tauriDir = Split-Path -Parent $PSScriptRoot    # ...\src-tauri
$repoRoot = Split-Path -Parent $tauriDir       # ...\tldl-summaries
$envFile  = Join-Path $PSScriptRoot 'vscmd_env.txt'

if (-not (Test-Path $envFile)) {
    throw "vscmd_env.txt introuvable dans $PSScriptRoot. Capture l'env VS (voir docs/architecture.md §10) et dépose-le ici."
}

# 1. Env VS (cl/nmake/INCLUDE/LIB...).
$envs = @{}
foreach ($line in [IO.File]::ReadAllLines($envFile)) {
    if ($line -match '^([A-Za-z_][A-Za-z0-9_]*)=(.*)$') { $envs[$Matches[1]] = $Matches[2] }
}
foreach ($k in $envs.Keys) { [Environment]::SetEnvironmentVariable($k, $envs[$k], 'Process') }
# CMake MSVC discovery (defensive).
$env:VS70COMNTOOLS = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\'

# 2. Gate F10 (GPU) + toolkit CUDA.
$env:TLDL_GPU  = 'cuda'
$env:CUDA_PATH = 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.5'

# 3. DLL runtime CUDA : `bin\x64` (pas `bin`, qui n'a que nvcc/ptxas).
#    IMPORTANT : l'env VS (ci-dessus) ÉCRASE le PATH du shell → il faut
#    ré-apporter bin\x64 ici, sinon STATUS_DLL_NOT_FOUND au boot de l'app.
#    Le subprocess `tldl-llm` hérite de ce PATH (Command sans env_clear).
$env:PATH = 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.5\bin\x64;' + $env:PATH

# 4. Lancer l'app.
Set-Location $repoRoot
npm run tauri dev
