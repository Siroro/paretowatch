//! Alert engine: threshold and any-change notifications evaluated on the
//! worker thread, semantic frontier alerts, and price-change detection for
//! the recent-moves feed.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use notify_rust::Notification;

use crate::bench::benchmarks_for_source;
use crate::pareto::{joined_points, pareto_frontier};
use crate::types::{
    AlertDirection, AlertMode, AlertRule, Benchmark, BenchmarkMetric, BenchmarkSource, PriceMetric,
    PriceSnapshot, Quote, Settings,
};

#[derive(Debug, Clone)]
pub(crate) struct PriceChangeEvent {
    pub(crate) display_name: String,
    pub(crate) at: DateTime<Utc>,
    pub(crate) old_blended: f64,
    pub(crate) new_blended: f64,
    pub(crate) old_input: f64,
    pub(crate) new_input: f64,
    pub(crate) old_output: f64,
    pub(crate) new_output: f64,
    pub(crate) source: &'static str,
}

impl PriceChangeEvent {
    pub(crate) fn delta(&self) -> f64 {
        self.new_blended - self.old_blended
    }

    pub(crate) fn percent_delta(&self) -> Option<f64> {
        if self.old_blended.abs() <= f64::EPSILON {
            None
        } else {
            Some((self.new_blended - self.old_blended) / self.old_blended * 100.0)
        }
    }
}
pub(crate) fn semantic_alert_status(
    alert: &AlertRule,
    quotes: &[Quote],
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
) -> String {
    let benchmarks = benchmarks_for_source(
        alert.benchmark_source,
        sets,
        alert.comparison_mode,
        &alert.common_scaffold,
    );
    let benchmark_metric = if alert.benchmark_source == BenchmarkSource::LiveBench {
        alert.benchmark_metric
    } else {
        BenchmarkMetric::AgenticCoding
    };
    let filtered = quotes
        .iter()
        .filter_map(|quote| {
            alert
                .liquidity_filter
                .apply(quote, input_weight, cache_read_weight, output_weight)
        })
        .collect::<Vec<_>>();
    let joined = joined_points(
        &filtered,
        &benchmarks,
        alert.metric,
        alert.cost_basis,
        benchmark_metric,
        input_weight,
        cache_read_weight,
        output_weight,
    );
    let Some(target) = joined.iter().find(|point| point.model_id == alert.model) else {
        return "not matched".into();
    };
    match alert.mode {
        AlertMode::EntersFrontier | AlertMode::LeavesFrontier => {
            let on_frontier = pareto_frontier(&joined)
                .iter()
                .any(|point| point.model_id == alert.model);
            if on_frontier {
                format!("on frontier · {:.1}", target.score)
            } else {
                format!("off frontier · {:.1}", target.score)
            }
        }
        AlertMode::CheapestAboveScore => {
            let cheapest = joined
                .iter()
                .filter(|point| point.score >= alert.score_threshold)
                .min_by(|a, b| a.cost.total_cmp(&b.cost));
            match cheapest {
                Some(best) if best.model_id == alert.model => {
                    format!("cheapest · ${:.4}{}", best.cost, alert.cost_basis.unit())
                }
                Some(best) => format!(
                    "leader: {} · ${:.4}{}",
                    best.model,
                    best.cost,
                    alert.cost_basis.unit()
                ),
                None => "no model clears score".into(),
            }
        }
        AlertMode::Threshold | AlertMode::AnyChange => String::new(),
    }
}

pub(crate) fn evaluate_alerts(
    snapshot: &PriceSnapshot,
    settings: &Settings,
    state: &mut HashMap<u64, bool>,
    previous_quotes: &HashMap<String, Quote>,
) {
    for alert in &settings.alerts {
        if !alert.enabled {
            state.insert(alert.id, false);
            continue;
        }
        if alert.mode.benchmark_dependent() {
            continue;
        }
        let Some(quote) = snapshot.quotes.iter().find(|q| q.model == alert.model) else {
            continue;
        };

        if alert.mode == AlertMode::AnyChange {
            state.insert(alert.id, false);
            let Some(previous) = previous_quotes.get(&alert.model) else {
                continue;
            };
            // A live-feed outage/recovery can swap a model between the marketplace
            // quote and the slower fallback feed. Do not misreport that source
            // transition as a market price move.
            if previous.live_market != quote.live_market
                || !quote_has_price_change(previous, quote, settings)
            {
                continue;
            }

            let old_blended = previous.price(
                PriceMetric::Blended,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            );
            let new_blended = quote.price(
                PriceMetric::Blended,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            );
            let (arrow, direction) = if new_blended < old_blended {
                ("↓", "down")
            } else if new_blended > old_blended {
                ("↑", "up")
            } else {
                ("↔", "components changed")
            };
            let pct = if old_blended.abs() > f64::EPSILON {
                format!(
                    " ({:+.2}%)",
                    (new_blended - old_blended) / old_blended * 100.0
                )
            } else {
                String::new()
            };
            let source = if quote.live_market {
                "live market"
            } else {
                "price matrix fallback"
            };
            let _ = Notification::new()
                .summary(&format!("{arrow} {} price {direction}", quote.display_name))
                .body(&format!(
                    "Blended  ${:.6} → ${:.6}{}\nInput     ${:.6} → ${:.6}\nOutput    ${:.6} → ${:.6}\n{}",
                    old_blended,
                    new_blended,
                    pct,
                    previous.input,
                    quote.input,
                    previous.output,
                    quote.output,
                    source,
                ))
                .show();
            continue;
        }

        let price = quote.price(
            alert.metric,
            settings.input_weight,
            settings.cache_read_weight,
            settings.output_weight,
        );
        let condition = match alert.direction {
            AlertDirection::BelowOrEqual => price <= alert.threshold,
            AlertDirection::AboveOrEqual => price >= alert.threshold,
        };
        let previous = state.get(&alert.id).copied().unwrap_or(false);
        if condition && !previous {
            let source = if quote.live_market {
                "live market"
            } else {
                "price matrix fallback"
            };
            let _ = Notification::new()
                .summary("ParetoWatch price alert")
                .body(&format!(
                    "{} {} is ${:.4}/1M — {} ${:.4} ({}).",
                    quote.display_name,
                    alert.metric.label(),
                    price,
                    alert.direction.label(),
                    alert.threshold,
                    source
                ))
                .show();
        }
        state.insert(alert.id, condition);
    }
}

pub(crate) fn prices_differ(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() > 1e-10 * scale
}

fn quote_has_price_change(old: &Quote, new: &Quote, settings: &Settings) -> bool {
    let old_blended = old.price(
        PriceMetric::Blended,
        settings.input_weight,
        settings.cache_read_weight,
        settings.output_weight,
    );
    let new_blended = new.price(
        PriceMetric::Blended,
        settings.input_weight,
        settings.cache_read_weight,
        settings.output_weight,
    );
    prices_differ(old.input, new.input)
        || prices_differ(old.output, new.output)
        || prices_differ(old_blended, new_blended)
}

pub(crate) fn detect_price_changes(
    previous: &PriceSnapshot,
    current: &PriceSnapshot,
    settings: &Settings,
) -> Vec<PriceChangeEvent> {
    let old_by_model = previous
        .quotes
        .iter()
        .map(|q| (q.model.as_str(), q))
        .collect::<HashMap<_, _>>();
    let mut changes = Vec::new();
    for new in &current.quotes {
        let Some(old) = old_by_model.get(new.model.as_str()).copied() else {
            continue;
        };
        if old.live_market != new.live_market || !quote_has_price_change(old, new, settings) {
            continue;
        }
        changes.push(PriceChangeEvent {
            display_name: new.display_name.clone(),
            at: current.fetched_at.clone(),
            old_blended: old.price(
                PriceMetric::Blended,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            ),
            new_blended: new.price(
                PriceMetric::Blended,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            ),
            old_input: old.input,
            new_input: new.input,
            old_output: old.output,
            new_output: new.output,
            source: if new.live_market {
                "live market"
            } else {
                "comparison/catalog"
            },
        });
    }
    changes.sort_by(|a, b| b.delta().abs().total_cmp(&a.delta().abs()));
    changes
}
pub(crate) fn next_alert_id(alerts: &[AlertRule]) -> u64 {
    alerts.iter().map(|a| a.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;

    #[test]
    fn any_change_detection_ignores_source_switches() {
        let settings = Settings::default();
        let old = PriceSnapshot {
            quotes: vec![test_quote("model-a", 1.0, true)],
            fetched_at: Utc::now(),
            comparison_updated_at: None,
            market_overlay_count: 1,
            market_error: None,
            base_source: "test".into(),
        };
        let changed = PriceSnapshot {
            quotes: vec![test_quote("model-a", 0.9, true)],
            fetched_at: Utc::now(),
            comparison_updated_at: None,
            market_overlay_count: 1,
            market_error: None,
            base_source: "test".into(),
        };
        assert_eq!(detect_price_changes(&old, &changed, &settings).len(), 1);
        let fallback = PriceSnapshot {
            quotes: vec![test_quote("model-a", 0.8, false)],
            fetched_at: Utc::now(),
            comparison_updated_at: None,
            market_overlay_count: 0,
            market_error: Some("test".into()),
            base_source: "test".into(),
        };
        assert!(detect_price_changes(&changed, &fallback, &settings).is_empty());
    }
}
