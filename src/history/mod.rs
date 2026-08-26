//! Persistent, event-sourced long-term history.
//!
//! The log (`store.rs`) records only actual changes — a poll that changes
//! nothing costs zero bytes. `track.rs` diffs live snapshots into events and
//! rebuilds queryable per-model series, including composite capability /
//! deployment scores and a once-per-day market telemetry summary. `ui.rs`
//! renders the History tab over those series. See each module's docs for the
//! encoding and recording contracts.

pub(crate) mod store;
pub(crate) mod track;
pub(crate) mod ui;

pub(crate) use track::HistoryTracker;
pub(crate) use ui::HistoryUiState;

pub(crate) fn history_log_path() -> std::path::PathBuf {
    data_dir().join("history.bin")
}

/// Best-effort persistence for History tab view state (selected comparison
/// models), so a restart reopens the same overlay.
pub(crate) fn ui_state_path() -> std::path::PathBuf {
    data_dir().join("history-ui.json")
}

fn data_dir() -> std::path::PathBuf {
    use directories::ProjectDirs;
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        let dir = project.data_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.to_path_buf()
    } else {
        std::path::PathBuf::from(".")
    }
}
