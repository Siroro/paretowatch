//! Surplus Provider Performance API: per-provider speed and reliability for
//! one model, aggregated from actual routed traffic over a rolling 24h
//! window. There is no bulk endpoint, so models are polled individually and
//! only when a user actually looks at one — the app requests a model's
//! breakdown on selection, keeps it in a disk-cached map, and a slow
//! background pass re-pulls whatever is cached (the 24h aggregates move
//! slowly, see [`PERFORMANCE_REFRESH_SECS`]).
//!
//! The market feed already carries the market-wide medians (`tps_p50`,
//! `ttft_p50_ms` on every quote); this module adds the per-provider split
//! that answers "which route is fast".

use std::thread;

use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;

use super::{fetch_json, first_number_path, string_at};
use crate::bench::matching::normalize;
use crate::types::{ModelPerformance, ProviderPerformance};

pub(crate) const SURPLUS_PERFORMANCE_URL: &str =
    "https://api.surplusintelligence.ai/api/analytics/performance/";

/// How long a cached per-model breakdown stays fresh for the slow background
/// refresh — the same six-hour cadence as the benchmark boards.
pub(crate) const PERFORMANCE_REFRESH_SECS: u64 = 6 * 60 * 60;

/// Selecting a model means the user is looking at it right now, so the
/// on-demand path tolerates less staleness than the background pass: a
/// cached breakdown older than this is refetched when the model is selected.
pub(crate) const ON_DEMAND_REFRESH_SECS: u64 = 15 * 60;

/// After a failed fetch (404, timeout, unparseable body) the model is left
/// alone for this long before a retry, so a dead endpoint is not hammered by
/// the per-frame on-demand check.
pub(crate) const PERFORMANCE_RETRY_BACKOFF_SECS: u64 = 10 * 60;

/// Threads fetching route analytics at once. On-demand pulls are usually one
/// model; the background sweep fans out over everything cached.
const PARALLEL_FETCHES: usize = 12;

/// Parse one model's Provider Performance payload. Returns `None` when the
/// payload carries no provider rows at all (404-shaped bodies, HTML error
/// pages); a provider array whose rows are all empty still parses, because
/// requests/uptime rows are meaningful without speed telemetry.
pub(crate) fn parse_model_performance(model: &str, value: &Value) -> Option<ModelPerformance> {
    let providers = value.get("providers")?.as_array()?;
    let parsed = providers
        .iter()
        .filter_map(|row| {
            let provider = string_at(row, &["provider", "name"])?;
            let count = |key: &str| {
                first_number_path(row, &[&[key]]).and_then(|n| (n >= 0.0).then_some(n as u64))
            };
            let latency = |key: &str| {
                first_number_path(row, &[&[key]]).filter(|v| v.is_finite() && *v >= 0.0)
            };
            Some(ProviderPerformance {
                provider,
                requests: count("requests"),
                failures: count("failures"),
                sellers: count("sellers"),
                uptime_pct: latency("uptime_pct"),
                avg_latency_ms: latency("avg_latency_ms"),
                p50_latency_ms: latency("p50_latency_ms"),
                p95_latency_ms: latency("p95_latency_ms"),
                ttft_p50_ms: latency("ttft_p50_ms"),
                ttft_p90_ms: latency("ttft_p90_ms"),
                ttft_samples: count("ttft_samples"),
                tps_p50: latency("tps_p50"),
                tps_samples: count("tps_samples"),
            })
        })
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        return None;
    }
    Some(ModelPerformance {
        model: model.to_owned(),
        fetched_at: Utc::now(),
        providers: parsed,
    })
}

fn fetch_one(client: &Client, model: &str) -> Option<ModelPerformance> {
    let url = format!("{SURPLUS_PERFORMANCE_URL}{model}");
    let value = fetch_json(client, &url, "Surplus performance").ok()?;
    parse_model_performance(model, &value)
}

/// Poll the given models' provider breakdowns in parallel chunks, returning
/// only the successes — a failed model simply stays absent (or keeps its
/// last cached row upstream), and the caller's backoff handles retries.
pub(crate) fn fetch_models_performance(
    client: &Client,
    models: &[String],
) -> Vec<ModelPerformance> {
    let mut rows = Vec::new();
    for chunk in models.chunks(PARALLEL_FETCHES.max(1)) {
        thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|model| scope.spawn(|| fetch_one(client, model)))
                .collect();
            for handle in handles {
                if let Some(perf) = handle.join().ok().flatten() {
                    rows.push(perf);
                }
            }
        });
    }
    rows
}

/// Key a performance row into the snapshot map: normalized, so catalog and
/// market spellings of the same model land on one entry.
pub(crate) fn performance_key(model: &str) -> String {
    normalize(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the live Provider Performance payload for gpt-5.4
    /// (2026-10): a healthy provider row, a zero-traffic row with failures,
    /// and a row with TTFT samples but no throughput observations.
    fn live_payload() -> Value {
        serde_json::json!({
            "model": "gpt-5.4",
            "window_days": 1,
            "complete": true,
            "computed_at": 1_793_030_863_238_i64,
            "providers": [
                {"provider": "inferhub", "requests": 0, "failures": 210, "sellers": 3,
                 "uptime_pct": 0, "avg_latency_ms": null, "p50_latency_ms": null,
                 "p95_latency_ms": null, "ttft_samples": 0, "ttft_p50_ms": null,
                 "ttft_p90_ms": null, "tps_samples": 0, "tps_p50": null},
                {"provider": "openai", "requests": 2757, "failures": 23, "sellers": 5,
                 "uptime_pct": 99.2, "avg_latency_ms": 4666, "p50_latency_ms": 1670,
                 "p95_latency_ms": 24867, "ttft_samples": 1570, "ttft_p50_ms": 916,
                 "ttft_p90_ms": 1670, "tps_samples": 1497, "tps_p50": 73.3},
                {"provider": "venice", "requests": 13, "failures": 27, "sellers": 3,
                 "uptime_pct": 32.5, "avg_latency_ms": 83825, "ttft_samples": 13,
                 "ttft_p50_ms": null, "tps_samples": 0, "tps_p50": null}
            ]
        })
    }

    #[test]
    fn parses_live_provider_rows_with_and_without_telemetry() {
        let perf = parse_model_performance("gpt-5.4", &live_payload()).unwrap();
        assert_eq!(perf.model, "gpt-5.4");
        assert_eq!(perf.providers.len(), 3);

        let openai = perf.provider("openai").unwrap();
        assert_eq!(openai.requests, Some(2757));
        assert_eq!(openai.uptime_pct, Some(99.2));
        assert_eq!(openai.tps_p50, Some(73.3));
        assert_eq!(openai.tps_samples, Some(1497));
        assert_eq!(openai.ttft_p50_ms, Some(916.0));
        assert_eq!(openai.p95_latency_ms, Some(24_867.0));

        // A dead route keeps its requests/failures/uptime even with nulls.
        let inferhub = perf.provider("inferhub").unwrap();
        assert_eq!(inferhub.requests, Some(0));
        assert_eq!(inferhub.uptime_pct, Some(0.0));
        assert_eq!(inferhub.tps_p50, None);

        // TTFT samples without percentiles still parse (nulls drop out).
        let venice = perf.provider("venice").unwrap();
        assert_eq!(venice.ttft_samples, Some(13));
        assert_eq!(venice.ttft_p50_ms, None);

        // Stamped for the on-demand staleness check.
        assert!(perf.fetched_at <= Utc::now());
    }

    #[test]
    fn payloads_without_provider_rows_are_rejected() {
        assert!(
            parse_model_performance("m", &serde_json::json!({"error": {"code": "not_found"}}))
                .is_none()
        );
        assert!(parse_model_performance("m", &serde_json::json!({"providers": []})).is_none());
        assert!(parse_model_performance("m", &serde_json::json!({})).is_none());
        // Rows without names cannot join the market's provider list.
        assert!(
            parse_model_performance("m", &serde_json::json!({"providers": [{"requests": 5}]}))
                .is_none()
        );
    }

    #[test]
    fn performance_keys_normalize_punctuation_and_case() {
        assert_eq!(performance_key("GPT 5.4"), performance_key("gpt-5.4"));
        assert_eq!(
            performance_key("deepseek_v4.1_flash"),
            performance_key("deepseek-v4.1-flash")
        );
        // A creator prefix is a different key, matching the market overlay
        // join's plain normalize equality.
        assert_ne!(
            performance_key("openai/gpt-5.4"),
            performance_key("gpt-5.4")
        );
    }
}
