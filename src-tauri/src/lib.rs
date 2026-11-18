pub mod asr;
mod commands;
mod config;
mod llm;
mod media;
pub mod models;
pub mod pipeline;
pub mod storage;

use std::sync::Mutex;

pub struct AppState {
    pub storage: Mutex<storage::Storage>,
    pub config: Mutex<config::Config>,
}

pub fn run() {
    let storage = storage::Storage::open(&models::app_data_dir())
        .expect("échec d'initialisation du stockage SQLite");
    let config = config::Config::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            storage: Mutex::new(storage),
            config: Mutex::new(config),
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_media,
            commands::run_pipeline,
            commands::list_models,
            commands::download_model,
            commands::cancel_model_download,
            commands::delete_model,
            commands::list_history,
            commands::get_history,
            commands::search_history,
            commands::get_config,
            commands::set_config,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de Tauri");
}
