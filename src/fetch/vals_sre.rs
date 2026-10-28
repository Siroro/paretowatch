//! SRE Bench (vals.ai) — agentic software reverse engineering: models must
//! figure out what a real-world, contamination-free binary does without its
//! source code. Vals runs every model under its own standardized protocol,
//! so like DeepSWE the board is fully weighted in both composite
//! flavors rather than demoted as harness-specific.
//!
//! The benchmark page server-renders an accessibility table but the real
//! payload is a hashed Astro data file (`/_astro/benchmark_view_sre_bench
//! *.json`) preloaded from the page HTML; the hash changes with updates, so
//! the URL is always re-extracted from the page rather than hardcoded.

use anyhow::{Result, anyhow};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;

use super::fetch_text;
use super::{number_value, score_benchmark};
use crate::types::{Benchmark, BenchmarkKind, VALS_SRE_BENCH_URL};

pub(crate) fn fetch_vals_sre_bench(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(
        client,
        VALS_SRE_BENCH_URL,
        "SRE Bench (Vals) benchmark page",
    )?;
    let data_url = data_file_url(&html).ok_or_else(|| {
        anyhow!(
            "SRE Bench (Vals) page did not reference its benchmark_view data file ({VALS_SRE_BENCH_URL})"
        )
    })?;
    let full_url = if data_url.starts_with("http") {
        data_url.to_owned()
    } else {
        format!("https://www.vals.ai{data_url}")
    };
    let data = super::fetch_json(client, &full_url, "SRE Bench (Vals) leaderboard data")?;
    let rows = parse_vals_sre_bench(&data);
    if rows.is_empty() {
        return Err(anyhow!(
            "SRE Bench (Vals) leaderboard data contained no usable rows ({full_url})"
        ));
    }
    Ok(rows)
}

/// Extract the hashed `/​_astro/benchmark_view_sre_bench.*.json` path the
/// page preloads (via `<link rel="preload">` or the island props).
pub(crate) fn data_file_url(html: &str) -> Option<&str> {
    let re = Regex::new(r#"/_astro/benchmark_view_sre_bench[^"'`\s\\]*\.json"#)
        .expect("valid data-file regex");
    re.find(html).map(|m| m.as_str())
}

pub(crate) fn parse_vals_sre_bench(root: &Value) -> Vec<Benchmark> {
    let Some(entries) = root
        .get("tasks")
        .and_then(|tasks| tasks.get("overall"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    for (model_id, entry) in entries {
        let Some(score) = entry.get("accuracy").and_then(number_value) else {
            continue;
        };
        if !score.is_finite() {
            continue;
        }
        // `openai/gpt-6-astra` → `gpt 6 astra`; the provider prefix is
        // metadata (creator inference handles it), not part of the name.
        let model = model_id
            .rsplit_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or(model_id)
            .replace(['-', '_'], " ");
        if model.is_empty() {
            continue;
        }
        let harness = entry
            .get("harness")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let effort = entry
            .get("reasoning_effort")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let kind = if harness.is_some() {
            BenchmarkKind::ModelAgent
        } else {
            BenchmarkKind::Model
        };
        let mut row = score_benchmark(model.clone(), score, harness, effort, kind);
        row.slug = model.clone();
        row.name = format!("{model} [SRE Bench binary RE accuracy]");
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_overall_rows_and_marks_harnesses() {
        let root = json!({
            "tasks": {
                "overall": {
                    "openai/gpt-6-astra": {"accuracy": 56.87, "harness": null, "reasoning_effort": "max"},
                    "anthropic/claude-opus-5-5": {"accuracy": 33.59, "harness": "claude-code", "reasoning_effort": null},
                    "broken/model": {"accuracy": "n/a"}
                },
                "solved": {"openai/gpt-6-astra": {"accuracy": 1.0}}
            }
        });
        let rows = parse_vals_sre_bench(&root);
        assert_eq!(rows.len(), 2);
        let astra = rows.iter().find(|r| r.slug == "gpt 6 astra").unwrap();
        assert_eq!(astra.agentic_coding, Some(56.87));
        assert_eq!(astra.kind, BenchmarkKind::Model);
        assert_eq!(astra.reasoning_effort.as_deref(), Some("max"));
        let opus = rows.iter().find(|r| r.slug == "claude opus 5 5").unwrap();
        assert_eq!(opus.kind, BenchmarkKind::ModelAgent);
        assert_eq!(opus.agent.as_deref(), Some("claude-code"));
    }

    #[test]
    fn extracts_hashed_data_file_url_from_page_html() {
        let html = r#"<link rel="preload" href="/_astro/benchmark_view_sre_bench.Bd0uBJxH.json" as="fetch">"#;
        assert_eq!(
            data_file_url(html),
            Some("/_astro/benchmark_view_sre_bench.Bd0uBJxH.json")
        );
        assert_eq!(data_file_url("<html>nothing</html>"), None);
    }

    #[test]
    #[ignore = "hits live vals.ai; run explicitly with cargo test -- --ignored"]
    fn live_vals_sre_bench_has_rows() {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let rows = fetch_vals_sre_bench(&client).expect("live SRE Bench fetch");
        println!("{} rows; top: {}", rows.len(), rows[0].name);
        assert!(!rows.is_empty());
    }
}
