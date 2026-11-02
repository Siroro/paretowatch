//! Fetch layer: the background worker loop, HTTP helpers, benchmark
//! source dispatch, and the small text/number parsing utilities shared
//! across board fetchers.

pub(crate) mod aa_evals;
pub(crate) mod aa_index;
pub(crate) mod aa_speed;
pub(crate) mod deepswe;
pub(crate) mod design_arena;
pub(crate) mod frontiercode;
#[cfg(test)]
mod live_validation;
pub(crate) mod livebench;
pub(crate) mod performance;
pub(crate) mod prices;
pub(crate) mod swebench_pro;
pub(crate) mod terminal_bench;
pub(crate) mod vals_sre;

pub(crate) use aa_evals::*;
pub(crate) use aa_index::*;
pub(crate) use aa_speed::*;
pub(crate) use deepswe::*;
pub(crate) use design_arena::*;
pub(crate) use frontiercode::*;
pub(crate) use livebench::*;
pub(crate) use performance::*;
pub(crate) use prices::*;
pub(crate) use swebench_pro::*;
pub(crate) use terminal_bench::*;
pub(crate) use vals_sre::*;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use eframe::egui;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::alerts::evaluate_alerts;
use crate::notifications::SharedNotifications;
use crate::theme::infer_creator;
use crate::types::{Benchmark, BenchmarkKind, BenchmarkSource, Quote, Settings};
use crate::worker::{WorkerCommand, WorkerMessage};

pub(crate) const BENCHMARK_REFRESH_SECS: u64 = 6 * 60 * 60;

/// Hard cap on buffered response bodies. The largest fetch (the AA
/// leaderboard page) is a few megabytes, so anything bigger means the
/// endpoint drifted or lied about its content: error out instead of
/// ballooning memory.
const MAX_RESPONSE_BYTES: u64 = 25 * 1024 * 1024;

pub(crate) fn start_worker(
    ctx: egui::Context,
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
            ctx.request_repaint();

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
                    let benchmark_ctx = ctx.clone();
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
                        benchmark_ctx.request_repaint();
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
                    let perf_ctx = ctx.clone();
                    thread::spawn(move || {
                        let attempted = models.clone();
                        let rows = fetch_models_performance(&perf_client, &models);
                        let _ = perf_tx.send(WorkerMessage::Performance(rows, attempted));
                        perf_ctx.request_repaint();
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
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.config_dir().join("alert_state.json")
    } else {
        PathBuf::from("paretowatch-alert-state.json")
    }
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
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Sorted keys keep the file bytes stable for identical state.
    let ordered: BTreeMap<u64, bool> = state.iter().map(|(k, v)| (*k, *v)).collect();
    if let Ok(bytes) = serde_json::to_vec(&ordered) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::rename(&tmp, path);
        }
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

pub(crate) fn fetch_json(client: &Client, url: &str, label: &str) -> Result<Value> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let body = read_capped_body(response, label, url)?;
    let text = decode_body(body, label, url)?;
    serde_json::from_str(&text).with_context(|| format!("{label} returned invalid JSON ({url})"))
}

pub(crate) fn fetch_text(client: &Client, url: &str, label: &str) -> Result<String> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let body = read_capped_body(response, label, url)?;
    decode_body(body, label, url)
}

pub(crate) fn fetch_json_post(
    client: &Client,
    url: &str,
    body: &Value,
    label: &str,
) -> Result<Value> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let bytes = read_capped_body(response, label, url)?;
    let text = decode_body(bytes, label, url)?;
    serde_json::from_str(&text).with_context(|| format!("{label} returned invalid JSON ({url})"))
}

fn ensure_body_within_cap(len: u64, label: &str, url: &str) -> Result<()> {
    if len > MAX_RESPONSE_BYTES {
        return Err(anyhow!(
            "{label} response of {len} bytes exceeds the {MAX_RESPONSE_BYTES}-byte cap ({url})"
        ));
    }
    Ok(())
}

/// Buffer the response body under [`MAX_RESPONSE_BYTES`]. The stream is read
/// one byte past the cap so a body that lies about (or omits) its
/// Content-Length is detected rather than silently truncated.
fn read_capped_body<R: Read>(response: R, label: &str, url: &str) -> Result<Vec<u8>> {
    let mut limited = response.take(MAX_RESPONSE_BYTES + 1);
    let mut body = Vec::new();
    limited
        .read_to_end(&mut body)
        .with_context(|| format!("Could not read {label} response ({url})"))?;
    ensure_body_within_cap(body.len() as u64, label, url)?;
    Ok(body)
}

/// Decode a capped body the way reqwest's `text()` does for the UTF-8
/// responses these feeds serve, including stripping a leading BOM.
fn decode_body(body: Vec<u8>, label: &str, url: &str) -> Result<String> {
    String::from_utf8(body)
        .map(|text| {
            text.strip_prefix('\u{feff}')
                .map(str::to_owned)
                .unwrap_or(text)
        })
        .with_context(|| format!("Could not read {label} response ({url})"))
}

pub(crate) fn fetch_benchmark_source(
    client: &Client,
    source: BenchmarkSource,
) -> Result<Vec<Benchmark>> {
    match source {
        BenchmarkSource::ArtificialAnalysisSnapshot => fetch_artificial_analysis_index(client),
        BenchmarkSource::TerminalBench4 => fetch_terminal_bench_4(client),
        BenchmarkSource::DeepSWE11 => fetch_deepswe(client),
        BenchmarkSource::LiveBench => fetch_livebench(client),
        BenchmarkSource::DesignArena => fetch_design_arena(client),
        BenchmarkSource::FrontierCode => fetch_frontiercode(client),
        BenchmarkSource::FrontierCode2 => fetch_frontiercode2(client),
        BenchmarkSource::AABriefcase => fetch_aa_briefcase(client),
        BenchmarkSource::AutomationBenchAA => fetch_automationbench_aa(client),
        BenchmarkSource::AA3D => fetch_aa_3d(client),
        BenchmarkSource::SWEBenchPro => fetch_swebench_pro(client),
        BenchmarkSource::ValsSREBench => fetch_vals_sre_bench(client),
        BenchmarkSource::CompositeAgentic | BenchmarkSource::CompositeDeployment => {
            Err(anyhow!("Composite is derived locally and is not fetched"))
        }
    }
}

pub(crate) fn score_benchmark(
    model: impl Into<String>,
    score: f64,
    agent: Option<String>,
    reasoning_effort: Option<String>,
    kind: BenchmarkKind,
) -> Benchmark {
    let model = model.into();
    Benchmark {
        slug: model.clone(),
        name: model.clone(),
        creator: infer_creator(&model),
        overall: Some(score),
        coding: Some(score),
        agentic_coding: Some(score),
        agent,
        reasoning_effort,
        kind,
        tokens_per_task: None,
        token_profile: None,
    }
}

pub(crate) fn strip_html_tags(html: &str) -> String {
    let re = Regex::new(r"(?s)<[^>]*>").expect("valid tag regex");
    let stripped = re.replace_all(html, " ");
    stripped
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
}

pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

pub(crate) fn json_shape(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().take(12).cloned().collect::<Vec<_>>();
            keys.sort();
            format!("object keys [{}]", keys.join(", "))
        }
        Value::Array(items) => format!("array len {}", items.len()),
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Number(_) => "number".into(),
        Value::String(v) => format!("string len {}", v.len()),
    }
}

pub(crate) fn first_number_path(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths
        .iter()
        .find_map(|p| value_at_path(value, p).and_then(value_as_f64))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

pub(crate) fn first_string_path(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|p| {
        value_at_path(value, p)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn value_at_path<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

pub(crate) fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(*k).and_then(Value::as_str).map(str::to_owned))
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

    #[test]
    fn oversized_bodies_are_rejected_with_source_named() {
        let url = "https://example.test/feed";
        assert!(ensure_body_within_cap(MAX_RESPONSE_BYTES, "Feed", url).is_ok());
        let err =
            ensure_body_within_cap(MAX_RESPONSE_BYTES + 1, "Feed", url).expect_err("over the cap");
        let message = format!("{err:#}");
        assert!(message.contains("Feed") && message.contains(url));
    }

    #[test]
    fn body_reads_stop_at_the_cap_even_when_the_stream_does_not() {
        // A stream with no end: the Take limit must stop the read and the
        // cap check must then reject it instead of truncating silently.
        let stream = std::io::repeat(0u8).take(MAX_RESPONSE_BYTES + 2);
        let err = read_capped_body(stream, "Feed", "https://example.test/feed")
            .expect_err("unbounded stream");
        let message = format!("{err:#}");
        assert!(message.contains("Feed") && message.contains("cap"));
    }

    #[test]
    fn bodies_within_the_cap_are_buffered_whole() {
        let stream = std::io::Cursor::new(b"payload".to_vec());
        let body =
            read_capped_body(stream, "Feed", "https://example.test/feed").expect("small body");
        assert_eq!(body, b"payload");
    }
}
