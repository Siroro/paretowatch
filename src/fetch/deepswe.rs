//! DeepSWE v1.1 leaderboard (Datacurve) JSON feed.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde_json::Value;

use super::{fetch_json, number_value, score_benchmark};
use crate::{format_compact_number, json_shape};
use crate::bench::matching::{model_core, normalize};
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const DEEPSWE_LEADERBOARD_URL: &str = "https://deepswe.datacurve.ai/artifacts/v1.1/leaderboard-live.json";

pub(crate) fn fetch_deepswe(client: &Client) -> Result<Vec<Benchmark>> {
    let value = fetch_json(client, DEEPSWE_LEADERBOARD_URL, "DeepSWE v1.1 leaderboard")?;
    let rows = parse_deepswe_json(&value)?;
    if rows.is_empty() {
        return Err(anyhow!("DeepSWE v1.1 contained no usable model scores"));
    }
    Ok(rows)
}

pub(crate) fn parse_deepswe_json(root: &Value) -> Result<Vec<Benchmark>> {
    let rows = root.get("rows").and_then(Value::as_array)
        .ok_or_else(|| anyhow!("DeepSWE JSON has no rows array; shape: {}", json_shape(root)))?;
    let mut best: HashMap<String, (i32, Benchmark)> = HashMap::new();
    for row in rows {
        let Some(model) = row.get("model").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else { continue };
        let harness = row.get("harness").and_then(Value::as_str).unwrap_or("mini-SWE-agent").trim();
        if !harness.is_empty() && !normalize(harness).contains("mini swe agent") {
            continue;
        }
        let Some(raw_score) = row.get("pass_at_1").and_then(number_value)
            .or_else(|| row.get("pass_rate").and_then(number_value)) else { continue };
        let score = if raw_score <= 1.0 { raw_score * 100.0 } else { raw_score };
        if !score.is_finite() { continue; }
        let effort = row.get("reasoning_effort").and_then(Value::as_str).unwrap_or("default");
        let rank = reasoning_effort_rank(effort);
        let key = model_core(model);
        if key.is_empty() { continue; }
        let mut b = score_benchmark(
            model,
            score,
            Some(if harness.is_empty() { "mini-SWE-agent".into() } else { harness.to_owned() }),
            Some(effort.to_owned()),
            BenchmarkKind::Model,
        );
        b.name = format!("{model} [mini-SWE-agent · {effort}]");
        let mean_input = row.get("mean_input_tokens").and_then(number_value);
        let mean_output = row.get("mean_output_tokens").and_then(number_value);
        if let (Some(input), Some(output)) = (mean_input, mean_output) {
            let tokens = input + output;
            if tokens.is_finite() && tokens > 0.0 {
                b.tokens_per_task = Some(tokens);
                b.token_profile = Some(format!(
                    "DeepSWE mean tokens/attempt · {:.1}M input + {} output",
                    input / 1_000_000.0,
                    format_compact_number(output),
                ));
            }
        }
        let replace = best
            .get(&key)
            .and_then(|(old_rank, old)| old.agentic_coding.map(|old_score| rank > *old_rank || (rank == *old_rank && score > old_score)))
            .unwrap_or(true);
        if replace { best.insert(key, (rank, b)); }
    }
    let mut out = best.into_values().map(|(_, b)| b).collect::<Vec<_>>();
    out.sort_by(|a, b| b.agentic_coding.unwrap_or_default().total_cmp(&a.agentic_coding.unwrap_or_default()));
    Ok(out)
}

pub(crate) fn reasoning_effort_rank(effort: &str) -> i32 {
    match normalize(effort).as_str() {
        "max" => 60,
        "xhigh" | "extra high" => 50,
        "high" => 40,
        "adaptive" | "thinking" => 35,
        "medium" | "default" => 30,
        "low" => 20,
        "minimal" => 15,
        "none" => 10,
        _ => 25,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn parses_deepswe_and_prefers_highest_effort() {
        let v = serde_json::json!({"rows": [
            {"model":"gpt-5-6-sol","harness":"mini-swe-agent","reasoning_effort":"high","pass_at_1":0.80,"mean_input_tokens":2000000,"mean_output_tokens":50000},
            {"model":"gpt-5-6-sol","harness":"mini-swe-agent","reasoning_effort":"max","pass_at_1":0.7267,"mean_input_tokens":7907652,"mean_output_tokens":60014},
            {"model":"gpt-5-6-sol","harness":"other-agent","reasoning_effort":"max","pass_at_1":0.99}
        ]});
        let rows = parse_deepswe_json(&v).unwrap();
        assert_eq!(rows.len(), 1);
        assert!((rows[0].agentic_coding.unwrap() - 72.67).abs() < 1e-9);
        assert_eq!(rows[0].kind, BenchmarkKind::Model);
        assert!(rows[0].agent.as_deref().unwrap().contains("mini"));
        assert_eq!(rows[0].tokens_per_task, Some(7_967_666.0));
    }

}
