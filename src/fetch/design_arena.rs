//! Frontend Design Elo — Design Arena's crowdsourced leaderboard. Blinded
//! pairwise votes on model-generated frontend designs produce an Elo rating
//! per model; ParetoWatch reads the public "all categories" board so the
//! score summarizes overall design standing rather than one prompt type.

use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::fetch_json_post;
use crate::theme::infer_creator;
use crate::types::{Benchmark, BenchmarkKind};

const DESIGN_ARENA_LEADERBOARD_URL: &str = "https://www.designarena.ai/api/leaderboard";
/// Elo from a handful of votes is near-random; skip fresh entrants until the
/// crowd has actually spoken.
const DESIGN_ARENA_MIN_BATTLES: u64 = 30;

pub(crate) fn fetch_design_arena(client: &Client) -> Result<Vec<Benchmark>> {
    let body = json!({
        "arenaType": "models",
        "category": "allcategories",
        "variationName": "public",
    });
    let value = fetch_json_post(
        client,
        DESIGN_ARENA_LEADERBOARD_URL,
        &body,
        "Design Arena leaderboard",
    )?;
    require_usable_rows(parse_design_arena_elo(&value))
}

/// An unusable payload (no `data`, or every row battle-filtered) parses to an
/// empty set; that must surface as an error rather than `Ok(vec![])`, which
/// the cache would save in place of the previous board.
fn require_usable_rows(rows: Vec<Benchmark>) -> Result<Vec<Benchmark>> {
    if rows.is_empty() {
        return Err(anyhow!(
            "Design Arena leaderboard contained no usable model scores"
        ));
    }
    Ok(rows)
}

pub(crate) fn parse_design_arena_elo(root: &Value) -> Vec<Benchmark> {
    let Some(rows) = root.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<Benchmark> = rows
        .iter()
        .filter_map(|row| {
            let id = row.get("modelId")?.as_str()?;
            let elo = row.get("elo")?.as_f64()?;
            let battles = row.get("battles").and_then(Value::as_u64).unwrap_or(0);
            if battles < DESIGN_ARENA_MIN_BATTLES {
                return None;
            }
            Some(Benchmark {
                slug: id.to_owned(),
                name: id.to_owned(),
                creator: infer_creator(id),
                // Elo is the board's only number, so it feeds every metric
                // selector like other single-score boards.
                overall: Some(elo),
                coding: Some(elo),
                agentic_coding: Some(elo),
                agent: None,
                reasoning_effort: None,
                kind: BenchmarkKind::Model,
                tokens_per_task: None,
                token_profile: None,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let (Some(x), Some(y)) = (a.agentic_coding, b.agentic_coding) else {
            return std::cmp::Ordering::Equal;
        };
        y.total_cmp(&x)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::best_benchmark_match;
    use crate::testfix::test_quote;

    #[test]
    fn parses_design_arena_elo_sorted_and_skips_thin_evidence() {
        let v = serde_json::json!({
            "success": true,
            "arenaType": "models",
            "category": "allcategories",
            "data": [
                {"modelId": "gpt-5.6-sol", "wins": 4558, "losses": 3276, "battles": 7834,
                 "winRate": 58.2, "elo": 1349, "btStdErr": null, "avgGenerationTimeMs": 162935},
                {"modelId": "kimi-k3", "wins": 1209, "losses": 718, "battles": 1927,
                 "winRate": 62.7, "elo": 1404, "btStdErr": null, "avgGenerationTimeMs": 652916},
                {"modelId": "brand-new-model", "wins": 1, "losses": 1, "battles": 2,
                 "winRate": 50.0, "elo": 1500, "btStdErr": null, "avgGenerationTimeMs": 1000}
            ]
        });
        let rows = parse_design_arena_elo(&v);
        assert_eq!(rows.len(), 2, "2-battle entrant must be skipped");
        assert_eq!(rows[0].slug, "kimi-k3", "rows must be sorted by Elo");
        assert_eq!(rows[0].agentic_coding, Some(1404.0));
        assert_eq!(rows[1].slug, "gpt-5.6-sol");
        assert_eq!(rows[0].kind, BenchmarkKind::Model);

        // Arena slugs join Surplus quotes through the canonical-key matcher.
        let q = test_quote("kimi-k3", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }

    #[test]
    fn fetch_path_errors_on_an_empty_parse() {
        // fetch_design_arena funnels every parse through require_usable_rows:
        // an unusable payload must surface as Err from the fetch path instead
        // of Ok(vec![]), which would wipe the cached board.
        let no_data = serde_json::json!({"success": true});
        assert!(require_usable_rows(parse_design_arena_elo(&no_data)).is_err());
        let all_filtered = serde_json::json!({"data": [
            {"modelId": "brand-new-model", "elo": 1500, "battles": 2}
        ]});
        assert!(require_usable_rows(parse_design_arena_elo(&all_filtered)).is_err());
        let usable = serde_json::json!({"data": [
            {"modelId": "kimi-k3", "elo": 1404, "battles": 1927}
        ]});
        let rows = require_usable_rows(parse_design_arena_elo(&usable)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slug, "kimi-k3");
    }
}
