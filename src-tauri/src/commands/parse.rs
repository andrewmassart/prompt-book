use std::path::Path;

use crate::error::AppError;
use crate::model::Session;
use crate::model::SessionSource;
use crate::parser::claude::parse_claude_session;
use crate::parser::copilot::parse_copilot_session;
use crate::parser::detect::{detect_format, detect_format_from_content};

#[tauri::command]
pub async fn parse_session(path: String) -> Result<Session, String> {
    parse_session_inner(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn parse_dropped_file(path: String) -> Result<Session, String> {
    parse_session_inner(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn parse_content(filename: String, content: String) -> Result<Session, String> {
    parse_content_inner(&filename, &content).map_err(|e| e.to_string())
}

fn parse_session_inner(path: &str) -> Result<Session, AppError> {
    let path = Path::new(path);
    let source = detect_format(path)?;

    match source {
        SessionSource::ClaudeCode => parse_claude_session(path),
        SessionSource::CopilotCli => parse_copilot_session(path),
    }
}

fn parse_content_inner(filename: &str, content: &str) -> Result<Session, AppError> {
    let source = detect_format_from_content(content)?;
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(filename);
    std::fs::write(&temp_path, content)?;

    let result = match source {
        SessionSource::ClaudeCode => parse_claude_session(&temp_path),
        SessionSource::CopilotCli => parse_copilot_session(&temp_path),
    };

    std::fs::remove_file(&temp_path).ok();
    result
}
