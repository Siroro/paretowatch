//! FrontierCode — Cognition's mergeability benchmark. Maintainer-built tasks
//! graded on whether the pull request would actually merge (correctness, test
//! quality, scope discipline, style), so the score describes a model inside
//! its vendor harness (claude-code, codex, chisel, grok-build, ...). The
//! leaderboard page loads from a static JSON export; the headline number is
//! the best `new_score` across reasoning efforts on the `main` subset.

use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

use super::fetch_json;
use crate::theme::{canonical_group, infer_creator};
use crate::types::{Benchmark, BenchmarkKind};

const FRONTIERCODE_DATA_URL: &str = "https://cognition.com/data/frontiercode-leaderboard/data.json";

/// Reasoning efforts the source names as tiers. Anything else (`none`, or a
/// raw parameter like `0.99`) is a run configuration, not an effort label.
const NAMED_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub(crate) fn fetch_frontiercode(client: &Client) -> Result<Vec<Benchmark>> {
    let root = fetch_json(client, FRONTIERCODE_DATA_URL, "FrontierCode leaderboard")?;
    let rows = parse_frontiercode(&root);
    if rows.is_empty() {
        return Err(anyhow!(
            "FrontierCode leaderboard export had no scored models"
        ));
    }
    Ok(rows)
}

pub(crate) fn parse_frontiercode(root: &Value) -> Vec<Benchmark> {
    // `v1_1` is FrontierCode 1.1 (current); `v1` keeps the original 1.0
    // results. Both carry the same schema.
    let Some(board) = root.get("v1_1").or_else(|| root.get("v1")) else {
        return Vec::new();
    };
    let Some(data) = board.get("data").and_then(Value::as_object) else {
        return Vec::new();
    };

    // Model → chart color and lab → color let the board name each model's lab
    // (including harness-native models like SWE-1.7 → Cognition that the
    // catalogue-side name heuristics do not know).
    let colors = board.get("colors").and_then(Value::as_object);
    let mut lab_by_color: Vec<(String, String)> = Vec::new();
    if let Some(labs) = board.get("lab_colors").and_then(Value::as_object) {
        for (lab, color) in labs {
            if let Some(hex) = color.as_str() {
                lab_by_color.push((hex.to_lowercase(), canonical_group(lab).to_owned()));
            }
        }
    }
    let creator_for = |model: &str| -> String {
        let Some(hex) = colors
            .and_then(|colors| colors.get(model))
            .and_then(Value::as_str)
        else {
            return infer_creator(model);
        };
        // Two labs sharing a chart color would make the lookup ambiguous;
        // defer to the name heuristic rather than guess.
        let mut hits = lab_by_color
            .iter()
            .filter(|(c, _)| *c == hex.to_lowercase());
        match (hits.next(), hits.next()) {
            (Some((_, lab)), None) => lab.clone(),
            _ => infer_creator(model),
        }
    };

    let mut out = Vec::new();
    for (model, efforts) in data {
        let mut best: Option<(&str, f64, Option<f64>)> = None;
        for (effort, subsets) in efforts.as_object().into_iter().flatten() {
            let Some(entry) = subsets.get("main") else {
                continue;
            };
            let Some(score) = entry.get("new_score").and_then(Value::as_f64) else {
                continue;
            };
            if !score.is_finite() {
                continue;
            }
            let tokens = entry.get("tokens").and_then(Value::as_f64);
            if best.is_none_or(|(_, best_score, _)| score > best_score) {
                best = Some((effort.as_str(), score, tokens));
            }
        }
        let Some((effort, score, tokens)) = best else {
            continue;
        };
        let agent = board
            .get("harness")
            .and_then(|h| h.get(model))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let reasoning_effort = NAMED_EFFORTS.contains(&effort).then_some(effort.to_owned());
        let score = (score * 100.0).clamp(0.0, 100.0);
        let mut b = super::score_benchmark(
            slugify(model),
            score,
            agent.clone(),
            reasoning_effort,
            if agent.is_some() {
                BenchmarkKind::ModelAgent
            } else {
                BenchmarkKind::Model
            },
        );
        b.name = model.clone();
        b.creator = creator_for(model);
        b.tokens_per_task = tokens.filter(|t| t.is_finite() && *t > 0.0);
        b.token_profile = b
            .tokens_per_task
            .map(|_| "FrontierCode avg tokens/task (main subset)".to_owned());
        out.push(b);
    }
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    out
}

fn slugify(name: &str) -> String {
    name.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::best_benchmark_match;
    use crate::testfix::test_quote;
    use serde_json::json;

    fn board_json() -> Value {
        json!({
            "v1": {
                "models": ["Claude Fable 5"],
                "colors": {"Claude Fable 5": "#abc"},
                "lab_colors": {"Anthropic": "#ABC"},
                "harness": {"Claude Fable 5": "claude-code"},
                "efforts": {"Claude Fable 5": ["high"]},
                "subsets": {"main": 100, "extended": 150},
                "data": {"Claude Fable 5": {"high": {"main": {"correct": 0.462, "new_score": 0.433, "tokens": 34776.9, "cost": 8.2}}}}
            },
            "v1_1": {
                "models": ["Claude Fable 5", "Kimi K3", "SWE-1.7"],
                "colors": {"Claude Fable 5": "#abc", "Kimi K3": "#def", "SWE-1.7": "#123"},
                "lab_colors": {"Anthropic": "#ABC", "Moonshot": "#DeF", "Cognition": "#123"},
                "harness": {"Claude Fable 5": "claude-code", "Kimi K3": "mini-swe-agent", "SWE-1.7": "chisel"},
                "efforts": {"Claude Fable 5": ["medium", "xhigh"], "Kimi K3": ["none"]},
                "subsets": {"main": 100, "extended": 150},
                "data": {
                    "Claude Fable 5": {
                        "medium": {"main": {"correct": 0.546, "new_score": 0.4982, "tokens": 33229.9, "cost": 7.2}, "extended": {"new_score": 0.6279}},
                        "xhigh": {"main": {"correct": 0.5766, "new_score": 0.5273, "tokens": 43482.8, "cost": 9.5}}
                    },
                    "Kimi K3": {
                        "none": {"main": {"correct": 0.4891, "new_score": 0.4417, "tokens": 53607.2, "cost": 1.0}}
                    },
                    "SWE-1.7": {
                        "none": {"main": {"correct": 0.474, "new_score": 0.4199, "tokens": 64756.6, "cost": 1.0}}
                    }
                }
            }
        })
    }

    #[test]
    fn parses_best_effort_main_subset_as_percent() {
        let rows = parse_frontiercode(&board_json());
        assert_eq!(rows.len(), 3);
        // Sorted desc: Fable 5's xhigh main-subset 0.5273 → 52.7.
        assert_eq!(rows[0].name, "Claude Fable 5");
        assert!((rows[0].agentic_coding.unwrap() - 52.73).abs() < 1e-9);
        assert_eq!(rows[0].agent.as_deref(), Some("claude-code"));
        assert_eq!(rows[0].reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(rows[0].kind, BenchmarkKind::ModelAgent);
        // Tokens come from the winning effort's main-subset run.
        assert!((rows[0].tokens_per_task.unwrap() - 43_482.8).abs() < 1e-9);
        // `none` is a run configuration, not an effort tier.
        let k3 = rows.iter().find(|r| r.name == "Kimi K3").unwrap();
        assert_eq!(k3.reasoning_effort, None);
        assert!((k3.agentic_coding.unwrap() - 44.17).abs() < 1e-9);
    }

    #[test]
    fn lab_colors_name_harness_native_creators() {
        let rows = parse_frontiercode(&board_json());
        let swe = rows.iter().find(|r| r.name == "SWE-1.7").unwrap();
        assert_eq!(swe.creator, "Cognition");
        let fable = rows.iter().find(|r| r.name == "Claude Fable 5").unwrap();
        assert_eq!(fable.creator, "Anthropic");
    }

    #[test]
    fn falls_back_to_v1_board_and_name_heuristics() {
        let mut root = board_json();
        root.as_object_mut().unwrap().remove("v1_1");
        let rows = parse_frontiercode(&root);
        assert_eq!(rows.len(), 1);
        assert!((rows[0].agentic_coding.unwrap() - 43.3).abs() < 1e-9);
    }

    #[test]
    fn display_names_join_surplus_quotes() {
        let rows = parse_frontiercode(&board_json());
        let q = test_quote("claude-fable-5", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }

    #[test]
    fn slugify_keeps_versions_and_drops_punctuation() {
        assert_eq!(slugify("Claude Fable 5.1"), "claude-fable-5-1");
        assert_eq!(slugify("DeepSeek V4 Flash 0731"), "deepseek-v4-flash-0731");
        assert_eq!(slugify("SWE-1.7"), "swe-1-7");
    }
}
