//! Where the core sends live events (chat streaming, titles, rate limits). The desktop forwards
//! them to its window as Tauri events; the CLI feeds them to its terminal UI.

use serde_json::Value;
use std::sync::Arc;

pub trait Sink: Send + Sync + 'static {
    fn emit(&self, kind: &str, payload: Value);
}

pub type Events = Arc<dyn Sink>;
