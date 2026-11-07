//! Persistent, event-sourced long-term history.
//!
//! The log (`store.rs`) records only actual changes — a poll that changes
//! nothing costs zero bytes. `track.rs` diffs live snapshots into price and
//! once-per-day market telemetry events and rebuilds queryable per-model
//! series. `ui.rs` renders the History tab over those series. See each module's
//! docs for the encoding and recording contracts.

pub(crate) mod store;
pub(crate) mod track;
pub(crate) mod ui;

pub(crate) use track::HistoryTracker;
pub(crate) use ui::HistoryUiState;

pub(crate) fn history_log_path() -> std::path::PathBuf {
    data_dir().join("history.bin")
}

/// Best-effort persistence for History tab view state (selected comparison
/// models), so a restart reopens the same overlay. (Named for what it holds
/// — not the same file as the app UI state's `ui_state.json`.)
pub(crate) fn history_selection_path() -> std::path::PathBuf {
    data_dir().join("history-ui.json")
}

fn data_dir() -> std::path::PathBuf {
    let dir = crate::persist::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
