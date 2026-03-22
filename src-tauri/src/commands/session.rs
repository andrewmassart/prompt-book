use std::path::Path;

use crate::error::AppError;
use crate::model::Session;
use crate::parser::detect::{detect_format, detect_format_from_content};
use crate::parser::parser_for;

#[tauri::command]
/// Parses a session file at the given path, auto-detecting the format.
pub async fn parse_session(path: String) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || parse_from_path(&path))
        .await
        .map_err(|e| format!("Task join error: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// Parses a drag-and-dropped session file.
pub async fn parse_dropped_file(path: String) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || parse_from_path(&path))
        .await
        .map_err(|e| format!("Task join error: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// Parses raw JSONL content with format auto-detection.
pub async fn parse_content(filename: String, content: String) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = detect_format_from_content(&content)?;
        parser_for(source).parse_content(&filename, &content)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
    .map_err(|e| e.to_string())
}

fn parse_from_path(path: &str) -> Result<Session, AppError> {
    let path = Path::new(path);
    let source = detect_format(path)?;
    parser_for(source).parse(path)
}
