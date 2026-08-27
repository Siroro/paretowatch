//! Fetch layer: the background worker loop, HTTP helpers, benchmark
//! source dispatch, and the small text/number parsing utilities shared
//! across board fetchers.

pub(crate) mod deepswe;
pub(crate) mod design_arena;
pub(crate) mod livebench;
pub(crate) mod prices;
pub(crate) mod revelo;
pub(crate) mod swe_rebench;
pub(crate) mod terminal_bench;

pub(crate) use deepswe::*;
pub(crate) use design_arena::*;
pub(crate) use livebench::*;
pub(crate) use prices::*;
pub(crate) use revelo::*;
pub(crate) use swe_rebench::*;
pub(crate) use terminal_bench::*;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use eframe::egui;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::alerts::evaluate_alerts;
use crate::artificial_analysis_snapshot::artificial_analysis_snapshot;
use crate::notifications::SharedNotifications;
use crate::theme::infer_creator;
use crate::types::{Benchmark, BenchmarkKind, BenchmarkSource, Quote, Settings};
use crate::worker::{WorkerCommand, WorkerMessage};

pub(crate) const BENCHMARK_REFRESH_SECS: u64 = 6 * 60 * 60;

pub(crate) fn start_worker(
    ctx: egui::Context,
    initial_settings: Settings,
    notifications: SharedNotifications,
) -> (Sender<WorkerCommand>, Receiver<WorkerMessage>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>();
    let (msg_tx, msg_rx) = mpsc::channel::<WorkerMessage>();

    thread::spawn(move || {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("ParetoWatch/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        let mut settings = initial_settings;
        let mut alert_state: HashMap<u64, bool> = HashMap::new();
        let mut previous_quotes: HashMap<String, Quote> = HashMap::new();
        let mut last_benchmark_fetch: Option<Instant> = None;

        loop {
            let price_result = fetch_prices(&client, &settings);
            if let Ok(snapshot) = &price_result {
                evaluate_alerts(
                    snapshot,
                    &settings,
                    &mut alert_state,
                    &previous_quotes,
                    &notifications,
                );
                previous_quotes = snapshot
                    .quotes
                    .iter()
                    .cloned()
                    .map(|q| (q.model.clone(), q))
                    .collect();
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
                        let result = fetch_benchmark_source(&benchmark_client, source)
                            .map_err(|e| format!("{e:#}"));
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
                }
                Ok(WorkerCommand::Quit) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    (cmd_tx, msg_rx)
}

pub(crate) fn fetch_json(client: &Client, url: &str, label: &str) -> Result<Value> {
    client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?
        .json::<Value>()
        .with_context(|| format!("{label} returned invalid JSON ({url})"))
}

pub(crate) fn fetch_text(client: &Client, url: &str, label: &str) -> Result<String> {
    client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?
        .text()
        .with_context(|| format!("Could not read {label} response ({url})"))
}

pub(crate) fn fetch_json_post(
    client: &Client,
    url: &str,
    body: &Value,
    label: &str,
) -> Result<Value> {
    client
        .post(url)
        .json(body)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?
        .json::<Value>()
        .with_context(|| format!("{label} returned invalid JSON ({url})"))
}

pub(crate) fn fetch_benchmark_source(
    client: &Client,
    source: BenchmarkSource,
) -> Result<Vec<Benchmark>> {
    match source {
        BenchmarkSource::ArtificialAnalysisSnapshot => Ok(artificial_analysis_snapshot()),
        BenchmarkSource::SWERebench => fetch_swe_rebench(client),
        BenchmarkSource::TerminalBench3 => fetch_terminal_bench_3(client),
        BenchmarkSource::DeepSWE11 => fetch_deepswe(client),
        BenchmarkSource::LiveBench => fetch_livebench(client),
        BenchmarkSource::ReveloCodeIndex => fetch_revelo_code_index(client),
        BenchmarkSource::DesignArena => fetch_design_arena(client),
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
