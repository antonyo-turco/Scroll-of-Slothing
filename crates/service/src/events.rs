use serde::Serialize;

use crate::jobs::JobStatus;

/// Events broadcast to the UI (via Tauri events) and other listeners.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A job started, progressed or finished.
    Job(JobStatus),
    /// Persistent data changed (from this process or another one).
    StoreChanged { entity: String, entity_id: String, op: String },
}
