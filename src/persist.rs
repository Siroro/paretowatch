//! Persistence plumbing shared by every file the app writes: the app's
//! config/data/cache directories, the atomic file write, and the JSON disk
//! cache the fetched-feed caches share.
//!
//! Writes are best-effort by design for cache-like files (a failed save only
//! costs a re-fetch) and reported for settings-like files; either way the
//! write itself is atomic (temp file + rename) so a crash mid-write can
//! never leave a truncated file behind.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

fn project() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch")
}

/// The user data directory, when the OS provides one.
pub(crate) fn data_dir() -> Option<PathBuf> {
    project().map(|p| p.data_dir().to_path_buf())
}

/// One of the app's files, by directory kind. When the OS gives us no home
/// directory the fallback is a flat file in the working directory, named
/// `paretowatch-<name>` with underscores dashed (the historical convention).
fn app_file(dir: fn(&directories::ProjectDirs) -> &Path, name: &str) -> PathBuf {
    if let Some(project) = project() {
        dir(&project).join(name)
    } else {
        PathBuf::from(format!("paretowatch-{}", name.replace('_', "-")))
    }
}

/// Files under the user config directory (`config.json`, `ui_state.json`,
/// `alert_state.json`).
pub(crate) fn config_file(name: &str) -> PathBuf {
    app_file(|p| p.config_dir(), name)
}

/// Files under the user data directory (`notifications.json`, history).
pub(crate) fn data_file(name: &str) -> PathBuf {
    app_file(|p| p.data_dir(), name)
}

/// Files under the user cache directory (`benchmarks.json`,
/// `performance.json`, `aa-speed.json`).
pub(crate) fn cache_file(name: &str) -> PathBuf {
    app_file(|p| p.cache_dir(), name)
}

/// Atomic file write: bytes land in a temp file next to the target, then a
/// rename swaps it in. If the target exists and the rename fails (Windows
/// denies rename-over-open-files), the live file is archived to
/// `<name>.old-<millis>` and the rename retried — the previous contents are
/// never deleted first. The temp file is cleaned up on failure.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(first) => {
            // The archive keeps the file's full name and appends, so
            // `notifications.json` archives as `notifications.json.old-…`.
            let mut aside_name = path.file_name().unwrap_or_default().to_os_string();
            aside_name.push(format!(".old-{}", chrono::Utc::now().timestamp_millis()));
            let aside = path.with_file_name(aside_name);
            let retried = std::fs::rename(path, &aside).and_then(|()| std::fs::rename(&tmp, path));
            if retried.is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
            retried.map_err(|_| first)
        }
    }
}

/// The `saved_at` stamp every cache file carries: local wall-clock text of
/// the last successful save, UTC-formatted for the status display.
pub(crate) fn cache_stamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Load one versioned JSON cache file. `None` on any problem — missing,
/// corrupt, or failing the caller's validity check — so callers fall back to
/// fresh data. The caller owns the wrapper struct, so each cache keeps its
/// on-disk field layout.
pub(crate) fn load_versioned<F: DeserializeOwned>(
    path: &Path,
    valid: impl Fn(&F) -> bool,
) -> Option<F> {
    let bytes = std::fs::read(path).ok()?;
    let file: F = serde_json::from_slice(&bytes).ok()?;
    valid(&file).then_some(file)
}

/// Save one versioned JSON cache file atomically. Best effort: failures are
/// swallowed (the next successful save heals the file). `build` receives the
/// fresh `saved_at` stamp so the wrapper keeps its on-disk field layout.
pub(crate) fn save_versioned<F: Serialize>(path: &Path, build: impl FnOnce(String) -> F) {
    if let Ok(bytes) = serde_json::to_vec(&build(cache_stamp())) {
        let _ = atomic_write(path, &bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("paretowatch-persist-tests")
            .join(format!("{}-{name}", std::process::id()))
    }

    #[test]
    fn atomic_write_round_trips_and_never_deletes_the_live_file() {
        let path = temp_path("atomic.json");
        let _ = std::fs::remove_file(&path);
        atomic_write(&path, b"first").expect("first write");
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        atomic_write(&path, b"second").expect("overwrite");
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        // No temp leftovers on success.
        assert!(!path.with_extension("tmp").exists());
        let _ = std::fs::remove_file(&path);
    }

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Clone)]
    struct TestFile {
        version: u32,
        saved_at: String,
        items: Vec<String>,
    }

    const TEST_VERSION: u32 = 3;

    fn load_test(path: &Path) -> Option<TestFile> {
        load_versioned(path, |f: &TestFile| f.version == TEST_VERSION)
    }

    #[test]
    fn json_cache_round_trips_and_rejects_drift() {
        let path = temp_path("cache.json");
        let _ = std::fs::remove_file(&path);
        save_versioned(&path, |saved_at| TestFile {
            version: TEST_VERSION,
            saved_at,
            items: vec!["a".into(), "b".into()],
        });
        let file = load_test(&path).expect("loads");
        assert_eq!(file.items, vec!["a", "b"]);
        // The stamp is UTC (matching the sibling caches), so it must sit
        // within a minute of the clock — a local-time stamp would drift by
        // the timezone offset.
        let stamped = chrono::NaiveDateTime::parse_from_str(&file.saved_at, "%Y-%m-%d %H:%M")
            .expect("saved_at parses as %Y-%m-%d %H:%M");
        let drift_minutes = (chrono::Utc::now().naive_utc() - stamped)
            .num_minutes()
            .abs();
        assert!(drift_minutes <= 1, "saved_at is not UTC: {}", file.saved_at);
        // A wrong version is ignored.
        let mut wrong = file.clone();
        wrong.version = 4;
        save_versioned(&path, |_| wrong);
        assert!(load_test(&path).is_none());
        // So is a corrupt file, and a missing one.
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(load_test(&path).is_none());
        let _ = std::fs::remove_file(&path);
        assert!(load_test(&path).is_none());
    }
}
