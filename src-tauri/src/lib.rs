mod config;

use tauri::Manager;

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::Config, String> {
    config::read(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: config::Config) -> Result<(), String> {
    config::write(&app, &config).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_keyring::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
