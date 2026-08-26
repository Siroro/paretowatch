//! Revelo Code Index: scrapes the research page's HTML table.

use anyhow::Result;
use regex::Regex;
use reqwest::blocking::Client;

use super::{collapse_whitespace, fetch_text, score_benchmark, strip_html_tags};
use crate::types::REVELO_CODE_INDEX_URL;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) fn fetch_revelo_code_index(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(client, REVELO_CODE_INDEX_URL, "Revelo Code Index")?;
    let rows = parse_revelo_code_index_html(&html);
    if rows.len() >= 3 {
        Ok(rows)
    } else {
        Ok(revelo_code_index_fallback())
    }
}

pub(crate) fn parse_revelo_code_index_html(html: &str) -> Vec<Benchmark> {
    let text = collapse_whitespace(&strip_html_tags(html));
    let re = Regex::new(
        r"(?i)([a-z0-9][a-z0-9._-]{2,80})\s+(claude-code|codex|terminus-2)\s+[0-9.]+\s*[BMK]?\s*/\s*[0-9.]+\s*[BMK]?\s+\$[0-9.]+\s*[kKmM]?\s+(\d{1,3}(?:\.\d+)?)"
    ).expect("valid Revelo regex");
    let mut out = Vec::new();
    for caps in re.captures_iter(&text) {
        let Some(model) = caps.get(1).map(|m| m.as_str().to_owned()) else {
            continue;
        };
        let Some(agent) = caps.get(2).map(|m| m.as_str().to_owned()) else {
            continue;
        };
        let Some(score) = caps.get(3).and_then(|m| m.as_str().parse::<f64>().ok()) else {
            continue;
        };
        let mut b = score_benchmark(
            model.clone(),
            score,
            Some(agent.clone()),
            None,
            BenchmarkKind::ModelAgent,
        );
        b.name = format!("{model} [{agent}]");
        out.push(b);
    }
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    out
}

pub(crate) fn revelo_code_index_fallback() -> Vec<Benchmark> {
    [
        ("claude-opus-5", "claude-code", 57.1),
        ("gpt-5.6-sol", "codex", 51.2),
        ("claude-fable-5", "claude-code", 50.0),
        ("claude-opus-4.8", "claude-code", 41.2),
        ("kimi-k3", "terminus-2", 39.6),
        ("gpt-5.6-terra", "codex", 37.1),
        ("qwen3.8-max", "claude-code", 37.1),
        ("gpt-5.6-luna", "codex", 36.2),
        ("grok-4.5", "terminus-2", 35.8),
        ("glm-5.2", "terminus-2", 30.8),
        ("deepseek-v4-flash-0731", "terminus-2", 30.4),
        ("gemini-3.6-flash", "terminus-2", 28.3),
        ("deepseek-v4-pro", "terminus-2", 19.6),
        ("hy3", "terminus-2", 19.6),
        ("minimax-m3", "terminus-2", 17.9),
        ("mimo-v2.5-pro", "terminus-2", 11.2),
        ("nemotron-3-ultra-550b-a55b", "terminus-2", 9.2),
    ]
    .into_iter()
    .map(|(model, agent, score)| {
        let mut b = score_benchmark(
            model,
            score,
            Some(agent.into()),
            None,
            BenchmarkKind::ModelAgent,
        );
        b.name = format!("{model} [{agent}]");
        b
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_revelo_code_index_rows() {
        let html = r#"<div>claude-opus-5 claude-code 1.35B / 22.3M $1.5k 57.1</div><div>gpt-5.6-sol codex 469.2M / 6.1M $492.7 51.2</div><div>glm-5.2 terminus-2 727.8M / 24.0M $446.0 30.8</div>"#;
        let rows = parse_revelo_code_index_html(html);
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter()
                .any(|row| row.slug == "gpt-5.6-sol" && row.agent.as_deref() == Some("codex"))
        );
    }
}
