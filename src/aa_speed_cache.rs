//! Disk cache for the AA standardized-speed table. The table rides the same
//! six-hour leaderboard fetch as the Intelligence Index; persisting it means
//! a restart starts from the previous session's speed columns instead of
//! waiting for the next fetch.
//!
//! Best-effort like the other caches: read errors fall back to "no data"
//! (columns degrade to the live-market telemetry) and write errors are
//! swallowed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::persist::{cache_file, load_versioned, save_versioned};
use crate::types::AaSpeedSnapshot;

const CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    /// UTC timestamp of the last successful save, for status display.
    saved_at: String,
    snapshot: AaSpeedSnapshot,
}

pub(crate) fn cache_path() -> PathBuf {
    cache_file("aa-speed.json")
}

/// Load the cached speed table. Returns the snapshot plus the save
/// timestamp; `None` on any problem so callers fall back to a fresh fetch.
pub(crate) fn load_aa_speed_cache() -> Option<(AaSpeedSnapshot, String)> {
    let file: CacheFile = load_versioned(&cache_path(), |f: &CacheFile| {
        f.version == CACHE_VERSION && !f.snapshot.entries.is_empty()
    })?;
    Some((file.snapshot, file.saved_at))
}

/// Persist the current snapshot.
pub(crate) fn save_aa_speed_cache(snapshot: &AaSpeedSnapshot) {
    save_versioned(&cache_path(), |saved_at| CacheFile {
        version: CACHE_VERSION,
        saved_at,
        snapshot: snapshot.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AaSpeedEntry;

    fn sample() -> AaSpeedSnapshot {
        AaSpeedSnapshot {
            entries: vec![AaSpeedEntry {
                key: "claude opus 5 5".into(),
                name: "Claude Opus 5.5".into(),
                creator: "Anthropic".into(),
                effort: Some("max".into()),
                median_tps: Some(61.2),
                tps_p05: Some(50.1),
                tps_p25: Some(55.0),
                tps_p75: Some(70.0),
                tps_p95: Some(80.9),
                median_ttft_s: Some(0.68),
                ttft_p95_s: Some(1.4),
                median_e2e_s: Some(5.1),
            }],
            fetched_at: chrono::Utc::now(),
        }
    }

    #[test]
    // One test, sequentially: both halves share the real cache path, and
    // separate #[test]s would race on that file under the parallel runner.
    fn cache_round_trips_and_tolerates_corruption() {
        let path = cache_path();
        let snapshot = sample();
        save_aa_speed_cache(&snapshot);
        let (loaded, saved_at) = load_aa_speed_cache().expect("cache loads after save");
        assert_eq!(loaded, snapshot);
        assert!(!saved_at.is_empty());
        // A corrupt file is ignored rather than fatal.
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(load_aa_speed_cache().is_none());
        // So is a missing one.
        let _ = std::fs::remove_file(&path);
        assert!(load_aa_speed_cache().is_none());
    }
}
