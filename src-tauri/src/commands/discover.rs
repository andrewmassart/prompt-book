use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::model::SessionSummary;
use crate::parser::{self, SessionParser};

#[tauri::command]
/// Discovers and returns summaries of sessions from all supported AI tools.
pub async fn discover_sessions() -> Result<Vec<SessionSummary>, String> {
    tauri::async_runtime::spawn_blocking(discover_all)
        .await
        .map_err(|e| format!("Task join error: {e}"))?
}

fn discover_all() -> Result<Vec<SessionSummary>, String> {
    let mut summaries: Vec<SessionSummary> = parser::parsers()
        .iter()
        .flat_map(|&p| collect_sessions(p))
        .collect();
    summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(summaries)
}

fn collect_sessions(parser: &dyn SessionParser) -> Vec<SessionSummary> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let dir = parser.home_subpath().iter().fold(home, |acc, part| acc.join(part));
    if !dir.exists() {
        return Vec::new();
    }

    find_jsonl_files(&dir)
        .into_iter()
        .filter_map(|path| parser::build_summary(parser, path).ok())
        .collect()
}

fn find_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some("jsonl".as_ref()))
        .map(|e| e.path().to_path_buf())
        .collect()
}
