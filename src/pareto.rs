//! The Pareto view model: joining Surplus quotes onto benchmark rows, the
//! cost/score frontier, the cache that keys it, and the plot-axis math the
//! chart needs.

use crate::bench::{best_benchmark_match, normalize};
use crate::types::{
    Benchmark, BenchmarkMetric, BenchmarkSource, ComparisonMode, CostBasis, LiquidityFilter,
    ModalityFilter, PriceMetric, Quote,
};

#[derive(Debug, Clone)]
pub(crate) struct JoinedPoint {
    pub(crate) model_id: String,
    pub(crate) model: String,
    pub(crate) creator: String,
    pub(crate) provider: String,
    pub(crate) benchmark_name: String,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_read: Option<f64>,
    pub(crate) cost: f64,
    pub(crate) tokens_per_task: Option<f64>,
    pub(crate) token_profile: Option<String>,
    pub(crate) score: f64,
    pub(crate) live_market: bool,
    pub(crate) free_offer_listed: bool,
    pub(crate) vision: bool,
}

/// Inputs that fully determine the derived Pareto view. egui redraws on every
/// hover/interaction, so anything derived from these must be cached instead of
/// recomputed per frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParetoCacheKey {
    pub(crate) data_version: u64,
    pub(crate) price_metric: PriceMetric,
    pub(crate) cost_basis: CostBasis,
    pub(crate) benchmark_source: BenchmarkSource,
    pub(crate) benchmark_metric: BenchmarkMetric,
    pub(crate) comparison_mode: ComparisonMode,
    pub(crate) common_scaffold: String,
    pub(crate) liquidity_filter: LiquidityFilter,
    pub(crate) modality_filter: ModalityFilter,
    pub(crate) input_weight: f64,
    pub(crate) cache_read_weight: f64,
    pub(crate) output_weight: f64,
}

/// Cached join/frontier for the current inputs. Kept behind an `Arc` in the
/// app so handing it to the UI each frame is a refcount bump instead of a
/// deep clone of every joined point.
#[derive(Debug, Clone)]
pub(crate) struct ParetoCache {
    pub(crate) key: ParetoCacheKey,
    pub(crate) benchmarks_present: bool,
    pub(crate) filtered_quotes: Vec<Quote>,
    pub(crate) joined: Vec<JoinedPoint>,
    pub(crate) frontier: Vec<JoinedPoint>,
}

pub(crate) fn price_to_plot_x(price: f64, log_scale: bool) -> f64 {
    if log_scale {
        // A tiny positive floor keeps genuinely-free/zero-priced observations
        // representable on a logarithmic chart without producing -inf.
        price.max(1e-6).log10()
    } else {
        price
    }
}

pub(crate) fn price_from_plot_x(x: f64, log_scale: bool) -> f64 {
    if log_scale { 10_f64.powf(x) } else { x }
}
pub(crate) fn joined_points(
    quotes: &[Quote],
    benchmarks: &[Benchmark],
    price_metric: PriceMetric,
    cost_basis: CostBasis,
    benchmark_metric: BenchmarkMetric,
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
) -> Vec<JoinedPoint> {
    let mut out = vec![];
    for q in quotes {
        if let Some(b) = best_benchmark_match(q, benchmarks) {
            if let Some(score) = benchmark_metric.value(b) {
                let per_million =
                    q.price(price_metric, input_weight, cache_read_weight, output_weight);
                let cost = match cost_basis {
                    CostBasis::PerMillion => per_million,
                    CostBasis::EstimatedPerTask => {
                        // Cost/task is intentionally only available for the blended
                        // rate. The benchmark contributes observed token volume; all
                        // dollar pricing comes from the current Surplus quote.
                        if price_metric != PriceMetric::Blended {
                            continue;
                        }
                        let Some(tokens) = b
                            .tokens_per_task
                            .filter(|tokens| tokens.is_finite() && *tokens > 0.0)
                        else {
                            continue;
                        };
                        per_million * tokens / 1_000_000.0
                    }
                };
                if cost.is_finite() && score.is_finite() {
                    out.push(JoinedPoint {
                        model_id: q.model.clone(),
                        model: q.display_name.clone(),
                        creator: q.creator.clone(),
                        provider: q.provider.clone(),
                        benchmark_name: b.name.clone(),
                        input: q.input,
                        output: q.output,
                        cache_read: q.cache_read,
                        cost,
                        tokens_per_task: b.tokens_per_task,
                        token_profile: b.token_profile.clone(),
                        score,
                        live_market: q.live_market,
                        free_offer_listed: q.free_offer_listed,
                        vision: q.vision,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    out
}

pub(crate) fn pareto_frontier(points: &[JoinedPoint]) -> Vec<JoinedPoint> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| b.score.total_cmp(&a.score))
    });
    let mut best_score = f64::NEG_INFINITY;
    let mut frontier = vec![];
    for p in sorted {
        if p.score > best_score + 1e-9 {
            best_score = p.score;
            frontier.push(p);
        }
    }
    frontier
}
/// Case-insensitive, whitespace-insensitive substring match used by the chart
/// search box. Matches the display name, model id, or creator/group.
pub(crate) fn pareto_search_matches(query: &str, p: &JoinedPoint) -> bool {
    let q = normalize(query).replace(' ', "");
    if q.is_empty() {
        return true;
    }
    [p.model.as_str(), p.model_id.as_str(), p.creator.as_str()]
        .iter()
        .any(|field| normalize(field).replace(' ', "").contains(&q))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::score_benchmark;
    use crate::testfix::test_quote;
    use crate::types::BenchmarkKind;

    #[test]
    fn log_price_axis_round_trips_prices() {
        for price in [0.0001, 0.01, 0.5, 1.0, 10.0, 250.0] {
            let x = price_to_plot_x(price, true);
            let round_trip = price_from_plot_x(x, true);
            assert!((round_trip - price).abs() < price * 1e-10 + 1e-12);
        }
    }

    #[test]
    fn estimated_task_cost_uses_current_surplus_blend_and_benchmark_volume() {
        let q = test_quote("gpt-5.6-sol", 2.0, true);
        let mut b = score_benchmark(
            "gpt-5.6-sol",
            72.7,
            Some("mini-SWE-agent".into()),
            Some("max".into()),
            BenchmarkKind::Model,
        );
        b.tokens_per_task = Some(2_000_000.0);
        b.token_profile = Some("test profile".into());
        let points = joined_points(
            &[q],
            &[b],
            PriceMetric::Blended,
            CostBasis::EstimatedPerTask,
            BenchmarkMetric::AgenticCoding,
            15.0,
            80.0,
            5.0,
        );
        assert_eq!(points.len(), 1);
        // test_quote => input $2, cache $0.2, output $4 => blend $0.66/M.
        // Two million observed tokens => $1.32 estimated per task.
        assert!((points[0].cost - 1.32).abs() < 1e-12);
    }

    #[test]
    fn search_matches_names_creators_and_ids_leniently() {
        let p = JoinedPoint {
            model_id: "claude-opus-5".into(),
            model: "Claude Opus 5".into(),
            creator: "Anthropic".into(),
            provider: "surplus".into(),
            benchmark_name: "row".into(),
            input: 1.0,
            output: 2.0,
            cache_read: None,
            cost: 1.5,
            tokens_per_task: None,
            token_profile: None,
            score: 80.0,
            live_market: true,
            free_offer_listed: false,
            vision: true,
        };
        assert!(pareto_search_matches("opus", &p));
        assert!(pareto_search_matches("CLAUDE OPUS", &p));
        assert!(pareto_search_matches("claudeopus5", &p));
        assert!(pareto_search_matches("anthropic", &p));
        assert!(!pareto_search_matches("gemini", &p));
        assert!(pareto_search_matches("", &p));
    }
}
