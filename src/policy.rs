//! Policy: ngân sách, cảnh báo ngữ cảnh, xoay vòng thợ OpenCode (spec §5.7, §5.9).

use anyhow::Result;

use crate::store::Store;

/// Lý do từ chối spawn nếu ngân sách cứng của đầu việc đã vượt (Task 8).
pub fn spawn_blocked_by_budget(_store: &Store, _task: &str) -> Result<Option<String>> {
    Ok(None)
}
