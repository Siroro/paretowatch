//! The background worker: the message protocol shared with the UI thread,
//! the poll loop that drives pricing/benchmark/performance fetches and
//! worker-evaluated alerts, and the persisted alert edge state.
//!
//! The loop is UI-agnostic: the caller hands in a repaint closure, so this
//! module has no egui dependency.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;

use crate::alerts::evaluate_alerts;
use crate::fetch::aa_index::fetch_aa_index_and_speed;
use crate::fetch::fetch_benchmark_source;
use crate::fetch::performance::fetch_models_performance;
use crate::fetch::prices::fetch_prices;
use crate::notifications::SharedNotifications;
use crate::types::{
    AaSpeedSnapshot, Benchmark, BenchmarkSource, ModelPerformance, PriceSnapshot, Quote, Settings,
};

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Refresh,
    UpdateSettings(Settings),
    /// Poll the Provider Performance API for these models (on-demand
    /// selection or the slow background refresh of cached rows).
    FetchPerformance(Vec<String>),
    Quit,
}

#[derive(Debug)]
pub(crate) enum WorkerMessage {
    Prices(Result<PriceSnapshot, String>),
    Benchmarks(BenchmarkSource, Result<Vec<Benchmark>, String>),
    /// Provider breakdowns that fetched successfully, plus every model the
    /// pull attempted: an attempted model with no row failed, so its last
    /// cached row survives upstream and the caller can back off retrying.
    Performance(Vec<ModelPerformance>, Vec<String>),
    /// AA's standardized speed table, a side product of the Intelligence
    /// Index leaderboard fetch.
    AaSpeed(Result<AaSpeedSnapshot, String>),
}

pub(crate) const BENCHMARK_REFRESH_SECS: u64 = 6 * 60 * 60;

/// Spawns the background poll loop. `repaint` is called after every batch of
/// fresh data so the UI reacts even while the window would otherwise idle.
pub(crate) fn start_worker(
    repaint: impl Fn() + Send + Sync + Clone + 'static,
    initial_settings: Settings,
    notifications: SharedNotifications,
) -> (Sender<WorkerCommand>, Receiver<WorkerMessage>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>();
    let (msg_tx, msg_rx) = mpsc::channel::<WorkerMessage>();

    thread::spawn(move || {
        // A failed client build must not panic here: that would kill the
        // worker thread before its loop and leave the UI on "Starting…"
        // forever. Surface it as a pricing error instead.
        let client = match Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("ParetoWatch/", env!("CARGO_PKG_VERSION")))
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                let _ = msg_tx.send(WorkerMessage::Prices(Err(format!(
                    "HTTP client init failed: {err}"
                ))));
                return;
            }
        };
        let mut settings = initial_settings;
        // Edge-trigger state for the worker-evaluated level alerts. It is
        // persisted so a standing condition does not re-fire on every app
        // start: an absent entry reads as `false`, which is the firing edge.
        let mut saved_alert_state: HashMap<u64, bool> = load_alert_state();
        let mut alert_state: HashMap<u64, bool> = saved_alert_state.clone();
        // Prune to the live alert ids the same way settings saves do, so
        // entries for deleted alerts cannot linger in the file.
        alert_state.retain(|id, _| settings.alerts.iter().any(|a| a.id == *id));
        let mut previous_quotes: HashMap<String, Quote> = HashMap::new();
        // Base feed of the last accepted poll, so alert evaluation can tell
        // a real price move apart from the fetch fallback chain switching
        // between catalog and text sources with different per-model prices.
        let mut previous_base_source: Option<String> = None;
        let mut last_benchmark_fetch: Option<Instant> = None;

        loop {
            let price_result = fetch_prices(&client, &settings);
            if let Ok(snapshot) = &price_result {
                // No previous poll yet means no comparison, not a switch
                // (mirroring the empty `previous_quotes` seeding below).
                let base_source_changed = previous_base_source
                    .as_deref()
                    .is_some_and(|source| source != snapshot.base_source);
                evaluate_alerts(
                    snapshot,
                    &settings,
                    &mut alert_state,
                    &previous_quotes,
                    base_source_changed,
                    &notifications,
                );
                persist_alert_state(&alert_state, &mut saved_alert_state);
                previous_quotes = snapshot
                    .quotes
                    .iter()
                    .cloned()
                    .map(|q| (q.model.clone(), q))
                    .collect();
                previous_base_source = Some(snapshot.base_source.clone());
            }
            let _ = msg_tx.send(WorkerMessage::Prices(
                price_result.map_err(|e| format!("{e:#}")),
            ));
            repaint();

            let should_fetch_benchmarks = last_benchmark_fetch
                .map(|t| t.elapsed() >= Duration::from_secs(BENCHMARK_REFRESH_SECS))
                .unwrap_or(true);
            if should_fetch_benchmarks {
                // Benchmark sources are independent and some HTML leaderboards can
                // be slow. Fetch them in parallel so a sluggish source never stalls
                // the 30-second Surplus pricing loop or the tray UI.
                for source in BenchmarkSource::remote_sources() {
                    let benchmark_client = client.clone();
                    let benchmark_tx = msg_tx.clone();
                    let benchmark_repaint = repaint.clone();
                    thread::spawn(move || {
                        // The AA leaderboard download yields two products —
                        // the Intelligence Index overlay and the
                        // standardized-speed table — so that source sends
                        // both messages from one fetch.
                        let result = if source == BenchmarkSource::ArtificialAnalysisSnapshot {
                            let (index, speed) = fetch_aa_index_and_speed(&benchmark_client);
                            let _ = benchmark_tx.send(WorkerMessage::AaSpeed(speed));
                            // Already a String error: an index failure must
                            // reach the app as an Err so it keeps its last
                            // good board instead of regressing to the older
                            // bundled snapshot.
                            index
                        } else {
                            fetch_benchmark_source(&benchmark_client, source)
                                .map_err(|e| format!("{e:#}"))
                        };
                        let _ = benchmark_tx.send(WorkerMessage::Benchmarks(source, result));
                        benchmark_repaint();
                    });
                }
                last_benchmark_fetch = Some(Instant::now());
            }

            match cmd_rx.recv_timeout(Duration::from_secs(settings.poll_seconds.max(30))) {
                Ok(WorkerCommand::Refresh) => last_benchmark_fetch = None,
                Ok(WorkerCommand::UpdateSettings(new_settings)) => {
                    settings = new_settings;
                    alert_state.retain(|id, _| settings.alerts.iter().any(|a| a.id == *id));
                    persist_alert_state(&alert_state, &mut saved_alert_state);
                }
                Ok(WorkerCommand::FetchPerformance(models)) => {
                    // Route analytics are polled per model (no bulk endpoint),
                    // so this is usually one URL — but it must never block the
                    // 30-second pricing loop or the tray UI.
                    let perf_client = client.clone();
                    let perf_tx = msg_tx.clone();
                    let perf_repaint = repaint.clone();
                    thread::spawn(move || {
                        let attempted = models.clone();
                        let rows = fetch_models_performance(&perf_client, &models);
                        let _ = perf_tx.send(WorkerMessage::Performance(rows, attempted));
                        perf_repaint();
                    });
                }
                Ok(WorkerCommand::Quit) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    (cmd_tx, msg_rx)
}

/// Persisted edge-trigger state for the worker-evaluated level alerts,
/// alongside the other config files: an unparseable or missing file starts
/// from the empty state (the pre-persistence behavior), never a panic.
fn alert_state_path() -> PathBuf {
    crate::persist::config_file("alert_state.json")
}

fn load_alert_state_from(path: &Path) -> HashMap<u64, bool> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn load_alert_state() -> HashMap<u64, bool> {
    load_alert_state_from(&alert_state_path())
}

/// Atomic write (temp + rename, the notification log's pattern). Best
/// effort: a failed save only costs a re-fire on the next app start.
fn save_alert_state_to(path: &Path, state: &HashMap<u64, bool>) {
    // Sorted keys keep the file bytes stable for identical state.
    let ordered: BTreeMap<u64, bool> = state.iter().map(|(k, v)| (*k, *v)).collect();
    if let Ok(bytes) = serde_json::to_vec(&ordered) {
        let _ = crate::persist::atomic_write(path, &bytes);
    }
}

/// `evaluate_alerts` rewrites the whole map every poll, so only touch the
/// disk when the map actually changed since the last write.
fn persist_alert_state_to(
    path: &Path,
    state: &HashMap<u64, bool>,
    last_saved: &mut HashMap<u64, bool>,
) {
    if *state != *last_saved {
        save_alert_state_to(path, state);
        *last_saved = state.clone();
    }
}

fn persist_alert_state(state: &HashMap<u64, bool>, last_saved: &mut HashMap<u64, bool>) {
    persist_alert_state_to(&alert_state_path(), state, last_saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("paretowatch-alert-state-tests")
            .join(format!("{}-{name}.json", std::process::id()))
    }

    #[test]
    fn alert_state_roundtrips_through_disk() {
        let path = temp_state_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let mut state: HashMap<u64, bool> = HashMap::new();
        state.insert(7, true);
        state.insert(11, false);
        save_alert_state_to(&path, &state);
        assert_eq!(load_alert_state_from(&path), state);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn alert_state_tolerates_corrupt_or_missing_files() {
        // Never panic, never seed stale edges: an unreadable file is the
        // empty state (the pre-persistence behavior).
        let path = temp_state_path("corrupt");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(load_alert_state_from(&path).is_empty());
        let _ = std::fs::remove_file(&path);
        assert!(load_alert_state_from(&path).is_empty());
    }

    #[test]
    fn alert_state_persists_only_when_changed() {
        // evaluate_alerts rewrites the map every poll; the file must not be
        // rewritten alongside it unless an edge actually moved.
        let path = temp_state_path("changed-only");
        let _ = std::fs::remove_file(&path);
        let mut saved: HashMap<u64, bool> = HashMap::new();
        let mut state: HashMap<u64, bool> = HashMap::new();
        state.insert(3, true);
        persist_alert_state_to(&path, &state, &mut saved);
        assert_eq!(load_alert_state_from(&path).get(&3), Some(&true));
        // Same state again: no write — a hand-mangled file survives intact.
        std::fs::write(&path, b"mangled").unwrap();
        persist_alert_state_to(&path, &state, &mut saved);
        assert_eq!(std::fs::read(&path).unwrap(), b"mangled");
        // A changed edge is written through and becomes the new baseline.
        state.insert(3, false);
        persist_alert_state_to(&path, &state, &mut saved);
        assert_eq!(load_alert_state_from(&path).get(&3), Some(&false));
        let _ = std::fs::remove_file(&path);
    }
}
