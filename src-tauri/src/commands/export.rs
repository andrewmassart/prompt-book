use std::fs;

use crate::model::Session;

const EXPORT_TEMPLATE: &str = include_str!("../../assets/export-template.html");

#[tauri::command]
/// Exports a parsed session to a standalone HTML file.
pub async fn export_html(session: Session, output_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || export_html_inner(session, output_path))
        .await
        .map_err(|e| format!("Task join error: {e}"))?
}

fn export_html_inner(session: Session, output_path: String) -> Result<(), String> {
    let session_json =
        serde_json::to_string(&session).map_err(|e| format!("Serialization error: {e}"))?;
    let html = EXPORT_TEMPLATE.replace("{{SESSION_DATA}}", &session_json);
    fs::write(&output_path, html).map_err(|e| format!("Write error: {e}"))
}
