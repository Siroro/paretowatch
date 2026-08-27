//! Terminal-Bench 3.0 leaderboard: Harbor API with a GitHub snapshot
//! fallback.

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

use super::json_shape;
use super::{fetch_text, number_value, score_benchmark};
use crate::bench::matching::normalize;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const TERMINAL_BENCH_HARBOR_URL: &str =
    "https://ofhuhcpkvzjlejydnvyd.supabase.co/functions/v1/leaderboard-read";
pub(crate) const TERMINAL_BENCH_PACKAGE: &str = "terminal-bench/terminal-bench";
pub(crate) const TERMINAL_BENCH_LEADERBOARD: &str = "3-0-0";
pub(crate) const TERMINAL_BENCH_SNAPSHOT_URL: &str = "https://raw.githubusercontent.com/harbor-framework/frontier-bench-docs/main/lib/announcement-leaderboard-snapshot.ts";

pub(crate) fn fetch_terminal_bench_3(client: &Client) -> Result<Vec<Benchmark>> {
    match fetch_terminal_bench_harbor(client).and_then(|value| parse_terminal_bench_harbor(&value))
    {
        Ok(rows) if !rows.is_empty() => Ok(rows),
        live_result => {
            let live_error = match live_result {
                Ok(_) => "live Harbor leaderboard contained no usable rows".to_owned(),
                Err(err) => format!("{err:#}"),
            };
            let text = fetch_text(
                client,
                TERMINAL_BENCH_SNAPSHOT_URL,
                "Terminal-Bench 3.0 fallback snapshot",
            )?;
            let rows = parse_terminal_bench_snapshot(&text)?;
            if rows.is_empty() {
                return Err(anyhow!(
                    "Terminal-Bench live read failed ({live_error}); fallback snapshot also contained no usable rows"
                ));
            }
            Ok(rows)
        }
    }
}

pub(crate) fn fetch_terminal_bench_harbor(client: &Client) -> Result<Value> {
    client
        .post(TERMINAL_BENCH_HARBOR_URL)
        .json(&serde_json::json!({
            "package": TERMINAL_BENCH_PACKAGE,
            "name": TERMINAL_BENCH_LEADERBOARD,
        }))
        .send()
        .with_context(|| {
            format!("Terminal-Bench 3.0 Harbor request failed ({TERMINAL_BENCH_HARBOR_URL})")
        })?
        .error_for_status()
        .with_context(|| {
            format!(
                "Terminal-Bench 3.0 Harbor returned an HTTP error ({TERMINAL_BENCH_HARBOR_URL})"
            )
        })?
        .json::<Value>()
        .with_context(|| {
            format!("Terminal-Bench 3.0 Harbor returned invalid JSON ({TERMINAL_BENCH_HARBOR_URL})")
        })
}

pub(crate) fn parse_terminal_bench_harbor(root: &Value) -> Result<Vec<Benchmark>> {
    let rows = root
        .get("rows")
        .and_then(Value::as_array)
        .or_else(|| {
            root.get("leaderboard")
                .and_then(|v| v.get("rows"))
                .and_then(Value::as_array)
        })
        .ok_or_else(|| {
            anyhow!(
                "Terminal-Bench Harbor JSON has no rows array; shape: {}",
                json_shape(root)
            )
        })?;

    let mut out = Vec::new();
    for row in rows {
        let status = row
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("display");
        if !status.eq_ignore_ascii_case("display") && !status.eq_ignore_ascii_case("hide") {
            continue;
        }
        let metadata = row.get("metadata").and_then(Value::as_object);
        let metrics = row.get("metrics").and_then(Value::as_object);
        let (Some(metadata), Some(metrics)) = (metadata, metrics) else {
            continue;
        };
        let Some(model) = metadata
            .get("model_display")
            .and_then(leaderboard_display_label)
            .or_else(|| metadata.get("model").and_then(leaderboard_display_label))
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(score) = metrics.get("accuracy").and_then(number_value) else {
            continue;
        };
        if !score.is_finite() {
            continue;
        }
        let agent = metadata
            .get("agent_display")
            .and_then(leaderboard_display_label)
            .or_else(|| metadata.get("agent").and_then(leaderboard_display_label))
            .unwrap_or_default();
        let effort = metadata
            .get("reasoning_effort")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let model = strip_provider_prefix(&model);
        let kind = if normalize(&agent).contains("mini swe agent") {
            BenchmarkKind::Model
        } else {
            BenchmarkKind::ModelAgent
        };
        let detail = if !agent.is_empty() && !effort.is_empty() {
            format!("{model} [{agent} · {effort}]")
        } else if !agent.is_empty() {
            format!("{model} [{agent}]")
        } else {
            model.clone()
        };
        let mut b = score_benchmark(
            model.clone(),
            score,
            (!agent.is_empty()).then_some(agent),
            (!effort.is_empty()).then_some(effort),
            kind,
        );
        b.slug = model;
        b.name = detail;
        let total_tokens = metrics
            .get("total_tokens")
            .and_then(number_value)
            .or_else(|| {
                let uncached = metrics
                    .get("uncached_input_tokens")
                    .and_then(number_value)
                    .unwrap_or(0.0);
                let cached = metrics
                    .get("cached_input_tokens")
                    .and_then(number_value)
                    .unwrap_or(0.0);
                let output = metrics
                    .get("output_tokens")
                    .and_then(number_value)
                    .unwrap_or(0.0);
                let total = uncached + cached + output;
                (total > 0.0).then_some(total)
            });
        let n_trials = row.get("n_trials").and_then(number_value);
        if let (Some(total_tokens), Some(n_trials)) = (total_tokens, n_trials)
            && total_tokens.is_finite()
            && total_tokens > 0.0
            && n_trials.is_finite()
            && n_trials > 0.0
        {
            b.tokens_per_task = Some(total_tokens / n_trials);
            b.token_profile = Some("Terminal-Bench total tokens ÷ trials".to_owned());
        }
        out.push(b);
    }
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    Ok(out)
}

pub(crate) fn leaderboard_display_label(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_owned());
    }
    value
        .as_object()
        .and_then(|obj| obj.get("label"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub(crate) fn parse_terminal_bench_snapshot(text: &str) -> Result<Vec<Benchmark>> {
    let mut out = Vec::new();
    for block in text.split('{').skip(1) {
        let Some(block) = block.split('}').next() else {
            continue;
        };
        if !block.contains("accuracy:") || !block.contains("model:") {
            continue;
        }
        let status = ts_string_field(block, "status").unwrap_or_else(|| "display".into());
        if !status.eq_ignore_ascii_case("display") && !status.eq_ignore_ascii_case("hide") {
            continue;
        }
        let Some(model_raw) = ts_string_field(block, "model") else {
            continue;
        };
        if model_raw.eq_ignore_ascii_case("multiple") {
            continue;
        }
        let Some(score) = ts_number_field(block, "accuracy") else {
            continue;
        };
        if !score.is_finite() {
            continue;
        }
        let model = strip_provider_prefix(&model_raw);
        let agent = ts_string_field(block, "agent").unwrap_or_default();
        let effort = ts_string_field(block, "reasoningEffort").unwrap_or_default();
        let kind = if normalize(&agent).contains("mini swe agent") {
            BenchmarkKind::Model
        } else {
            BenchmarkKind::ModelAgent
        };
        let detail = if !agent.is_empty() && !effort.is_empty() {
            format!("{model} [{agent} · {effort}]")
        } else if !agent.is_empty() {
            format!("{model} [{agent}]")
        } else {
            model.clone()
        };
        let mut b = score_benchmark(
            model.clone(),
            score,
            (!agent.is_empty()).then_some(agent),
            (!effort.is_empty()).then_some(effort),
            kind,
        );
        b.slug = model;
        b.name = detail;
        out.push(b);
    }
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    Ok(out)
}

pub(crate) fn ts_string_field(block: &str, key: &str) -> Option<String> {
    for line in block.lines() {
        let line = line.trim();
        let prefix = format!("{key}:");
        if !line.starts_with(&prefix) {
            continue;
        }
        let value = line[prefix.len()..].trim().trim_end_matches(',').trim();
        if value == "null" {
            return None;
        }
        return Some(value.trim_matches(|c| c == '\'' || c == '"').to_owned());
    }
    None
}

pub(crate) fn ts_number_field(block: &str, key: &str) -> Option<f64> {
    for line in block.lines() {
        let line = line.trim();
        let prefix = format!("{key}:");
        if !line.starts_with(&prefix) {
            continue;
        }
        return line[prefix.len()..]
            .trim()
            .trim_end_matches(',')
            .trim()
            .parse::<f64>()
            .ok();
    }
    None
}

pub(crate) fn strip_provider_prefix(model: &str) -> String {
    model
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(model)
        .replace('-', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn terminal_bench_preserves_agent_rows_and_hidden_valid_runs() {
        let v = serde_json::json!({"rows": [
            {"status":"display","metadata":{"model_display":{"label":"GPT-5.6 Sol"},"agent_display":{"label":"Codex"},"reasoning_effort":"max"},"metrics":{"accuracy":34.6}},
            {"status":"hide","metadata":{"model_display":{"label":"GPT-5.6 Sol"},"agent_display":{"label":"mini-SWE-agent"},"reasoning_effort":"max"},"metrics":{"accuracy":34.59}}
        ]});
        let rows = parse_terminal_bench_harbor(&v).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().any(|row| row.kind == BenchmarkKind::ModelAgent
                && row.agent.as_deref() == Some("Codex"))
        );
        assert!(rows.iter().any(|row| row.kind == BenchmarkKind::Model
            && row.agent.as_deref() == Some("mini-SWE-agent")));
    }
}
