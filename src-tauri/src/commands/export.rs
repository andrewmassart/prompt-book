use std::fs;

use crate::model::Session;

const EXPORT_TEMPLATE: &str = include_str!("../../assets/export-template.html");

#[tauri::command]
pub async fn export_html(session: Session, output_path: String) -> Result<(), String> {
    let session_json =
        serde_json::to_string(&session).map_err(|e| format!("Serialization error: {e}"))?;

    let html = EXPORT_TEMPLATE.replace("{{SESSION_DATA}}", &session_json);

    fs::write(&output_path, html).map_err(|e| format!("Write error: {e}"))?;

    Ok(())
}
