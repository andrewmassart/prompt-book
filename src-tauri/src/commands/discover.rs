use std::path::PathBuf;

use walkdir::WalkDir;

use crate::error::AppError;
use crate::model::{SessionSource, SessionSummary};
use crate::parser::claude::scan_claude_summary;
use crate::parser::codex::scan_codex_summary;
use crate::parser::copilot::scan_copilot_summary;

#[tauri::command]
pub async fn discover_sessions() -> Result<Vec<SessionSummary>, String> {
    discover_sessions_inner().map_err(|e| e.to_string())
}

fn discover_sessions_inner() -> Result<Vec<SessionSummary>, AppError> {
    let mut summaries = collect_claude_sessions();
    summaries.extend(collect_copilot_sessions());
    summaries.extend(collect_codex_sessions());
    sort_newest_first(&mut summaries);
    Ok(summaries)
}

fn collect_claude_sessions() -> Vec<SessionSummary> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let claude_dir = home.join(".claude").join("projects");
    if !claude_dir.exists() {
        return Vec::new();
    }

    find_jsonl_files(&claude_dir)
        .into_iter()
        .filter_map(|path| build_claude_summary(path).ok())
        .collect()
}

fn collect_copilot_sessions() -> Vec<SessionSummary> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let copilot_dir = home.join(".copilot").join("session-state");
    if !copilot_dir.exists() {
        return Vec::new();
    }

    find_jsonl_files(&copilot_dir)
        .into_iter()
        .filter_map(|path| build_copilot_summary(path).ok())
        .collect()
}

fn collect_codex_sessions() -> Vec<SessionSummary> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let codex_dir = home.join(".codex").join("sessions");
    if !codex_dir.exists() {
        return Vec::new();
    }

    find_jsonl_files(&codex_dir)
        .into_iter()
        .filter_map(|path| build_codex_summary(path).ok())
        .collect()
}

fn find_jsonl_files(dir: &std::path::Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_jsonl(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn is_jsonl(path: &std::path::Path) -> bool {
    path.extension().map_or(false, |ext| ext == "jsonl")
}

fn sort_newest_first(summaries: &mut [SessionSummary]) {
    summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
}

fn build_claude_summary(path: PathBuf) -> Result<SessionSummary, AppError> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (title, model, started_at, message_count) = scan_claude_summary(&path)?;

    Ok(SessionSummary {
        id,
        source: SessionSource::ClaudeCode,
        path,
        title,
        started_at,
        message_count,
        model,
    })
}

fn build_codex_summary(path: PathBuf) -> Result<SessionSummary, AppError> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (title, model, started_at, message_count) = scan_codex_summary(&path)?;

    Ok(SessionSummary {
        id,
        source: SessionSource::Codex,
        path,
        title,
        started_at,
        message_count,
        model,
    })
}

fn build_copilot_summary(path: PathBuf) -> Result<SessionSummary, AppError> {
    let id = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (title, model, started_at, message_count) = scan_copilot_summary(&path)?;

    Ok(SessionSummary {
        id,
        source: SessionSource::CopilotCli,
        path,
        title,
        started_at,
        message_count,
        model,
    })
}
