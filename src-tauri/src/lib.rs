mod commands;
mod error;
mod model;
mod parser;

use commands::discover::discover_sessions;
use commands::export::export_html;
use commands::parse::{parse_content, parse_dropped_file, parse_session};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            discover_sessions,
            parse_session,
            parse_dropped_file,
            parse_content,
            export_html,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
