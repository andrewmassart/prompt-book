pub mod claude;
pub mod copilot;
pub mod detect;

use crate::model::{ContentBlock, Message};

pub fn calculate_durations(messages: &mut [Message]) {
    for i in 1..messages.len() {
        if messages[i].duration_ms.is_some() {
            continue;
        }
        let prev_ts = messages[i - 1].timestamp.as_deref();
        let curr_ts = messages[i].timestamp.as_deref();
        if let (Some(prev), Some(curr)) = (prev_ts, curr_ts) {
            if let Some(ms) = duration_between(prev, curr) {
                messages[i].duration_ms = Some(ms);
            }
        }
    }
    calculate_tool_durations(messages);
}

fn calculate_tool_durations(messages: &mut [Message]) {
    for i in 0..messages.len() {
        let msg_ts = messages[i].timestamp.as_deref().map(String::from);
        let next_ts = messages.get(i + 1).and_then(|m| m.timestamp.as_deref()).map(String::from);

        let Some(from) = msg_ts else { continue };
        let Some(to) = next_ts else { continue };

        for block in messages[i].content.iter_mut() {
            if let ContentBlock::ToolUse { duration_ms, .. } = block {
                if duration_ms.is_none() {
                    *duration_ms = duration_between(&from, &to);
                }
            }
        }
    }
}

fn duration_between(from: &str, to: &str) -> Option<u64> {
    let from_dt = from.parse::<chrono::DateTime<chrono::Utc>>().ok()?;
    let to_dt = to.parse::<chrono::DateTime<chrono::Utc>>().ok()?;
    let diff = to_dt.signed_duration_since(from_dt);
    if diff.num_milliseconds() > 0 {
        Some(diff.num_milliseconds() as u64)
    } else {
        None
    }
}
