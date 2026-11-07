//! Disk cache for fetched benchmark boards. Restarting the app used to lose
//! every board until the next six-hour refresh (only the bundled AA snapshot
//! survived); this file persists the last good set per source so a restart
//! starts from the previous session's boards and the periodic refresh
//! replaces them in place.
//!
//! The cache is best-effort by design: any read error (missing, corrupt,
//! version drift) is ignored — the app falls back to the bundled snapshot and
//! fresh fetches — and any write error is swallowed the same way, because the
//! cost of a stale chart is zero next to the cost of a dead app.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::persist::{cache_file, load_versioned, save_versioned};
use crate::types::{Benchmark, BenchmarkSource};

// Bumped to 2 in 0.32.0 when Terminal-Bench 3.0, SWE-rebench, and Revelo
// Code Index left BenchmarkSource: a cached file that still names them would
// fail to deserialize (the whole cache is then ignored, by design), and an
// explicit bump keeps that cold refresh deliberate rather than incidental.
const CACHE_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    /// UTC timestamp of the last successful save, for status display.
    saved_at: String,
    sources: HashMap<BenchmarkSource, Vec<Benchmark>>,
}

pub(crate) fn cache_path() -> PathBuf {
    cache_file("benchmarks.json")
}

/// Load the cached boards. Returns the per-source sets plus the save
/// timestamp; `None` on any problem so callers fall back to a fresh fetch.
pub(crate) fn load_benchmark_cache() -> Option<(HashMap<BenchmarkSource, Vec<Benchmark>>, String)> {
    let file: CacheFile = load_versioned(&cache_path(), |f: &CacheFile| {
        f.version == CACHE_VERSION && !f.sources.is_empty()
    })?;
    Some((file.sources, file.saved_at))
}

/// Persist the current boards.
pub(crate) fn save_benchmark_cache(sets: &HashMap<BenchmarkSource, Vec<Benchmark>>) {
    save_versioned(&cache_path(), |saved_at| CacheFile {
        version: CACHE_VERSION,
        saved_at,
        sources: sets.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BenchmarkKind;
    use crate::types::score_benchmark;

    fn sample_sets() -> HashMap<BenchmarkSource, Vec<Benchmark>> {
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::TerminalBench4,
            vec![score_benchmark(
                "Beam 1.0",
                48.0,
                Some("mini-swe-agent".into()),
                None,
                BenchmarkKind::ModelAgent,
            )],
        );
        sets
    }

    #[test]
    // One test, sequentially: both halves share the real cache path, and
    // separate #[test]s would race on that file under the parallel runner.
    fn cache_round_trips_and_tolerates_corruption() {
        let path = cache_path();
        let sets = sample_sets();
        save_benchmark_cache(&sets);
        let (loaded, saved_at) = load_benchmark_cache().expect("cache loads after save");
        assert_eq!(
            loaded
                .get(&BenchmarkSource::TerminalBench4)
                .map(|rows| rows.len()),
            Some(1)
        );
        assert!(!saved_at.is_empty());
        // A corrupt file is ignored rather than fatal.
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(load_benchmark_cache().is_none());
        // So is a missing one.
        let _ = std::fs::remove_file(&path);
        assert!(load_benchmark_cache().is_none());
    }
}
