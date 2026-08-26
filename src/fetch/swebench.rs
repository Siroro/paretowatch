//! SWE-bench Live (JSONL reports) and SWE-bench Verified (leaderboard
//! JSON).

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;

use super::{collapse_whitespace, fetch_json, fetch_text, number_value, score_benchmark};
use crate::bench::matching::{benchmark_model_key, normalize};
use crate::json_shape;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const SWEBENCH_LEADERBOARD_URL: &str = "https://raw.githubusercontent.com/SWE-bench/swe-bench.github.io/master/data/leaderboards.json";
pub(crate) const SWEBENCH_LIVE_REPORTS_URL: &str = "https://raw.githubusercontent.com/SWE-bench-Live/swe-bench-live.github.io/main/reports-0605.jsonl";

pub(crate) fn fetch_swebench_live(client: &Client) -> Result<Vec<Benchmark>> {
    let text = fetch_text(client, SWEBENCH_LIVE_REPORTS_URL, "SWE-bench Live reports")?;
    let rows = parse_swebench_live_jsonl(&text)?;
    if rows.is_empty() {
        return Err(anyhow!("SWE-bench Live contained no usable model/agent rows"));
    }
    Ok(rows)
}

#[derive(Debug, Clone)]
pub(crate) struct SweBenchLiveAggregate {
    pub(crate) model: String,
    pub(crate) agent: String,
    pub(crate) resolved: u64,
    pub(crate) total: u64,
    pub(crate) newest_date: String,
}

pub(crate) fn parse_swebench_live_jsonl(text: &str) -> Result<Vec<Benchmark>> {
    let mut newest: HashMap<(String, String, String), (String, u64, u64, String, String)> = HashMap::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("SWE-bench Live JSONL line {} is invalid", line_no + 1))?;
        let Some(name) = value.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else { continue };
        let Some((agent, model)) = split_swebench_live_label(name) else { continue };
        let set = value.get("set").and_then(Value::as_str).unwrap_or("unknown").to_owned();
        let Some(total) = value.get("total").and_then(Value::as_u64) else { continue };
        if total == 0 { continue; }
        let resolved = match value.get("resolved") {
            Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
            Some(Value::Array(items)) => items.len() as u64,
            _ => 0,
        };
        let date = value.get("date").and_then(Value::as_str).unwrap_or("").to_owned();
        let key = (benchmark_model_key(&model), normalize(&agent), set);
        let replace = newest.get(&key).map(|(old_date, _, _, _, _)| date > *old_date).unwrap_or(true);
        if replace {
            newest.insert(key, (date, resolved, total, model, agent));
        }
    }

    let mut combined: HashMap<(String, String), SweBenchLiveAggregate> = HashMap::new();
    for ((model_key, agent_key, _set), (date, resolved, total, model, agent)) in newest {
        let entry = combined.entry((model_key, agent_key)).or_insert(SweBenchLiveAggregate {
            model,
            agent,
            resolved: 0,
            total: 0,
            newest_date: date.clone(),
        });
        entry.resolved = entry.resolved.saturating_add(resolved);
        entry.total = entry.total.saturating_add(total);
        if date > entry.newest_date { entry.newest_date = date; }
    }

    let mut out = combined.into_values().filter_map(|agg| {
        if agg.total == 0 { return None; }
        let score = agg.resolved as f64 / agg.total as f64 * 100.0;
        let mut b = score_benchmark(
            agg.model.clone(),
            score,
            Some(agg.agent.clone()),
            None,
            BenchmarkKind::ModelAgent,
        );
        b.name = format!("{} [{} · through {}]", agg.model, agg.agent, agg.newest_date);
        Some(b)
    }).collect::<Vec<_>>();
    out.sort_by(|a, b| b.agentic_coding.unwrap_or_default().total_cmp(&a.agentic_coding.unwrap_or_default()));
    Ok(out)
}

pub(crate) fn split_swebench_live_label(label: &str) -> Option<(String, String)> {
    // Most rows are `Agent + Model`, but the public feed also contains compound
    // harness names such as `MIT-IBM Agent + SWE-Agent + Seed-OSS-36B`.
    // Locate the first recognisable model-family token rather than requiring
    // exactly one ` + ` separator.
    let families = [
        "gpt", "claude", "gemini", "grok", "deepseek", "qwen", "kimi",
        "glm", "minimax", "mistral", "llama", "gemma", "seed", "nemotron",
        "mimo", "devstral", "composer",
    ];
    let parts = label.split(" + ").map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let model_index = parts.iter().position(|part| {
        let n = normalize(part);
        families.iter().any(|family| n == *family || n.starts_with(&format!("{family} ")))
    })?;
    let agent = if model_index == 0 {
        "SWE-bench Live harness".to_owned()
    } else {
        parts[..model_index].join(" + ")
    };
    let model = clean_swebench_live_model(&parts[model_index..].join(" + "));
    (!model.is_empty()).then_some((agent, model))
}

pub(crate) fn clean_swebench_live_model(raw: &str) -> String {
    let mut s = raw.replace('-', " ");
    if let Some(index) = s.find(" (") {
        s.truncate(index);
    }
    // If another auxiliary model follows a `+`, keep the primary model only.
    if let Some(index) = s.find(" + ") {
        s.truncate(index);
    }
    collapse_whitespace(&s)
}

pub(crate) fn fetch_swebench_verified(client: &Client) -> Result<Vec<Benchmark>> {
    let value = fetch_json(client, SWEBENCH_LEADERBOARD_URL, "SWE-bench leaderboard")?;
    let rows = parse_swebench_verified_json(&value)?;
    if rows.is_empty() {
        return Err(anyhow!("SWE-bench Verified leaderboard contained no usable scores"));
    }
    Ok(rows)
}

pub(crate) fn parse_swebench_verified_json(root: &Value) -> Result<Vec<Benchmark>> {
    let leaderboards = root.get("leaderboards").and_then(Value::as_array)
        .ok_or_else(|| anyhow!("SWE-bench JSON has no leaderboards array; shape: {}", json_shape(root)))?;
    let verified = leaderboards.iter().find(|board| {
        board.get("name").and_then(Value::as_str)
            .map(|name| name.eq_ignore_ascii_case("verified"))
            .unwrap_or(false)
    }).ok_or_else(|| anyhow!("SWE-bench JSON has no Verified leaderboard"))?;
    let results = verified.get("results").and_then(Value::as_array)
        .ok_or_else(|| anyhow!("SWE-bench Verified has no results array"))?;

    let mut out = Vec::new();
    for row in results {
        if row.get("warning").is_some_and(|v| !v.is_null()) { continue; }
        let Some(name) = row.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else { continue };
        let Some(score) = row.get("resolved").and_then(number_value) else { continue };
        if !score.is_finite() { continue; }
        let slug = swebench_model_slug(name).unwrap_or_else(|| name.to_owned());
        let agent = name.split_once('+')
            .map(|(left, _)| left.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "SWE-bench submission harness".into());
        let mut b = score_benchmark(
            slug.clone(),
            score,
            Some(agent),
            None,
            BenchmarkKind::ModelAgent,
        );
        b.name = name.to_owned();
        b.slug = slug;
        out.push(b);
    }
    Ok(out)
}

pub(crate) fn swebench_model_slug(label: &str) -> Option<String> {
    let tokens = normalize(label).split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let families = ["gpt", "claude", "gemini", "grok", "deepseek", "qwen", "kimi", "glm", "minimax", "mistral", "llama", "gemma"];
    let start = tokens.iter().position(|token| families.iter().any(|family| token.as_str() == *family || token.starts_with(*family)))?;
    let mut picked = Vec::new();
    for token in tokens.iter().skip(start) {
        if picked.len() >= 6 { break; }
        if !picked.is_empty() && matches!(token.as_str(), "agent" | "openhands" | "aider" | "swe" | "verified" | "harness") {
            break;
        }
        picked.push(token.clone());
    }
    (!picked.is_empty()).then(|| picked.join(" "))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn swebench_live_aggregates_latest_rows_across_languages() {
        let jsonl = r#"
    {"name":"SWE-agent + GPT-5.5 (Medium)","set":"go","total":100,"date":"2026-06-05","resolved":40}
    {"name":"SWE-agent + GPT-5.5 (Medium)","set":"go","total":100,"date":"2026-07-05","resolved":50}
    {"name":"SWE-agent + GPT-5.5 (Medium)","set":"rust","total":50,"date":"2026-06-05","resolved":25}
    {"name":"Claude Code + GPT-5.5 (Medium)","set":"go","total":100,"date":"2026-07-05","resolved":60}
    "#;
        let rows = parse_swebench_live_jsonl(jsonl).unwrap();
        let swe = rows.iter().find(|row| row.agent.as_deref() == Some("SWE-agent")).unwrap();
        assert!((swe.agentic_coding.unwrap() - 50.0).abs() < 1e-9);
        assert_eq!(swe.kind, BenchmarkKind::ModelAgent);
    }

    #[test]
    fn swebench_live_extracts_model_from_agent_label() {
        let (agent, model) = split_swebench_live_label("Slingshot-v3.1.0 + GPT-5.5 (Medium)").unwrap();
        assert_eq!(agent, "Slingshot-v3.1.0");
        assert_eq!(benchmark_model_key(&model), "gpt 5 5");
    
        let (agent, model) = split_swebench_live_label("MIT-IBM Agent (BOAD) + SWE-Agent + Seed-OSS-36B").unwrap();
        assert!(agent.contains("MIT-IBM"));
        assert_eq!(benchmark_model_key(&model), "seed oss 36b");
    }

}
