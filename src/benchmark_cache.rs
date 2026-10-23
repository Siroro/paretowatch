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

use std::{collections::HashMap, fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::types::{Benchmark, BenchmarkSource};

const CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    /// Wall-clock timestamp of the last successful save, for status display.
    saved_at: String,
    sources: HashMap<BenchmarkSource, Vec<Benchmark>>,
}

pub(crate) fn cache_path() -> PathBuf {
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.cache_dir().join("benchmarks.json")
    } else {
        PathBuf::from("paretowatch-benchmarks.json")
    }
}

/// Load the cached boards. Returns the per-source sets plus the save
/// timestamp; `None` on any problem so callers fall back to a fresh fetch.
pub(crate) fn load_benchmark_cache() -> Option<(HashMap<BenchmarkSource, Vec<Benchmark>>, String)> {
    let path = cache_path();
    let bytes = fs::read(&path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != CACHE_VERSION || file.sources.is_empty() {
        return None;
    }
    Some((file.sources, file.saved_at))
}

/// Persist the current boards. Atomic (temp file + rename) so a crash mid-write
/// can never leave a truncated cache behind.
pub(crate) fn save_benchmark_cache(sets: &HashMap<BenchmarkSource, Vec<Benchmark>>) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = CacheFile {
        version: CACHE_VERSION,
        saved_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        sources: sets.clone(),
    };
    let Ok(bytes) = serde_json::to_vec(&file) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, bytes).is_ok() && fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::score_benchmark;
    use crate::types::BenchmarkKind;

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
