#![cfg(target_family = "wasm")]

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WarpEvent {
    LoggedOut,
    SessionJoined,
    ErrorLogged { error: String },
    OpenOnNative { url: String },
    ThemeBackgroundChanged { color: String },
}

pub fn emit_event(_event: WarpEvent) {}
