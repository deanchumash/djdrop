mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;

use tauri::Manager;

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::Config, String> {
    config::read(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: config::Config) -> Result<(), String> {
    config::write(&app, &config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_download(app: tauri::AppHandle, id: String, input: String) -> Result<(), String> {
    let cfg = config::read(&app).map_err(|e| e.to_string())?;
    let source = router::route(&input);
    let output_dir = cfg.output_dir.clone();
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    tokio::spawn(downloader::run_download(app, id, source, input, output_dir));
    Ok(())
}

#[tauri::command]
async fn save_credential(pool: String, username: String, password: String) -> Result<(), String> {
    credentials::store_in_keychain(&pool, &username, &password)
}

#[tauri::command]
async fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            start_download,
            save_credential,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
