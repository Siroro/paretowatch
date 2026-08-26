//! Alerts tab: form for new alert rules, the live rule table, and the
//! recent price-moves feed.

use eframe::egui;

use crate::alerts::{next_alert_id, semantic_alert_status};
use crate::types::{
    AlertDirection, AlertMode, AlertRule, BenchmarkMetric, BenchmarkSource, ComparisonMode,
    CostBasis, LiquidityFilter, PriceMetric,
};

use super::ParetoWatchApp;

impl ParetoWatchApp {
    pub(super) fn alerts_tab(&mut self, ui: &mut egui::Ui) {
        let quotes = self
            .price_snapshot
            .as_ref()
            .map(|s| s.quotes.clone())
            .unwrap_or_default();
        let scaffold_options = self.cached_scaffolds();

        ui.group(|ui| {
            ui.strong("New alert");
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("alert_model")
                    .width(260.0)
                    .selected_text(
                        quotes
                            .iter()
                            .find(|q| q.model == self.alert_model)
                            .map(|q| q.display_name.as_str())
                            .unwrap_or("Select model"),
                    )
                    .show_ui(ui, |ui| {
                        for q in &quotes {
                            ui.selectable_value(
                                &mut self.alert_model,
                                q.model.clone(),
                                &q.display_name,
                            );
                        }
                    });

                egui::ComboBox::from_id_salt("alert_mode")
                    .selected_text(self.alert_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in [
                            AlertMode::Threshold,
                            AlertMode::AnyChange,
                            AlertMode::EntersFrontier,
                            AlertMode::LeavesFrontier,
                            AlertMode::CheapestAboveScore,
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
                                ui.selectable_value(&mut self.alert_metric, metric, metric.label());
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
                    ui.label("Notify whenever input, output, or blended price changes.");
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
                                ui.selectable_value(&mut self.comparison_mode, mode, mode.label());
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
                                for basis in [CostBasis::PerMillion, CostBasis::EstimatedPerTask] {
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
                .add_enabled(!self.alert_model.is_empty(), egui::Button::new("Add alert"))
                .clicked()
            {
                self.settings.alerts.push(AlertRule {
                    id: next_alert_id(&self.settings.alerts),
                    model: self.alert_model.clone(),
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
                });
                self.save_and_push_settings();
            }
        });

        ui.add_space(10.0);
        let mut remove_id = None;
        let mut changed = false;
        // Copy these before mutably iterating `settings.alerts`; this keeps the
        // UI borrow simple and makes the alert row rendering compiler-friendly.
        let input_weight = self.settings.input_weight;
        let cache_read_weight = self.settings.cache_read_weight;
        let output_weight = self.settings.output_weight;
        let benchmark_sets = &self.benchmark_sets;
        egui::Grid::new("alerts_grid").striped(true).show(ui, |ui| {
            ui.strong("On");
            ui.strong("Model");
            ui.strong("Rule");
            ui.strong("Current");
            ui.end_row();

            for alert in &mut self.settings.alerts {
                if ui.checkbox(&mut alert.enabled, "").changed() {
                    changed = true;
                }
                let name = quotes
                    .iter()
                    .find(|q| q.model == alert.model)
                    .map(|q| q.display_name.as_str())
                    .unwrap_or(&alert.model);
                ui.label(name);
                let rule_text = match alert.mode {
                    AlertMode::Threshold => format!(
                        "{} {} ${:.4}",
                        alert.metric.label(),
                        alert.direction.label(),
                        alert.threshold
                    ),
                    AlertMode::AnyChange => "Any price change".into(),
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
                    AlertMode::CheapestAboveScore => format!(
                        "Cheapest ≥ {:.1} · {} · {}",
                        alert.score_threshold,
                        alert.benchmark_source.short_label(),
                        alert.cost_basis.label()
                    ),
                };
                ui.label(rule_text);

                let current_text = if alert.mode.benchmark_dependent() {
                    semantic_alert_status(
                        alert,
                        &quotes,
                        benchmark_sets,
                        input_weight,
                        cache_read_weight,
                        output_weight,
                    )
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
                    quotes
                        .iter()
                        .find(|q| q.model == alert.model)
                        .map(|q| {
                            format!(
                                "${:.4}",
                                q.price(
                                    alert.metric,
                                    input_weight,
                                    cache_read_weight,
                                    output_weight,
                                )
                            )
                        })
                        .unwrap_or_else(|| "—".into())
                };
                ui.label(current_text);
                if ui.small_button("Delete").clicked() {
                    remove_id = Some(alert.id);
                }
                ui.end_row();
            }
        });
        if let Some(id) = remove_id {
            self.settings.alerts.retain(|a| a.id != id);
            self.semantic_alert_state.remove(&id);
            changed = true;
        }
        if changed {
            self.save_and_push_settings();
        }

        ui.add_space(8.0);
        ui.label("Threshold and benchmark alerts are edge-triggered and re-arm when their condition becomes false. Any-change alerts fire once per observed price move; live-market/fallback source switches are ignored.");

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
    }
}
