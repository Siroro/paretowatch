//! SWE-rebench leaderboard: scrapes and parses the public HTML table.

use std::collections::HashMap;

use anyhow::Result;
use regex::Regex;
use reqwest::blocking::Client;

use super::{collapse_whitespace, fetch_text, score_benchmark, strip_html_tags};
use crate::bench::matching::{benchmark_model_key, normalize};
use crate::types::SWE_REBENCH_URL;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) fn fetch_swe_rebench(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(client, SWE_REBENCH_URL, "SWE-rebench leaderboard")?;
    let parsed = parse_swe_rebench_html(&html);
    if parsed.len() >= 3 {
        return Ok(parsed);
    }
    Ok(swe_rebench_fallback())
}

pub(crate) fn parse_swe_rebench_html(html: &str) -> Vec<Benchmark> {
    let text = collapse_whitespace(&strip_html_tags(html));

    // Current leaderboard rows render as:
    // `1 Fable 5 [high]Model 64.5%± 1.41% 78.4% $4.40 2,518,308 94.9% cached`.
    // Parse the entire row so surrounding headings/table text cannot leak into
    // the model name (which made otherwise-correct scores impossible to join).
    let row_re = Regex::new(
        r"(?i)(?:^|\s)(?:\d{1,3}\s+)?([A-Za-z][A-Za-z0-9 ._-]{1,56}?)(?:\s+\[([^\]]+)\])?\s*Model\s+(\d{1,3}(?:\.\d+)?)%\s*±\s*[0-9.]+%\s+\d{1,3}(?:\.\d+)?%\s+\$[0-9.,]+\s+([0-9,]+)\s+(\d{1,3}(?:\.\d+)?)%\s+cached"
    ).expect("valid SWE-rebench row regex");

    let mut best: HashMap<String, Benchmark> = HashMap::new();
    for caps in row_re.captures_iter(&text) {
        let Some(raw_model) = caps.get(1).map(|m| m.as_str().trim()) else {
            continue;
        };
        let model = clean_swe_rebench_model(raw_model);
        if model.is_empty() || normalize(&model).contains("agent") {
            continue;
        }
        let Some(score) = caps.get(3).and_then(|m| m.as_str().parse::<f64>().ok()) else {
            continue;
        };
        let effort = caps
            .get(2)
            .map(|m| m.as_str().trim().to_owned())
            .filter(|v| !v.is_empty());
        let tokens = caps
            .get(4)
            .map(|m| m.as_str().replace(',', ""))
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0);
        let cached_pct = caps.get(5).and_then(|m| m.as_str().parse::<f64>().ok());

        let mut row = score_benchmark(model.clone(), score, None, effort, BenchmarkKind::Model);
        if let Some(tokens) = tokens {
            row.tokens_per_task = Some(tokens);
            row.token_profile = Some(match cached_pct {
                Some(cached) => format!("SWE-rebench tokens/problem · {cached:.1}% cached"),
                None => "SWE-rebench tokens/problem".to_owned(),
            });
        }
        insert_best_benchmark(&mut best, row);
    }

    // Keep a looser compatibility parser for older SWE-rebench page revisions.
    if best.len() < 3 {
        let legacy_re = Regex::new(
            r"(?i)([A-Za-z][A-Za-z0-9 ._-]{1,44}?)(?:\s+\[([^\]]+)\])?\s*Model\s+(\d{1,3}(?:\.\d+)?)%\s*±"
        ).expect("valid SWE-rebench compatibility regex");
        for caps in legacy_re.captures_iter(&text) {
            let Some(raw_model) = caps.get(1).map(|m| m.as_str().trim()) else {
                continue;
            };
            let model = clean_swe_rebench_model(raw_model);
            if model.is_empty() || normalize(&model).contains("agent") {
                continue;
            }
            let Some(score) = caps.get(3).and_then(|m| m.as_str().parse::<f64>().ok()) else {
                continue;
            };
            let effort = caps
                .get(2)
                .map(|m| m.as_str().trim().to_owned())
                .filter(|v| !v.is_empty());
            insert_best_benchmark(
                &mut best,
                score_benchmark(model, score, None, effort, BenchmarkKind::Model),
            );
        }
    }

    let mut out = best.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    out
}

pub(crate) fn insert_best_benchmark(best: &mut HashMap<String, Benchmark>, row: Benchmark) {
    let key = benchmark_model_key(&row.slug);
    if key.is_empty() {
        return;
    }
    let score = row.agentic_coding.unwrap_or(f64::NEG_INFINITY);
    let replace = best
        .get(&key)
        .and_then(|old| old.agentic_coding)
        .map(|old| score > old)
        .unwrap_or(true);
    if replace {
        best.insert(key, row);
    }
}

pub(crate) fn clean_swe_rebench_model(raw: &str) -> String {
    let mut s = raw.trim().to_owned();
    // The live table renders mini-leaderboard badges inside some model cells;
    // after tag-stripping they glue onto the name (`Tokens per Problem 1
    // Anthropic Fable 5`). Cut the badge label plus its rank digit so the
    // row joins the model it belongs to — otherwise the model silently loses
    // this board's evidence.
    for label in [
        "tokens per problem",
        "cost per problem",
        "problems per dollar",
    ] {
        let n = normalize(&s);
        if n.starts_with(label) {
            let rest = s.split_whitespace().skip(3).collect::<Vec<_>>();
            let rest = if rest
                .first()
                .is_some_and(|t| t.chars().all(|c| c.is_ascii_digit()))
            {
                &rest[1..]
            } else {
                &rest[..]
            };
            s = rest.join(" ");
            break;
        }
    }
    // Strip vendor prefixes only when they are distinct from the actual model
    // family. Do not strip `Grok`, `DeepSeek`, or `MiniMax`: those words are
    // themselves part of the canonical model name.
    for prefix in [
        "Anthropic",
        "OpenAI",
        "Z.ai",
        "Z AI",
        "Xiaomi",
        "Google",
        "Alibaba",
        "Moonshot",
        "NVIDIA",
    ] {
        if s.len() > prefix.len()
            && s.get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            s = s.get(prefix.len()..).unwrap_or_default().trim().to_owned();
            break;
        }
    }
    if normalize(&s).starts_with("grok grok ") {
        s = s.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
    }
    let mut tokens = s.split_whitespace().collect::<Vec<_>>();
    if tokens
        .first()
        .is_some_and(|token| token.chars().all(|c| c.is_ascii_digit()))
    {
        tokens.remove(0);
        s = tokens.join(" ");
    }
    s
}

pub(crate) fn swe_rebench_fallback() -> Vec<Benchmark> {
    [
        ("Fable 5", 64.5, Some("high"), 2_518_308.0, 94.9),
        ("Grok 4.5", 63.8, Some("high"), 2_429_424.0, 92.6),
        ("Opus 5", 63.4, Some("high"), 4_322_143.0, 95.7),
        ("GLM-5.2", 62.9, Some("high"), 5_524_892.0, 92.1),
        ("GPT-5.6 Sol", 62.3, Some("medium"), 605_340.0, 84.7),
        ("Sonnet 5", 56.8, Some("high"), 4_645_617.0, 96.4),
        ("MiniMax M3", 47.2, None, 13_869_459.0, 97.0),
        ("MiMo V2.5 Pro", 46.5, None, 4_687_987.0, 95.7),
        ("GPT-5.6 Luna", 43.6, Some("medium"), 395_522.0, 85.2),
    ]
    .into_iter()
    .map(|(model, score, effort, tokens, cached)| {
        let mut row = score_benchmark(
            model,
            score,
            None,
            effort.map(str::to_owned),
            BenchmarkKind::Model,
        );
        row.tokens_per_task = Some(tokens);
        row.token_profile = Some(format!(
            "SWE-rebench tokens/problem · {cached:.1}% cached (fallback snapshot)"
        ));
        row
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_swe_rebench_model_rows() {
        let html = r#"<table><tr><td>1</td><td>GPT-5.6 Sol [medium]<span>Model</span></td><td>62.3% <span>± 1.83%</span></td></tr><tr><td>2</td><td>Grok 4.5 [high]<span>Model</span></td><td>63.8% <span>± 0.60%</span></td></tr></table>"#;
        let rows = parse_swe_rebench_html(html);
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .any(|row| benchmark_model_key(&row.slug) == "gpt 5 6 sol"
                    && (row.agentic_coding.unwrap() - 62.3).abs() < 1e-9)
        );
        assert!(
            rows.iter()
                .any(|row| benchmark_model_key(&row.slug) == "grok 4 5"
                    && (row.agentic_coding.unwrap() - 63.8).abs() < 1e-9)
        );
    }

    #[test]
    fn parses_swe_rebench_tokens_per_problem() {
        let html = r#"<table><tr><td>GPT-5.6 Sol [medium]<span>Model</span></td><td>62.3% ± 1.83%</td><td>79.3%</td><td>$0.85</td><td>605,340 84.7% cached</td></tr></table>"#;
        let rows = parse_swe_rebench_html(html);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tokens_per_task, Some(605_340.0));
        assert!(
            rows[0]
                .token_profile
                .as_deref()
                .unwrap_or_default()
                .contains("84.7% cached")
        );
    }

    #[test]
    fn badge_prefixed_model_cell_still_joins_the_model() {
        // Regression (live 2026-08-26): the rebench table renders a
        // `Tokens per Problem 1` mini-leaderboard badge inside Fable 5's
        // model cell; after tag-stripping it glued onto the name and the
        // row scored a phantom model, silently dropping Fable's #1 board.
        let html = r#"<table><tr><td>Tokens per Problem 1 Anthropic Fable 5 [high]<span>Model</span></td><td>64.5% ± 1.41%</td><td>78.4%</td><td>$4.40</td><td>2,518,308 94.9% cached</td></tr></table>"#;
        let rows = parse_swe_rebench_html(html);
        assert_eq!(rows.len(), 1, "badge row must parse as exactly one model");
        assert_eq!(benchmark_model_key(&rows[0].slug), "fable 5");
        assert!((rows[0].agentic_coding.unwrap() - 64.5).abs() < 1e-9);
        // The cleaned key must be exactly joinable with the Surplus quote.
        assert_eq!(
            benchmark_model_key("anthropic/claude-fable-5"),
            benchmark_model_key(&rows[0].slug)
        );
    }
}
