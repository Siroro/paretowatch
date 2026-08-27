//! Alerts tab: form for new alert rules, the live rule table, and the
//! recent price-moves feed.

use eframe::egui;

use crate::alerts::next_alert_id;
use crate::types::{
    ANY_MODEL, AlertDirection, AlertMode, AlertRule, BenchmarkMetric, BenchmarkSource,
    ComparisonMode, CostBasis, LiquidityFilter, MoveDirection, PriceMetric,
};

use super::ParetoWatchApp;

impl ParetoWatchApp {
    pub(super) fn alerts_tab(&mut self, ui: &mut egui::Ui) {
        let scaffold_options = self.cached_scaffolds();
        // Computed up front (with per-alert caching of the heavy semantic
        // statuses) so the table below renders without re-running the
        // benchmark join every frame.
        let statuses = self.cached_alert_statuses();

        // Deferred mutations: performed after the quote borrow below ends.
        let mut add_alert = false;
        let mut remove_id = None;
        let mut changed = false;

        {
            // Borrowed, not cloned: this used to deep-copy every quote (each
            // with its provider list) on every repaint of this tab.
            let quotes: &[crate::types::Quote] = self
                .price_snapshot
                .as_ref()
                .map(|s| s.quotes.as_slice())
                .unwrap_or(&[]);

            ui.group(|ui| {
                ui.strong("New alert");
                // The wildcard sentinel only makes sense on the modes that
                // diff a population (frontier membership, cheapest lead);
                // switching modes must not leave it selected where no rule
                // would consume it.
                let wildcard_allowed = self.alert_mode.benchmark_dependent();
                if self.alert_model == ANY_MODEL && !wildcard_allowed {
                    self.alert_model = quotes.first().map(|q| q.model.clone()).unwrap_or_default();
                }
                ui.horizontal_wrapped(|ui| {
                    if self.alert_mode.feed_wide() {
                        ui.label("Feed-wide — watches every model in the price feed");
                    } else {
                        egui::ComboBox::from_id_salt("alert_model")
                            .width(260.0)
                            .selected_text(if self.alert_model == ANY_MODEL {
                                "Any model"
                            } else {
                                quotes
                                    .iter()
                                    .find(|q| q.model == self.alert_model)
                                    .map(|q| q.display_name.as_str())
                                    .unwrap_or("Select model")
                            })
                            .show_ui(ui, |ui| {
                                if wildcard_allowed {
                                    ui.selectable_value(
                                        &mut self.alert_model,
                                        ANY_MODEL.into(),
                                        "Any model",
                                    );
                                }
                                for q in quotes {
                                    ui.selectable_value(
                                        &mut self.alert_model,
                                        q.model.clone(),
                                        &q.display_name,
                                    );
                                }
                            });
                    }

                    egui::ComboBox::from_id_salt("alert_mode")
                        .selected_text(self.alert_mode.label())
                        .show_ui(ui, |ui| {
                            for mode in [
                                AlertMode::Threshold,
                                AlertMode::AnyChange,
                                AlertMode::PriceFloor,
                                AlertMode::Discount,
                                AlertMode::SellerHealth,
                                AlertMode::EntersFrontier,
                                AlertMode::LeavesFrontier,
                                AlertMode::CheapestAboveScore,
                                AlertMode::NewModelListed,
                                AlertMode::ModelDelisted,
                            ] {
                                ui.selectable_value(&mut self.alert_mode, mode, mode.label());
                            }
                        });

                    if self.alert_mode == AlertMode::Threshold {
                        egui::ComboBox::from_id_salt("alert_metric")
                            .selected_text(self.alert_metric.label())
                            .show_ui(ui, |ui| {
                                for metric in [
                                    PriceMetric::Blended,
                                    PriceMetric::Input,
                                    PriceMetric::Output,
                                ] {
                                    ui.selectable_value(
                                        &mut self.alert_metric,
                                        metric,
                                        metric.label(),
                                    );
                                }
                            });
                        egui::ComboBox::from_id_salt("alert_direction")
                            .selected_text(self.alert_direction.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.alert_direction,
                                    AlertDirection::BelowOrEqual,
                                    "at or below",
                                );
                                ui.selectable_value(
                                    &mut self.alert_direction,
                                    AlertDirection::AboveOrEqual,
                                    "at or above",
                                );
                            });
                        ui.label("$");
                        ui.add(
                            egui::DragValue::new(&mut self.alert_threshold)
                                .speed(0.05)
                                .range(0.0..=100_000.0),
                        );
                        ui.label("/ 1M");
                    } else if self.alert_mode == AlertMode::AnyChange {
                        ui.label("moves");
                        egui::ComboBox::from_id_salt("alert_move_direction")
                            .selected_text(self.alert_move_direction.label())
                            .show_ui(ui, |ui| {
                                for direction in
                                    [MoveDirection::Any, MoveDirection::Up, MoveDirection::Down]
                                {
                                    ui.selectable_value(
                                        &mut self.alert_move_direction,
                                        direction,
                                        direction.label(),
                                    );
                                }
                            });
                        ui.label("by at least");
                        ui.add(
                            egui::DragValue::new(&mut self.alert_min_move_pct)
                                .speed(0.25)
                                .range(0.0..=100.0)
                                .suffix("%"),
                        );
                        ui.label("(input, output, or blended)");
                    } else if self.alert_mode == AlertMode::PriceFloor {
                        ui.label("Fires whenever the blended price sets a new all-time low (per the history log).");
                    } else if self.alert_mode == AlertMode::Discount {
                        ui.label("≥");
                        ui.add(
                            egui::DragValue::new(&mut self.alert_discount_pct)
                                .speed(1.0)
                                .range(0.0..=100.0)
                                .suffix("%"),
                        );
                        ui.label("live-market discount");
                    } else if self.alert_mode == AlertMode::SellerHealth {
                        ui.label("healthy sellers ≤");
                        ui.add(
                            egui::DragValue::new(&mut self.alert_healthy_floor)
                                .speed(1.0)
                                .range(0..=100u64),
                        );
                    }
                });

                if self.alert_mode.benchmark_dependent() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Benchmark:");
                        egui::ComboBox::from_id_salt("alert_benchmark_source")
                            .selected_text(self.benchmark_source.label())
                            .show_ui(ui, |ui| {
                                for source in [
                                    BenchmarkSource::CompositeAgentic,
                                    BenchmarkSource::CompositeDeployment,
                                ]
                                .into_iter()
                                .chain(BenchmarkSource::display_sources())
                                {
                                    ui.selectable_value(
                                        &mut self.benchmark_source,
                                        source,
                                        source.label(),
                                    );
                                }
                            });
                        if self.benchmark_source == BenchmarkSource::LiveBench {
                            egui::ComboBox::from_id_salt("alert_benchmark_metric")
                                .selected_text(self.benchmark_metric.label())
                                .show_ui(ui, |ui| {
                                    for metric in [
                                        BenchmarkMetric::Overall,
                                        BenchmarkMetric::Coding,
                                        BenchmarkMetric::AgenticCoding,
                                    ] {
                                        ui.selectable_value(
                                            &mut self.benchmark_metric,
                                            metric,
                                            metric.label(),
                                        );
                                    }
                                });
                        }
                        ui.separator();
                        egui::ComboBox::from_id_salt("alert_comparison_mode")
                            .selected_text(self.comparison_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in [
                                    ComparisonMode::ModelCapability,
                                    ComparisonMode::BestAvailableAgent,
                                    ComparisonMode::CommonScaffold,
                                ] {
                                    ui.selectable_value(
                                        &mut self.comparison_mode,
                                        mode,
                                        mode.label(),
                                    );
                                }
                            });
                        if self.comparison_mode == ComparisonMode::CommonScaffold {
                            egui::ComboBox::from_id_salt("alert_common_scaffold")
                                .width(180.0)
                                .selected_text(&self.common_scaffold)
                                .show_ui(ui, |ui| {
                                    for scaffold in &scaffold_options {
                                        ui.selectable_value(
                                            &mut self.common_scaffold,
                                            scaffold.clone(),
                                            scaffold,
                                        );
                                    }
                                });
                        }
                        ui.separator();
                        egui::ComboBox::from_id_salt("alert_liquidity_filter")
                            .selected_text(self.liquidity_filter.label())
                            .show_ui(ui, |ui| {
                                for filter in [
                                    LiquidityFilter::Any,
                                    LiquidityFilter::Trusted,
                                    LiquidityFilter::Healthy3,
                                    LiquidityFilter::Healthy10,
                                ] {
                                    ui.selectable_value(
                                        &mut self.liquidity_filter,
                                        filter,
                                        filter.label(),
                                    );
                                }
                            });
                        ui.separator();
                        ui.label("Price axis:");
                        egui::ComboBox::from_id_salt("semantic_alert_price_metric")
                            .selected_text(self.alert_metric.label())
                            .show_ui(ui, |ui| {
                                for metric in [
                                    PriceMetric::Blended,
                                    PriceMetric::Input,
                                    PriceMetric::Output,
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut self.alert_metric,
                                            metric,
                                            metric.label(),
                                        )
                                        .changed()
                                        && self.alert_metric != PriceMetric::Blended
                                    {
                                        self.alert_cost_basis = CostBasis::PerMillion;
                                    }
                                }
                            });
                        if self.alert_metric == PriceMetric::Blended {
                            ui.separator();
                            egui::ComboBox::from_id_salt("semantic_alert_cost_basis")
                                .selected_text(self.alert_cost_basis.label())
                                .show_ui(ui, |ui| {
                                    for basis in
                                        [CostBasis::PerMillion, CostBasis::EstimatedPerTask]
                                    {
                                        ui.selectable_value(
                                            &mut self.alert_cost_basis,
                                            basis,
                                            basis.label(),
                                        );
                                    }
                                });
                        }
                        if self.alert_mode == AlertMode::CheapestAboveScore {
                            ui.separator();
                            ui.label("Minimum score");
                            ui.add(
                                egui::DragValue::new(&mut self.alert_score_threshold)
                                    .speed(1.0)
                                    .range(0.0..=100.0),
                            );
                        }
                    });
                }

                if ui
                    .add_enabled(
                        !self.alert_model.is_empty() || self.alert_mode.feed_wide(),
                        egui::Button::new("Add alert"),
                    )
                    .clicked()
                {
                    add_alert = true;
                }
            });

            ui.add_space(10.0);
            // Copy these before mutably iterating `settings.alerts`; this keeps the
            // UI borrow simple and makes the alert row rendering compiler-friendly.
            let input_weight = self.settings.input_weight;
            let cache_read_weight = self.settings.cache_read_weight;
            let output_weight = self.settings.output_weight;
            egui::Grid::new("alerts_grid").striped(true).show(ui, |ui| {
                ui.strong("On");
                ui.strong("Model");
                ui.strong("Rule");
                ui.strong("Current");
                ui.end_row();

                for (index, alert) in self.settings.alerts.iter_mut().enumerate() {
                    if ui.checkbox(&mut alert.enabled, "").changed() {
                        changed = true;
                    }
                    let name = if alert.mode.feed_wide() {
                        "— feed —"
                    } else if alert.model == ANY_MODEL {
                        "Any model"
                    } else {
                        quotes
                            .iter()
                            .find(|q| q.model == alert.model)
                            .map(|q| q.display_name.as_str())
                            .unwrap_or(&alert.model)
                    };
                    ui.label(name);
                    let rule_text = match alert.mode {
                        AlertMode::Threshold => format!(
                            "{} {} ${:.4}",
                            alert.metric.label(),
                            alert.direction.label(),
                            alert.threshold
                        ),
                        AlertMode::AnyChange => {
                            if alert.min_move_pct > 0.0
                                || alert.move_direction != MoveDirection::Any
                            {
                                format!(
                                    "Move ≥ {:.2}% · {}",
                                    alert.min_move_pct,
                                    alert.move_direction.label()
                                )
                            } else {
                                "Any price change".into()
                            }
                        }
                        AlertMode::PriceFloor => "All-time blended low".into(),
                        AlertMode::Discount => {
                            format!("Discount ≥ {:.0}%", alert.discount_threshold_pct)
                        }
                        AlertMode::SellerHealth => {
                            format!("Healthy sellers ≤ {}", alert.healthy_seller_floor)
                        }
                        AlertMode::EntersFrontier => format!(
                            "Enters frontier · {} · {} · {}",
                            alert.benchmark_source.short_label(),
                            alert.comparison_mode.label(),
                            alert.cost_basis.label()
                        ),
                        AlertMode::LeavesFrontier => format!(
                            "Leaves frontier · {} · {} · {}",
                            alert.benchmark_source.short_label(),
                            alert.comparison_mode.label(),
                            alert.cost_basis.label()
                        ),
                        AlertMode::CheapestAboveScore => {
                            if alert.model == ANY_MODEL {
                                format!(
                                    "Lead change ≥ {:.1} · {} · {}",
                                    alert.score_threshold,
                                    alert.benchmark_source.short_label(),
                                    alert.cost_basis.label()
                                )
                            } else {
                                format!(
                                    "Cheapest ≥ {:.1} · {} · {}",
                                    alert.score_threshold,
                                    alert.benchmark_source.short_label(),
                                    alert.cost_basis.label()
                                )
                            }
                        }
                        AlertMode::NewModelListed => "New model appears in the feed".into(),
                        AlertMode::ModelDelisted => "Model disappears from the feed".into(),
                    };
                    ui.label(rule_text);

                    // Precomputed above: semantic statuses come from the
                    // per-alert cache; price statuses are a live quote lookup.
                    let current_text = if alert.mode.benchmark_dependent() {
                        statuses.get(index).cloned().unwrap_or_default()
                    } else if alert.mode == AlertMode::AnyChange {
                        quotes
                            .iter()
                            .find(|q| q.model == alert.model)
                            .map(|q| {
                                format!(
                                    "${:.4} blend",
                                    q.price(
                                        PriceMetric::Blended,
                                        input_weight,
                                        cache_read_weight,
                                        output_weight,
                                    )
                                )
                            })
                            .unwrap_or_else(|| "—".into())
                    } else {
                        statuses.get(index).cloned().unwrap_or_default()
                    };
                    ui.label(current_text);
                    if ui.small_button("Delete").clicked() {
                        remove_id = Some(alert.id);
                    }
                    ui.end_row();
                }
            });
        }

        if add_alert {
            self.settings.alerts.push(AlertRule {
                id: next_alert_id(&self.settings.alerts),
                model: if self.alert_mode.feed_wide() {
                    ANY_MODEL.into()
                } else {
                    self.alert_model.clone()
                },
                mode: self.alert_mode,
                metric: self.alert_metric,
                cost_basis: if self.alert_mode.benchmark_dependent() {
                    self.alert_cost_basis
                } else {
                    CostBasis::PerMillion
                },
                direction: self.alert_direction,
                threshold: self.alert_threshold,
                enabled: true,
                benchmark_source: self.benchmark_source,
                benchmark_metric: self.benchmark_metric,
                comparison_mode: self.comparison_mode,
                common_scaffold: self.common_scaffold.clone(),
                liquidity_filter: self.liquidity_filter,
                score_threshold: self.alert_score_threshold,
                min_move_pct: self.alert_min_move_pct,
                move_direction: self.alert_move_direction,
                discount_threshold_pct: self.alert_discount_pct,
                healthy_seller_floor: self.alert_healthy_floor,
            });
            changed = true;
        }
        if let Some(id) = remove_id {
            self.settings.alerts.retain(|a| a.id != id);
            self.semantic_alert_state.remove(&id);
            self.semantic_frontier_state.remove(&id);
            self.semantic_leader_state.remove(&id);
            changed = true;
        }
        if changed {
            self.save_and_push_settings();
        }

        ui.add_space(8.0);
        ui.label("Threshold, discount, seller-health, and benchmark alerts are edge-triggered and re-arm when their condition becomes false. Move alerts fire per observed poll change once any leg (input, output, blended) clears the minimum; live-market/fallback source switches are ignored. All-time-low alerts fire whenever the blended price undercuts the history log's minimum (discounts and seller counts come from the live market only). Feed-wide alerts fire when a model appears in or disappears from the price feed (new listings are remembered across restarts via the history log; the first poll after startup only seeds the baseline). \"Any model\" frontier alerts diff the whole frontier membership; \"Any model\" cheapest alerts fire when the lead changes hands.");

        ui.add_space(14.0);
        ui.heading("Recent price moves");
        if self.recent_changes.is_empty() {
            ui.label("No price changes observed since the app started.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for change in self.recent_changes.iter().take(40) {
                        let delta = change.delta();
                        let (arrow, color) = if delta < 0.0 {
                            ("↓", egui::Color32::from_rgb(54, 179, 126))
                        } else if delta > 0.0 {
                            ("↑", egui::Color32::from_rgb(224, 86, 86))
                        } else {
                            ("↔", ui.visuals().text_color())
                        };
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(
                                    color,
                                    egui::RichText::new(arrow).size(22.0).strong(),
                                );
                                ui.strong(&change.display_name);
                                if let Some(pct) = change.percent_delta() {
                                    ui.colored_label(color, format!("{pct:+.2}% blended"));
                                } else {
                                    ui.colored_label(color, "price changed");
                                }
                                ui.separator();
                                ui.small(change.at.format("%H:%M:%S UTC").to_string());
                                ui.separator();
                                ui.small(change.source);
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "Blended ${:.6} → ${:.6}",
                                    change.old_blended, change.new_blended
                                ));
                                ui.separator();
                                ui.label(format!(
                                    "Input ${:.6} → ${:.6}",
                                    change.old_input, change.new_input
                                ));
                                ui.separator();
                                ui.label(format!(
                                    "Output ${:.6} → ${:.6}",
                                    change.old_output, change.new_output
                                ));
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.heading("Notification history");
            if ui.small_button("Clear").clicked()
                && let Ok(mut log) = self.notifications.lock()
            {
                log.clear();
            }
        });
        let records = self
            .notifications
            .lock()
            .map(|log| log.records().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if records.is_empty() {
            ui.label("No alerts have fired this session.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for record in &records {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(&record.summary);
                                ui.separator();
                                ui.small(record.at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                            });
                            if !record.body.is_empty() {
                                ui.small(&record.body);
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
        }
    }
}
