//! Alert engine: threshold and any-change notifications evaluated on the
//! worker thread, semantic frontier alerts, and price-change detection for
//! the recent-moves feed.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::bench::benchmarks_for_source;
use crate::notifications::{NotificationKind, SharedNotifications, notify_alert};
use crate::pareto::{JoinedPoint, joined_points, pareto_frontier};
use crate::types::{
    ANY_MODEL, AlertDirection, AlertMode, AlertRearm, AlertRule, Benchmark, BenchmarkMetric,
    BenchmarkSource, MoveDirection, PriceMetric, PriceSnapshot, Quote, Settings,
};

#[derive(Debug, Clone)]
pub(crate) struct PriceChangeEvent {
    pub(crate) model: String,
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
        (input_weight, cache_read_weight, output_weight),
    );
    if alert.model == ANY_MODEL {
        // Wildcard rules watch the whole population, not one target.
        if joined.is_empty() {
            return "no data".into();
        }
        if alert.mode == AlertMode::CheapestAboveScore {
            return match joined
                .iter()
                .filter(|point| point.score >= alert.score_threshold)
                .min_by(|a, b| a.cost.total_cmp(&b.cost))
            {
                Some(best) => format!(
                    "leader: {} · ${:.4}{}",
                    best.model,
                    best.cost,
                    alert.cost_basis.unit()
                ),
                None => "no model clears score".into(),
            };
        }
        return format!("frontier: {} models", pareto_frontier(&joined).len());
    }
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
        AlertMode::Threshold
        | AlertMode::AnyChange
        | AlertMode::PriceFloor
        | AlertMode::Discount
        | AlertMode::SellerHealth
        | AlertMode::NewModelListed
        | AlertMode::ModelDelisted => String::new(),
    }
}

pub(crate) fn evaluate_alerts(
    snapshot: &PriceSnapshot,
    settings: &Settings,
    state: &mut HashMap<u64, bool>,
    previous_quotes: &HashMap<String, Quote>,
    notifications: &SharedNotifications,
) {
    for alert in &settings.alerts {
        if !alert.enabled {
            state.insert(alert.id, false);
            continue;
        }
        if !alert.mode.worker_evaluated() {
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
            if previous.live_market != quote.live_market {
                continue;
            }
            let Some(best) = qualifying_move(previous, quote, alert, settings) else {
                continue;
            };

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
            let (arrow, direction) = if best.new < best.old {
                ("↓", "down")
            } else {
                ("↑", "up")
            };
            let leg = if best.label == "Blended" {
                String::new()
            } else {
                format!("{} ", best.label.to_lowercase())
            };
            let pct = match best.pct {
                Some(p) => format!(" ({p:+.2}%)"),
                None => " (from $0)".into(),
            };
            let blended_pct = if old_blended.abs() > f64::EPSILON {
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
            notify_alert(
                notifications,
                alert,
                NotificationKind::Price,
                Some(&quote.model),
                &format!(
                    "{arrow} {} {}price {direction}{pct}",
                    quote.display_name, leg
                ),
                &format!(
                    "Blended  ${:.6} → ${:.6}{}\nInput     ${:.6} → ${:.6}\nOutput    ${:.6} → ${:.6}\n{}",
                    old_blended,
                    new_blended,
                    blended_pct,
                    previous.input,
                    quote.input,
                    previous.output,
                    quote.output,
                    source,
                ),
            );
            continue;
        }

        if alert.mode == AlertMode::Discount {
            // Only the live-market overlay publishes discount numbers; on
            // the fallback feed the condition is unknown rather than false.
            let condition = quote
                .discount_pct
                .is_some_and(|pct| pct >= alert.discount_threshold_pct);
            let previous = state.get(&alert.id).copied().unwrap_or(false);
            if should_fire_level(alert, condition, previous, notifications) {
                let pct = quote.discount_pct.unwrap_or_default();
                let was = previous_quotes
                    .get(&alert.model)
                    .and_then(|q| q.discount_pct)
                    .map(|old| format!(" · was {old:.0}%"))
                    .unwrap_or_default();
                notify_alert(
                    notifications,
                    alert,
                    NotificationKind::Discount,
                    Some(&quote.model),
                    &format!("％ {} discount {:.0}%", quote.display_name, pct),
                    &format!(
                        "Live market discount is {:.0}% (alert at {}%).{was}\n{} healthy sellers",
                        pct,
                        alert.discount_threshold_pct,
                        quote.healthy_seller_count.unwrap_or(0),
                    ),
                );
            }
            state.insert(alert.id, condition);
            continue;
        }

        if alert.mode == AlertMode::SellerHealth {
            // Same unknown-on-fallback caveat as discounts: seller counts
            // only exist on live-market quotes.
            let condition = quote
                .healthy_seller_count
                .is_some_and(|count| count <= alert.healthy_seller_floor);
            let previous = state.get(&alert.id).copied().unwrap_or(false);
            if should_fire_level(alert, condition, previous, notifications) {
                let count = quote.healthy_seller_count.unwrap_or_default();
                notify_alert(
                    notifications,
                    alert,
                    NotificationKind::SellerHealth,
                    Some(&quote.model),
                    &format!("⚠ {} seller health", quote.display_name),
                    &format!(
                        "Only {count} healthy sellers left (alert at ≤ {}).\n{} total sellers · live market",
                        alert.healthy_seller_floor,
                        quote.seller_count.unwrap_or(0),
                    ),
                );
            }
            state.insert(alert.id, condition);
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
        if should_fire_level(alert, condition, previous, notifications) {
            let source = if quote.live_market {
                "live market"
            } else {
                "price matrix fallback"
            };
            notify_alert(
                notifications,
                alert,
                NotificationKind::Price,
                Some(&quote.model),
                "ParetoWatch price alert",
                &format!(
                    "{} {} is ${:.4}/1M — {} ${:.4} ({}).",
                    quote.display_name,
                    alert.metric.label(),
                    price,
                    alert.direction.label(),
                    alert.threshold,
                    source
                ),
            );
        }
        state.insert(alert.id, condition);
    }
}

fn should_fire_level(
    alert: &AlertRule,
    condition: bool,
    previous: bool,
    notifications: &SharedNotifications,
) -> bool {
    if !condition {
        return false;
    }
    if !previous {
        return true;
    }
    if alert.rearm != AlertRearm::AfterCooldown {
        return false;
    }
    notifications
        .lock()
        .map(|log| log.ready_for_repeat(alert))
        .unwrap_or(false)
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

/// One price leg's old→new move. `pct` is `None` when the old price was
/// zero and any nonzero price replaced it — an unbounded percentage rise.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LegMove {
    pub(crate) label: &'static str,
    pub(crate) old: f64,
    pub(crate) new: f64,
    pub(crate) pct: Option<f64>,
}

impl LegMove {
    fn magnitude(self) -> f64 {
        self.pct.map(|p| p.abs()).unwrap_or(f64::INFINITY)
    }
}

fn leg_pct(old: f64, new: f64) -> Option<f64> {
    if old.abs() <= f64::EPSILON {
        None
    } else {
        Some((new - old) / old * 100.0)
    }
}

/// The largest move among input, output, and blended that clears the rule's
/// minimum-percentage and direction filter, or `None` when no observed
/// change qualifies. With the default filter (0%, any direction) this is
/// exactly "some price changed" — the historical any-change behavior.
pub(crate) fn qualifying_move(
    previous: &Quote,
    quote: &Quote,
    alert: &AlertRule,
    settings: &Settings,
) -> Option<LegMove> {
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
    [
        LegMove {
            label: "Input",
            old: previous.input,
            new: quote.input,
            pct: leg_pct(previous.input, quote.input),
        },
        LegMove {
            label: "Output",
            old: previous.output,
            new: quote.output,
            pct: leg_pct(previous.output, quote.output),
        },
        LegMove {
            label: "Blended",
            old: old_blended,
            new: new_blended,
            pct: leg_pct(old_blended, new_blended),
        },
    ]
    .into_iter()
    .filter(|leg| prices_differ(leg.old, leg.new))
    .filter(|leg| leg.magnitude() >= alert.min_move_pct)
    .filter(|leg| match alert.move_direction {
        MoveDirection::Any => true,
        MoveDirection::Up => leg.new > leg.old,
        MoveDirection::Down => leg.new < leg.old,
    })
    .max_by(|a, b| a.magnitude().total_cmp(&b.magnitude()))
}

/// A model that vanished from the price feed between two consecutive polls.
#[derive(Debug, Clone)]
pub(crate) struct ListingChange {
    pub(crate) display_name: String,
    pub(crate) last_blended: f64,
}

/// Models present in `previous` but missing from `current`. The base-feed
/// fallback chain (catalog/matrix) can legitimately change which models a
/// degraded poll sees, so delistings are only trustworthy when both polls
/// came from the same base source.
pub(crate) fn detect_delisted(
    previous: &PriceSnapshot,
    current: &PriceSnapshot,
    settings: &Settings,
) -> Vec<ListingChange> {
    if previous.base_source != current.base_source {
        return Vec::new();
    }
    let current_ids: HashSet<&str> = current.quotes.iter().map(|q| q.model.as_str()).collect();
    let mut out = previous
        .quotes
        .iter()
        .filter(|q| !current_ids.contains(q.model.as_str()))
        .map(|q| ListingChange {
            display_name: q.display_name.clone(),
            last_blended: q.price(
                PriceMetric::Blended,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            ),
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    out
}

/// Membership delta of the Pareto frontier between two evaluations of a
/// wildcard frontier rule. Exited models may no longer be joinable at all,
/// so they carry only their remembered id/name pair.
#[derive(Debug, Default)]
pub(crate) struct FrontierDiff {
    pub(crate) entered: Vec<JoinedPoint>,
    pub(crate) exited: Vec<(String, String)>,
}

pub(crate) fn frontier_changes(
    previous: &HashMap<String, String>,
    frontier: &[JoinedPoint],
) -> FrontierDiff {
    let current: HashMap<&str, &str> = frontier
        .iter()
        .map(|p| (p.model_id.as_str(), p.model.as_str()))
        .collect();
    let entered = frontier
        .iter()
        .filter(|p| !previous.contains_key(&p.model_id))
        .cloned()
        .collect();
    let exited = previous
        .iter()
        .filter(|(id, _)| !current.contains_key(id.as_str()))
        .map(|(id, name)| (id.clone(), name.clone()))
        .collect();
    FrontierDiff { entered, exited }
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
            model: new.model.clone(),
            display_name: new.display_name.clone(),
            at: current.fetched_at,
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
    use crate::types::{ComparisonMode, CostBasis, LiquidityFilter};

    fn move_alert(min_move_pct: f64, move_direction: MoveDirection) -> AlertRule {
        AlertRule {
            id: 1,
            model: "model-a".into(),
            mode: AlertMode::AnyChange,
            metric: PriceMetric::Blended,
            cost_basis: CostBasis::PerMillion,
            direction: AlertDirection::BelowOrEqual,
            threshold: 0.0,
            enabled: true,
            benchmark_source: BenchmarkSource::CompositeAgentic,
            benchmark_metric: BenchmarkMetric::AgenticCoding,
            comparison_mode: ComparisonMode::BestAvailableAgent,
            common_scaffold: crate::types::default_common_scaffold(),
            liquidity_filter: LiquidityFilter::Any,
            score_threshold: 50.0,
            min_move_pct,
            move_direction,
            discount_threshold_pct: crate::types::default_discount_threshold(),
            healthy_seller_floor: 0,
            cooldown_minutes: 0,
            rearm: AlertRearm::ConditionReset,
            sound: crate::types::AlertSound::Chime,
        }
    }

    fn moved_quote(input: f64) -> Quote {
        test_quote("model-a", input, true)
    }

    fn snapshot(quotes: Vec<Quote>, base_source: &str) -> PriceSnapshot {
        PriceSnapshot {
            quotes,
            fetched_at: Utc::now(),
            comparison_updated_at: None,
            market_overlay_count: 0,
            market_error: None,
            base_source: base_source.into(),
        }
    }

    fn frontier_point(id: &str, name: &str, cost: f64, score: f64) -> JoinedPoint {
        JoinedPoint {
            model_id: id.into(),
            model: name.into(),
            creator: "test".into(),
            provider: "test".into(),
            benchmark_name: "row".into(),
            input: 1.0,
            output: 2.0,
            cache_read: None,
            cost,
            tokens_per_task: None,
            token_profile: None,
            score,
            live_market: true,
            free_offer_listed: false,
            vision: false,
        }
    }

    #[test]
    fn any_change_detection_ignores_source_switches() {
        let settings = Settings::default();
        let old = snapshot(vec![test_quote("model-a", 1.0, true)], "test");
        let changed = snapshot(vec![test_quote("model-a", 0.9, true)], "test");
        let changes = detect_price_changes(&old, &changed, &settings);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].model, "model-a");
        let fallback = snapshot(vec![test_quote("model-a", 0.8, false)], "test");
        assert!(detect_price_changes(&changed, &fallback, &settings).is_empty());
    }

    #[test]
    fn move_filter_respects_threshold_and_direction() {
        let settings = Settings::default();
        // test_quote: input x, output 2x, cache 0.1x. At input 2.0 the blend is
        // (2·15 + 0.2·80 + 4·5)/100 = 0.66. Move only the input leg so each
        // leg's percentage is independent of the others.
        let previous = moved_quote(2.0);
        let mut risen = moved_quote(2.0);
        risen.input = 2.2; // input +10%, blended +4.55%, output flat

        let best = qualifying_move(
            &previous,
            &risen,
            &move_alert(0.0, MoveDirection::Any),
            &settings,
        )
        .expect("default filter fires on any change");
        assert_eq!(best.label, "Input");
        assert!((best.pct.unwrap() - 10.0).abs() < 1e-9);

        // Input moved 10% but no leg clears 50%.
        assert!(
            qualifying_move(
                &previous,
                &risen,
                &move_alert(50.0, MoveDirection::Any),
                &settings
            )
            .is_none()
        );
        // The rise does not satisfy a drops-only filter.
        assert!(
            qualifying_move(
                &previous,
                &risen,
                &move_alert(5.0, MoveDirection::Down),
                &settings
            )
            .is_none()
        );
        // 5% minimum: input (+10%) qualifies even though blended (+4.55%) does not.
        let best = qualifying_move(
            &previous,
            &risen,
            &move_alert(5.0, MoveDirection::Any),
            &settings,
        )
        .expect("input leg clears the minimum");
        assert_eq!(best.label, "Input");

        let mut dropped = moved_quote(2.0);
        dropped.input = 1.0; // input −50%, blended −22.7%, output flat
        assert!(
            qualifying_move(
                &previous,
                &dropped,
                &move_alert(40.0, MoveDirection::Down),
                &settings
            )
            .is_some()
        );
        assert!(
            qualifying_move(
                &previous,
                &dropped,
                &move_alert(40.0, MoveDirection::Up),
                &settings
            )
            .is_none()
        );
    }

    #[test]
    fn move_filter_treats_zero_prices_as_unbounded_moves() {
        let settings = Settings::default();
        let free = moved_quote(0.0);
        let priced = moved_quote(2.0);
        // $0 → priced is an infinite percentage rise: any minimum clears.
        let best = qualifying_move(
            &free,
            &priced,
            &move_alert(99.9, MoveDirection::Up),
            &settings,
        )
        .expect("rise from zero always qualifies");
        assert!(best.pct.is_none());
        // Priced → $0 is a −100% drop.
        assert!(
            qualifying_move(
                &priced,
                &free,
                &move_alert(99.0, MoveDirection::Down),
                &settings
            )
            .is_some()
        );
        assert!(
            qualifying_move(
                &priced,
                &free,
                &move_alert(0.0, MoveDirection::Up),
                &settings
            )
            .is_none()
        );
    }

    #[test]
    fn delist_detection_requires_same_base_source() {
        let settings = Settings::default();
        let previous = snapshot(
            vec![
                test_quote("model-a", 1.0, true),
                test_quote("model-b", 2.0, true),
            ],
            "catalog+matrix",
        );
        let shrunk = snapshot(vec![test_quote("model-a", 1.0, true)], "catalog+matrix");
        let delisted = detect_delisted(&previous, &shrunk, &settings);
        assert_eq!(delisted.len(), 1);
        assert_eq!(delisted[0].display_name, "model-b");

        // A base-feed fallback switch can shrink the visible set without any
        // real delisting; that poll must not fire.
        let degraded = snapshot(vec![test_quote("model-a", 1.0, true)], "text fallback");
        assert!(detect_delisted(&previous, &degraded, &settings).is_empty());
    }

    #[test]
    fn frontier_diff_reports_entries_and_exits() {
        let previous: HashMap<String, String> = [
            ("model-a".to_string(), "Model A".to_string()),
            ("model-b".to_string(), "Model B".to_string()),
        ]
        .into_iter()
        .collect();
        let frontier = vec![
            frontier_point("model-b", "Model B", 1.0, 80.0),
            frontier_point("model-c", "Model C", 2.0, 90.0),
        ];
        let diff = frontier_changes(&previous, &frontier);
        assert_eq!(diff.entered.len(), 1);
        assert_eq!(diff.entered[0].model_id, "model-c");
        assert_eq!(
            diff.exited,
            vec![("model-a".to_string(), "Model A".to_string())]
        );
    }
}
