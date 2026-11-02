//! Disk cache for per-model route analytics (Provider Performance API).
//! Breakdowns are fetched on demand — when a model is selected somewhere the
//! user can see its providers — and persist here so a restart starts from
//! the previous session's rows instead of refetching every model that was
//! ever looked at. The slow background refresh replaces them in place.
//!
//! Best-effort like the benchmark cache: any read error (missing, corrupt,
//! version drift) is ignored — the app falls back to on-demand fetches — and
//! any write error is swallowed, because stale route numbers cost nothing
//! next to a dead app.

use std::collections::HashMap;
use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::types::ModelPerformance;

const CACHE_VERSION: u32 = 1;

/// Rows the cache keeps: without a cap, every model ever hovered would hold
/// a persisted row (and a six-hour background refresh) forever. Past the
/// cap, saves evict the rows with the oldest `fetched_at` — a row's fetch
/// doubles as its last-use stamp, since rows are only refetched when a view
/// actually uses them.
const MAX_ROWS: usize = 500;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    /// UTC timestamp of the last successful save, for status display.
    saved_at: String,
    /// Normalized model id → last provider breakdown.
    rows: HashMap<String, ModelPerformance>,
}

pub(crate) fn cache_path() -> PathBuf {
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.cache_dir().join("performance.json")
    } else {
        PathBuf::from("paretowatch-performance.json")
    }
}

/// Load the cached route analytics. Returns the per-model rows plus the save
/// timestamp; `None` on any problem so callers fall back to fresh fetches.
pub(crate) fn load_performance_cache() -> Option<(HashMap<String, ModelPerformance>, String)> {
    let path = cache_path();
    let bytes = fs::read(&path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != CACHE_VERSION || file.rows.is_empty() {
        return None;
    }
    Some((file.rows, file.saved_at))
}

/// Evict the least-recently-fetched rows past [`MAX_ROWS`] so the map (and
/// the file it is saved to) cannot grow without bound.
fn prune_lru(rows: &mut HashMap<String, ModelPerformance>) {
    if rows.len() <= MAX_ROWS {
        return;
    }
    let mut by_age: Vec<(String, chrono::DateTime<chrono::Utc>)> = rows
        .iter()
        .map(|(key, row)| (key.clone(), row.fetched_at))
        .collect();
    by_age.sort_unstable_by_key(|(_, fetched_at)| *fetched_at);
    let evict = by_age.len() - MAX_ROWS;
    for (key, _) in by_age.into_iter().take(evict) {
        rows.remove(&key);
    }
}

/// Persist the current rows, pruning past [`MAX_ROWS`] first so the cap
/// holds on disk and in memory alike. Atomic (temp file + rename) so a
/// crash mid-write can never leave a truncated cache behind.
pub(crate) fn save_performance_cache(rows: &mut HashMap<String, ModelPerformance>) {
    prune_lru(rows);
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = CacheFile {
        version: CACHE_VERSION,
        saved_at: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
        rows: rows.clone(),
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
    use crate::fetch::performance_key;
    use crate::types::ProviderPerformance;
    use chrono::{Duration, Utc};

    fn sample_rows() -> HashMap<String, ModelPerformance> {
        let mut rows = HashMap::new();
        rows.insert(
            performance_key("gpt-5.4"),
            ModelPerformance {
                model: "gpt-5.4".into(),
                fetched_at: Utc::now() - Duration::hours(2),
                providers: vec![ProviderPerformance {
                    requests: Some(2757),
                    tps_p50: Some(73.3),
                    ttft_p50_ms: Some(916.0),
                    uptime_pct: Some(99.2),
                    ..empty_provider("openai")
                }],
            },
        );
        rows
    }

    fn empty_provider(name: &str) -> ProviderPerformance {
        ProviderPerformance {
            provider: name.to_owned(),
            requests: None,
            failures: None,
            sellers: None,
            uptime_pct: None,
            avg_latency_ms: None,
            p50_latency_ms: None,
            p95_latency_ms: None,
            ttft_p50_ms: None,
            ttft_p90_ms: None,
            ttft_samples: None,
            tps_p50: None,
            tps_samples: None,
        }
    }

    #[test]
    // One test, sequentially: both halves share the real cache path, and
    // separate #[test]s would race on that file under the parallel runner.
    fn cache_round_trips_and_tolerates_corruption() {
        let path = cache_path();
        let mut rows = sample_rows();
        save_performance_cache(&mut rows);
        let (loaded, saved_at) = load_performance_cache().expect("cache loads after save");
        let row = loaded.get(&performance_key("gpt-5.4")).unwrap();
        assert_eq!(row.provider("openai").unwrap().tps_p50, Some(73.3));
        assert_eq!(row.provider("openai").unwrap().uptime_pct, Some(99.2));
        assert!(!saved_at.is_empty());
        // The stamp is UTC (matching the sibling caches), so it must sit
        // within a minute of the clock — a local-time stamp would drift by
        // the timezone offset.
        let stamped = chrono::NaiveDateTime::parse_from_str(&saved_at, "%Y-%m-%d %H:%M")
            .expect("saved_at parses as %Y-%m-%d %H:%M");
        let drift_minutes = (chrono::Utc::now().naive_utc() - stamped)
            .num_minutes()
            .abs();
        assert!(drift_minutes <= 1, "saved_at is not UTC: {saved_at}");
        // A corrupt file is ignored rather than fatal.
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(load_performance_cache().is_none());
        // So is a missing one.
        let _ = std::fs::remove_file(&path);
        assert!(load_performance_cache().is_none());
    }

    #[test]
    fn prune_evicts_oldest_fetched_past_the_cap() {
        let build = |count: usize| {
            (0..count)
                .map(|i| {
                    (
                        format!("model-{i}"),
                        ModelPerformance {
                            model: format!("model-{i}"),
                            // Higher index = fetched longer ago (older).
                            fetched_at: Utc::now() - Duration::hours(i as i64 + 1),
                            providers: Vec::new(),
                        },
                    )
                })
                .collect::<HashMap<String, ModelPerformance>>()
        };
        let mut rows = build(MAX_ROWS + 5);
        prune_lru(&mut rows);
        assert_eq!(rows.len(), MAX_ROWS, "cap enforced");
        // The five oldest rows (the highest indexes) are evicted first.
        assert!(!rows.contains_key(&format!("model-{}", MAX_ROWS + 4)));
        assert!(!rows.contains_key(&format!("model-{}", MAX_ROWS)));
        assert!(rows.contains_key(&format!("model-{}", MAX_ROWS - 1)));
        assert!(rows.contains_key("model-0"));
        // At the cap the prune is a no-op.
        prune_lru(&mut rows);
        assert_eq!(rows.len(), MAX_ROWS);
    }
}
