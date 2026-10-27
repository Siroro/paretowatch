//! Pareto tab: the cost/score scatter chart, the selected-model detail
//! card, and the Pareto-efficient table.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::DateTime;
use eframe::egui;
use egui_plot::{HoverPosition, Line, Plot, PlotPoint, PlotPoints, Points, Text};

use crate::artificial_analysis_snapshot::ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE;
use crate::bench::{format_percentile, normalize};
use crate::fetch::ON_DEMAND_REFRESH_SECS;
use crate::format::{
    format_compact_number, format_compact_usd, format_latency_ms, format_price_tick,
    format_throughput,
};
use crate::history::track::{ModelSeries, historical_low};
use crate::pareto::{JoinedPoint, pareto_search_matches, price_from_plot_x, price_to_plot_x};
use crate::theme::{
    PRICE_DOWN, PRICE_UP, copyable_slug, creator_color, discount_color, free_offer_badge,
    group_label,
};
use crate::types::{
    ANY_MODEL, AlertMode, BenchmarkMetric, BenchmarkSource, ComparisonMode, CostBasis,
    LiquidityFilter, ModalityFilter, PriceMetric, PricingSource, Quote, UncensoredFilter,
    blended_price,
};
use crate::worker::WorkerCommand;

use super::controls::{option_row, popover_button, radio_list};
use super::{ParetoWatchApp, Tab};

impl ParetoWatchApp {
    pub(super) fn pareto_tab(&mut self, ui: &mut egui::Ui) {
        let mut chart_axes_changed = false;
        let mut reset_zoom = false;
        let scaffold_options = self.cached_scaffolds();

        // One compact toolbar row. The axis selectors and the search box stay
        // on screen because they change the chart every time; everything else
        // hides behind the Filters and Workload mix popovers so the chart,
        // not the controls, owns the window.
        ui.horizontal_wrapped(|ui| {
            ui.label("Price:");
            let mut axis = (self.price_metric, self.cost_basis);
            egui::ComboBox::from_id_salt("price_axis")
                .selected_text(price_axis_label(axis))
                .show_ui(ui, |ui| {
                    for option in PRICE_AXES {
                        if ui
                            .selectable_value(&mut axis, option, price_axis_label(option))
                            .changed()
                        {
                            // A new cost basis changes what the plotted price
                            // means, so the selection's card would lie; a new
                            // metric on the same basis only re-scales x.
                            if option.1 != self.cost_basis {
                                self.selected_pareto_model = None;
                            }
                            self.price_metric = option.0;
                            self.cost_basis = option.1;
                            chart_axes_changed = true;
                        }
                    }
                });

            ui.separator();
            ui.label("Benchmark:");
            let combo = egui::ComboBox::from_id_salt("benchmark_source")
                .selected_text(self.benchmark_source.label())
                .show_ui(ui, |ui| {
                    for source in [
                        BenchmarkSource::CompositeAgentic,
                        BenchmarkSource::CompositeDeployment,
                    ]
                    .into_iter()
                    .chain(BenchmarkSource::display_sources())
                    {
                        if ui
                            .selectable_value(&mut self.benchmark_source, source, source.label())
                            .changed()
                        {
                            chart_axes_changed = true;
                            self.selected_pareto_model = None;
                        }
                    }
                });
            // Composite rows disclose their method on the axis label and
            // LiveBench picks its metric below; every raw board reports one
            // fixed metric, which rides along as a hover note.
            if let Some(metric) = benchmark_metric_note(self.benchmark_source) {
                combo.response.on_hover_text(metric);
            }

            if self.benchmark_source == BenchmarkSource::LiveBench {
                ui.separator();
                ui.label("Quality:");
                egui::ComboBox::from_id_salt("benchmark_metric")
                    .selected_text(self.benchmark_metric.label())
                    .show_ui(ui, |ui| {
                        for metric in [
                            BenchmarkMetric::Overall,
                            BenchmarkMetric::Coding,
                            BenchmarkMetric::AgenticCoding,
                        ] {
                            if ui
                                .selectable_value(
                                    &mut self.benchmark_metric,
                                    metric,
                                    metric.label(),
                                )
                                .changed()
                            {
                                chart_axes_changed = true;
                            }
                        }
                    });
            }

            ui.separator();
            let search_edit = egui::TextEdit::singleline(&mut self.pareto_search)
                .hint_text("highlight models, creators…")
                .desired_width(200.0)
                .clip_text(true);
            ui.add(search_edit);
            if !self.pareto_search.trim().is_empty()
                && ui.small_button("✗").on_hover_text("Clear search").clicked()
            {
                self.pareto_search.clear();
            }

            // Everything else lives behind two popover buttons. The count on
            // the Filters button says how many non-default filters are
            // shaping the chart, so a quiet chart is never a mystery.
            let active_filters =
                usize::from(self.comparison_mode != ComparisonMode::BestAvailableAgent)
                    + usize::from(self.liquidity_filter != LiquidityFilter::Any)
                    + usize::from(self.modality_filter != ModalityFilter::All)
                    + usize::from(self.uncensored_filter != UncensoredFilter::All)
                    + usize::from(self.pricing_source != PricingSource::All);
            let filters_label = if active_filters > 0 {
                format!("Filters · {active_filters} ▾")
            } else {
                "Filters ▾".to_owned()
            };
            popover_button(
                ui,
                "pareto_filters",
                egui::RichText::new(filters_label),
                Some("Pricing source, comparison basis, market quality, modality, safety, and price-axis options"),
                |ui| {
                    ui.set_min_width(400.0);
                    egui::Grid::new("pareto_filter_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Comparison").on_hover_text(
                                "How leaderboard rows are matched to priced models: the model's \
                                 own capability row, its best row under any agent harness, or one \
                                 fixed scaffold every model must use.",
                            );
                            if option_row(
                                ui,
                                &mut self.comparison_mode,
                                &[
                                    (ComparisonMode::ModelCapability, "Model capability"),
                                    (ComparisonMode::BestAvailableAgent, "Best available agent"),
                                    (ComparisonMode::CommonScaffold, "Same/common scaffold"),
                                ],
                            ) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Market quality").on_hover_text(
                                "Keep only models whose market clears a liquidity bar — a \
                                 trusted provider, or a floor on healthy sellers.",
                            );
                            if option_row(
                                ui,
                                &mut self.liquidity_filter,
                                &[
                                    (LiquidityFilter::Any, "Any"),
                                    (LiquidityFilter::Trusted, "Trusted"),
                                    (LiquidityFilter::Healthy3, "3+ healthy"),
                                    (LiquidityFilter::Healthy10, "10+ healthy"),
                                ],
                            ) {
                                chart_axes_changed = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Pricing").on_hover_text(
                                "Which prices the chart bills: live-market asks with catalogue \
                                 list prices filling in for models that have no live ask, \
                                 live-market asks only, or every model at its published list \
                                 rates — official prices, ignoring marketplace discounts.",
                            );
                            if option_row(
                                ui,
                                &mut self.pricing_source,
                                &[
                                    (PricingSource::All, "All sources"),
                                    (PricingSource::LiveMarket, "Live market"),
                                    (PricingSource::Catalog, "Catalog list"),
                                ],
                            ) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Modality");
                            if option_row(
                                ui,
                                &mut self.modality_filter,
                                &[
                                    (ModalityFilter::All, "All"),
                                    (ModalityFilter::Vision, "Vision"),
                                    (ModalityFilter::TextOnly, "Text-only"),
                                ],
                            ) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Safety");
                            if option_row(
                                ui,
                                &mut self.uncensored_filter,
                                &[
                                    (UncensoredFilter::All, "All"),
                                    (UncensoredFilter::Uncensored, "Uncensored"),
                                    (UncensoredFilter::SafetyTuned, "Safety-tuned"),
                                ],
                            ) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();
                        });
                    if self.comparison_mode == ComparisonMode::CommonScaffold {
                        ui.label("Scaffold (every model scored under it):");
                        let scaffold_rows: Vec<(String, String)> = scaffold_options
                            .iter()
                            .map(|s| (s.clone(), s.clone()))
                            .collect();
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if radio_list(ui, &mut self.common_scaffold, &scaffold_rows) {
                                    chart_axes_changed = true;
                                    self.selected_pareto_model = None;
                                }
                            });
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.log_price_axis, "Log price axis")
                        .changed()
                    {
                        reset_zoom = true;
                    }
                },
            );

            if self.price_metric == PriceMetric::Blended {
                let mix_label =
                    format!("Workload mix{} ▾", if self.settings_dirty { " ●" } else { "" });
                popover_button(
                    ui,
                    "pareto_weights",
                    egui::RichText::new(mix_label),
                    Some(
                        "Workload mix behind the blended price (fresh input / cache read / output)",
                    ),
                    |ui| {
                        ui.set_min_width(320.0);
                        ui.strong("Workload mix");
                        ui.small(
                            "Share of a token workload billed at each leg. Apply saves the mix, \
                             refreshes prices, and re-renders every view.",
                        );
                        ui.horizontal(|ui| {
                            for (label, mix) in [
                                ("Agentic 15/80/5", (15.0, 80.0, 5.0)),
                                ("No-cache 95/0/5", (95.0, 0.0, 5.0)),
                                ("Balanced 50/0/50", (50.0, 0.0, 50.0)),
                            ] {
                                if ui.small_button(label).clicked() {
                                    self.settings.input_weight = mix.0;
                                    self.settings.cache_read_weight = mix.1;
                                    self.settings.output_weight = mix.2;
                                    self.settings_dirty = true;
                                }
                            }
                        });
                        let total = self.settings.input_weight.max(0.0)
                            + self.settings.cache_read_weight.max(0.0)
                            + self.settings.output_weight.max(0.0);
                        if total > f64::EPSILON {
                            ui.small(format!(
                                "Current mix: {:.0}% fresh input · {:.0}% cache read · {:.0}% output",
                                self.settings.input_weight.max(0.0) / total * 100.0,
                                self.settings.cache_read_weight.max(0.0) / total * 100.0,
                                self.settings.output_weight.max(0.0) / total * 100.0,
                            ));
                        }
                        egui::Grid::new("pareto_weights_grid")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Fresh input");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.settings.input_weight)
                                            .speed(0.1)
                                            .range(0.0..=100.0),
                                    )
                                    .changed()
                                {
                                    self.settings_dirty = true;
                                }
                                ui.end_row();
                                ui.label("Cache read");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.settings.cache_read_weight)
                                            .speed(0.1)
                                            .range(0.0..=100.0),
                                    )
                                    .changed()
                                {
                                    self.settings_dirty = true;
                                }
                                ui.end_row();
                                ui.label("Output");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.settings.output_weight)
                                            .speed(0.1)
                                            .range(0.0..=100.0),
                                    )
                                    .changed()
                                {
                                    self.settings_dirty = true;
                                }
                                ui.end_row();
                                ui.label("Assumed cache %").on_hover_text(
                                    "Cache-read price assumed for models with no published cache \
                                     rate (no provider ask and no catalogue price), as a share of \
                                     the fresh-input price. Published rates are always preferred.",
                                );
                                if ui
                                    .add(
                                        egui::DragValue::new(
                                            &mut self.settings.cache_read_fallback_pct,
                                        )
                                        .speed(1.0)
                                        .range(0.0..=100.0)
                                        .suffix("%"),
                                    )
                                    .changed()
                                {
                                    self.settings_dirty = true;
                                }
                                ui.end_row();
                            });
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(self.settings_dirty, egui::Button::new("Apply"))
                                .on_hover_text("Save the workload mix and refresh")
                                .clicked()
                            {
                                self.save_and_push_settings();
                                self.weights_applied_at = Some(ui.input(|i| i.time));
                                chart_axes_changed = true;
                                let _ = self.worker_tx.send(WorkerCommand::Refresh);
                            }
                            if let Some(at) = self.weights_applied_at {
                                const APPLIED_SECS: f64 = 2.5;
                                let age = ui.input(|i| i.time) - at;
                                if age < APPLIED_SECS {
                                    ui.colored_label(PRICE_DOWN, "✓ Applied");
                                    ui.ctx().request_repaint_after(
                                        std::time::Duration::from_secs_f64(
                                            (APPLIED_SECS - age).max(0.05),
                                        ),
                                    );
                                }
                            }
                        });
                    },
                );
            }

            if ui
                .small_button("Reset zoom")
                .on_hover_text(
                    "Fit every visible model again (double-clicking the chart works too)",
                )
                .clicked()
            {
                reset_zoom = true;
            }
        });

        if self.cost_basis == CostBasis::EstimatedPerTask {
            ui.small("Est. $/task = benchmark-observed tokens/task × current Surplus blended $/1M. Benchmark-reported historical dollar costs are never used.");
        }

        // Resolved before the controls so the legend can toggle providers without
        // borrowing `self` inside the scroll closure; the call is cached, so
        // the chart body below sees the same view.
        let view = self.ensure_pareto_view();

        // The provider legend collapsed to one button. The clickable chips
        // live inside the popover, so a twenty-provider palette cannot eat
        // three wrapped rows above the chart.
        if let Some(cache) = view.as_deref() {
            let mut legend_groups: HashMap<&str, usize> = HashMap::new();
            for p in &cache.joined {
                *legend_groups.entry(group_label(&p.creator)).or_insert(0) += 1;
            }
            let mut legend: Vec<(&str, usize)> = legend_groups.into_iter().collect();
            // Enabled providers first, biggest first; hidden chips trail at the
            // end so a disabled family can always be found again.
            legend.sort_by(|a, b| {
                let a_hidden = self.hidden_groups.contains(a.0);
                let b_hidden = self.hidden_groups.contains(b.0);
                a_hidden
                    .cmp(&b_hidden)
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.cmp(b.0))
            });
            let hidden_count = legend
                .iter()
                .filter(|(name, _)| self.hidden_groups.contains(*name))
                .count();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let label = if hidden_count > 0 {
                    format!("Providers · {hidden_count}/{} hidden ▾", legend.len())
                } else {
                    "Providers ▾".to_owned()
                };
                popover_button(
                    ui,
                    "pareto_groups",
                    egui::RichText::new(label),
                    Some(
                        "Chart colour legend by provider — click a chip to hide or show its models",
                    ),
                    |ui| {
                        ui.set_min_width(380.0);
                        ui.horizontal(|ui| {
                            if hidden_count > 0 && ui.small_button("Show all").clicked() {
                                self.hidden_groups.clear();
                                reset_zoom = true;
                            }
                            if hidden_count < legend.len() && ui.small_button("Hide all").clicked()
                            {
                                for &(name, _) in &legend {
                                    self.hidden_groups.insert(name.to_owned());
                                }
                                // The selection is hidden along with every
                                // provider; drop it instead of pointing at an
                                // invisible orb.
                                self.selected_pareto_model = None;
                                reset_zoom = true;
                            }
                        });
                        // Clicking a chip hides that provider's orbs; clicking
                        // a disabled chip brings it back. Hidden chips stay
                        // listed (dimmed, struck through) so they are always
                        // re-enableable.
                        ui.horizontal_wrapped(|ui| {
                            for &(name, count) in &legend {
                                let is_hidden = self.hidden_groups.contains(name);
                                let base = creator_color(name);
                                let dot = if is_hidden {
                                    egui::Color32::from_rgba_unmultiplied(
                                        base.r(),
                                        base.g(),
                                        base.b(),
                                        80,
                                    )
                                } else {
                                    base
                                };
                                let chip = egui::RichText::new(format!("● {name} ({count})"))
                                    .color(dot)
                                    .size(13.0);
                                let chip = if is_hidden {
                                    chip.strikethrough()
                                } else {
                                    chip
                                };
                                if ui.small_button(chip).clicked() {
                                    if is_hidden {
                                        self.hidden_groups.remove(name);
                                    } else {
                                        self.hidden_groups.insert(name.to_owned());
                                        // A selection hidden along with its
                                        // provider would point at an
                                        // invisible orb; drop it too.
                                        if let Some(selected_id) = &self.selected_pareto_model
                                            && let Some(p) = cache
                                                .joined
                                                .iter()
                                                .find(|p| &p.model_id == selected_id)
                                            && group_label(&p.creator) == name
                                        {
                                            self.selected_pareto_model = None;
                                        }
                                    }
                                    reset_zoom = true;
                                }
                                ui.add_space(2.0);
                            }
                        });
                    },
                );
            });
            ui.add_space(2.0);
        }

        ui.label(
            egui::RichText::new(
                "Wheel: zoom · middle-drag: pan · right-drag: box zoom · double-click: reset · click an orb: inspect · right-click an orb: pin a price widget",
            )
            .small()
            .weak(),
        );

        let reset_zoom = reset_zoom || chart_axes_changed;

        // The whole chart body scrolls as one page so the detail card and
        // frontier table can never be pushed out of the viewport.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
        ui.add_space(6.0);
        if !self.benchmark_source.is_composite()
            && let Some(err) = self.benchmark_errors.get(&self.benchmark_source) {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{}: {err}", self.benchmark_source.label()),
                );
            }

        if self.price_snapshot.is_none() {
            ui.spinner();
            ui.label("Waiting for pricing data…");
            return;
        }
        let (joined, visible, frontier, benchmarks_present) = match view.as_deref() {
            Some(cache) => (
                cache.joined.as_slice(),
                cache.visible.as_slice(),
                cache.frontier.as_slice(),
                cache.benchmarks_present,
            ),
            None => (&[][..], &[][..], &[][..], false),
        };

        if joined.is_empty() {
            if self.cost_basis == CostBasis::EstimatedPerTask && benchmarks_present {
                ui.label(format!(
                    "{} has no matched rows with usable tokens-per-task telemetry for this comparison mode. Switch to $/1M or choose SWE-rebench, DeepSWE, Terminal-Bench, or a composite model with one of those profiles.",
                    self.benchmark_source.label()
                ));
                return;
            }
            if !benchmarks_present {
                let raw_rows = if self.benchmark_source.is_composite() {
                    0
                } else {
                    self.benchmark_sets.get(&self.benchmark_source).map(Vec::len).unwrap_or(0)
                };
                if raw_rows > 0 {
                    ui.label(format!(
                        "{} has {} raw rows, but none fit the '{}' comparison mode{}.",
                        self.benchmark_source.label(),
                        raw_rows,
                        self.comparison_mode.label(),
                        if self.comparison_mode == ComparisonMode::CommonScaffold {
                            format!(" for {}", self.common_scaffold)
                        } else {
                            String::new()
                        }
                    ));
                    ui.small("Try 'Best available agent' for agent+harness leaderboards, or choose a common scaffold that this source actually reports.");
                } else {
                    ui.label(format!("Waiting for {} scores…", self.benchmark_source.label()));
                }
            } else {
                ui.label(format!(
                    "No confident Surplus / {} model matches yet.",
                    self.benchmark_source.label()
                ));
                if self.modality_filter != ModalityFilter::All {
                    ui.small(format!(
                        "'{}' filter is active — models without matching {} metadata are hidden.",
                        self.modality_filter.label(),
                        if self.modality_filter == ModalityFilter::Vision { "image-input" } else { "text-only" },
                    ));
                }
                if self.uncensored_filter != UncensoredFilter::All {
                    ui.small(format!(
                        "'{}' filter is active — {} listings are hidden.",
                        self.uncensored_filter.label(),
                        if self.uncensored_filter == UncensoredFilter::Uncensored {
                            "everything but uncensored"
                        } else {
                            "uncensored"
                        },
                    ));
                }
                if self.pricing_source == PricingSource::LiveMarket {
                    ui.small("'Live market' pricing is active — models with no live-market ask are hidden.");
                }
            }
            return;
        }
        if visible.is_empty() {
            ui.label("Every provider is hidden — open Providers above and re-enable a chip to bring models back.");
            return;
        }
        let frontier_names: std::collections::HashSet<&str> = frontier.iter().map(|p| p.model_id.as_str()).collect();
        let hidden_frontier = self.settings.hidden_frontier_set();
        let selected_before = self.selected_pareto_model.clone();
        let search_query = self.pareto_search.clone();
        let search_active = !normalize(&search_query).replace(' ', "").is_empty();
        let search_matches: Vec<bool> = visible
            .iter()
            .map(|p| !search_active || pareto_search_matches(&search_query, p))
            .collect();
        ui.add_space(4.0);
        if search_active {
            let total = search_matches.iter().filter(|m| **m).count();
            ui.small(format!(
                "Search “{}”: {} of {} shown models match (non-matches are dimmed).",
                search_query.trim(),
                total,
                visible.len()
            ));
            ui.add_space(4.0);
        }

        let log_price_axis = self.log_price_axis;
        let cost_basis = self.cost_basis;
        let pricing_source = self.pricing_source;
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
            self.settings.cache_read_fallback_pct,
        );
        // Every orb is hoverable, so the tooltip formatter must be able to
        // resolve any series name back to its model: plain orbs are named
        // after the display name, while the selection/search halos carry a
        // `*_ring_{model_id}` prefix.
        let series_to_id: HashMap<String, String> = joined
            .iter()
            .map(|p| (p.model.clone(), p.model_id.clone()))
            .collect();
        let hover_cache = Arc::clone(view.as_ref().expect("joined is non-empty"));
        let history = &self.history;
        let score_source = match self.benchmark_source {
            BenchmarkSource::LiveBench => {
                format!("LiveBench {}", self.benchmark_metric.label())
            }
            source => source.short_label().to_owned(),
        };
        // On the per-task basis the plotted price is the blended rate scaled
        // by the benchmark's token volume, so the historical low must be
        // blended as well to stay comparable.
        let low_metric = if cost_basis == CostBasis::EstimatedPerTask {
            PriceMetric::Blended
        } else {
            self.price_metric
        };
        let x_axis_label = match (self.cost_basis, log_price_axis) {
            (CostBasis::PerMillion, true) => format!("{} price ($ / 1M, log scale)", self.price_metric.label()),
            (CostBasis::PerMillion, false) => format!("{} price ($ / 1M)", self.price_metric.label()),
            (CostBasis::EstimatedPerTask, true) => "Estimated blended cost / benchmark task ($, log scale)".to_owned(),
            (CostBasis::EstimatedPerTask, false) => "Estimated blended cost / benchmark task ($)".to_owned(),
        };
        let benchmark_axis_label = match self.benchmark_source {
            BenchmarkSource::CompositeAgentic => "Capability composite percentile (posterior)".to_owned(),
            BenchmarkSource::CompositeDeployment => "Deployment composite percentile (posterior)".to_owned(),
            BenchmarkSource::ArtificialAnalysisSnapshot => format!(
                "AA Intelligence Index (live over {ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE} snapshot)"
            ),
            BenchmarkSource::SWERebench => "SWE-rebench Result@1".to_owned(),
            BenchmarkSource::TerminalBench3 => "Terminal-Bench 3.0 accuracy".to_owned(),
            BenchmarkSource::TerminalBench4 => "Terminal-Bench 4.0 accuracy".to_owned(),
            BenchmarkSource::DeepSWE11 => "DeepSWE v1.1 pass@1".to_owned(),
            BenchmarkSource::LiveBench => format!("LiveBench {} score", self.benchmark_metric.label()),
            BenchmarkSource::ReveloCodeIndex => "Revelo Code Index".to_owned(),
            BenchmarkSource::DesignArena => "Design Arena Elo".to_owned(),
            BenchmarkSource::FrontierCode => "FrontierCode 1.1 mergeability score".to_owned(),
            BenchmarkSource::AABriefcase => "AA-Briefcase Elo (blind pairwise)".to_owned(),
            BenchmarkSource::AutomationBenchAA => {
                "AutomationBench-AA partial score".to_owned()
            }
            BenchmarkSource::SWEBenchPro => "SWE-Bench Pro V2 HARD resolve rate".to_owned(),
            BenchmarkSource::ValsSREBench => "SRE Bench binary RE accuracy".to_owned(),
        };

        // The chart body scrolls as one page with the card and table below it,
        // so the plot takes a fixed share of the window instead of whatever is
        // left after the (variable) detail card. The single toolbar row leaves
        // more of that window for the chart itself.
        let screen_height = ui.ctx().input(|i| i.viewport_rect().height());
        let plot_height = (screen_height * 0.58).clamp(320.0, 760.0);
        // Bigger, calmer chart typography; restored right after the plot so the
        // detail cards below keep their normal sizing.
        let saved_style = ui.style().clone();
        {
            let style = ui.style_mut();
            style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
            style.visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));
        }
        let (clicked_model, right_clicked_model) = Plot::new("pareto_plot")
            .height(plot_height)
            .x_axis_label(x_axis_label)
            .x_axis_formatter(move |mark, _range| {
                let price = price_from_plot_x(mark.value, log_price_axis);
                format_price_tick(price)
            })
            .y_axis_label(benchmark_axis_label)
            .allow_zoom(true)
            .allow_scroll(true)
            .allow_drag(true)
            .allow_axis_zoom_drag(true)
            .allow_boxed_zoom(true)
            .allow_double_click_reset(true)
            .pan_pointer_button(egui::PointerButton::Middle)
            .boxed_zoom_pointer_button(egui::PointerButton::Secondary)
            .set_margin_fraction(egui::vec2(0.04, 0.06))
            .label_formatter(move |pos| match pos {
                HoverPosition::NearDataPoint { plot_name, position, .. } if !plot_name.is_empty() => {
                    let model_id = plot_name
                        .strip_prefix("sel_ring_")
                        .or_else(|| plot_name.strip_prefix("search_ring_"))
                        .map(str::to_owned)
                        .or_else(|| series_to_id.get(*plot_name).cloned())?;
                    let p = hover_cache
                        .joined
                        .iter()
                        .find(|p| p.model_id == model_id)?;
                    let quote = hover_cache
                        .filtered_quotes
                        .iter()
                        .find(|q| q.model == model_id);
                    let ctx = HoverContext {
                        quote,
                        history: history.series(&model_id),
                        low_metric,
                        cost_basis,
                        weights,
                        score_source: score_source.clone(),
                        pricing_source,
                    };
                    Some(pareto_hover_label(p, &ctx, position.y))
                }
                _ => None,
            })
            .show(ui, |plot_ui| {
                if reset_zoom {
                    plot_ui.set_auto_bounds(true);
                }

                for (p, matches_search) in visible.iter().zip(search_matches.iter().copied()) {
                    let is_frontier = frontier_names.contains(p.model_id.as_str());
                    let is_hidden = hidden_frontier.contains(p.model_id.as_str());
                    let is_selected = selected_before.as_deref() == Some(p.model_id.as_str());
                    let base = creator_color(&p.creator);
                    // Non-matching models fade back so matches pop during
                    // search; frontier-hidden models stay faded even when
                    // they match — hiding is a deliberate standing state.
                    let color = if is_hidden {
                        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 45)
                    } else if matches_search {
                        base
                    } else {
                        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 55)
                    };
                    let mut radius: f32 = if is_selected { 8.0 } else if is_frontier { 6.0 } else { 4.0 };
                    if search_active && matches_search {
                        radius = (radius + 2.0).max(6.5);
                    }
                    let plot_x = price_to_plot_x(p.cost, log_price_axis);
                    let pts = PlotPoints::from(vec![[plot_x, p.score]]);
                    plot_ui.points(Points::new(p.model.clone(), pts).radius(radius).color(color));
                    if is_selected {
                        // egui_plot paints highlighted items last — above our
                        // labels. A hollow ring drawn in normal order keeps the
                        // selection emphasis underneath the text.
                        plot_ui.points(
                            Points::new(
                                format!("sel_ring_{}", p.model_id),
                                PlotPoints::from(vec![[plot_x, p.score]]),
                            )
                            .radius(radius + 4.0)
                            .color(egui::Color32::WHITE)
                            .filled(false),
                        );
                    } else if search_active && matches_search {
                        // Search matches get a slim halo in their provider colour.
                        plot_ui.points(
                            Points::new(
                                format!("search_ring_{}", p.model_id),
                                PlotPoints::from(vec![[plot_x, p.score]]),
                            )
                            .radius(radius + 3.0)
                            .color(base)
                            .filled(false),
                        );
                    }
                }
                if frontier.len() >= 2 {
                    let line_pts = PlotPoints::from(
                        frontier
                            .iter()
                            .map(|p| [price_to_plot_x(p.cost, log_price_axis), p.score])
                            .collect::<Vec<_>>(),
                    );
                    // The line is decoration only. Disabling hover prevents it from
                    // stealing the tooltip from a frontier orb and showing the generic
                    // "Pareto frontier" name instead of the model name.
                    plot_ui.line(
                        Line::new("Pareto frontier", line_pts)
                            .width(2.0)
                            .allow_hover(false),
                    );
                }

                // Frontier points are few and important: label them directly so the
                // chart is readable without having to hover each orb. Selected
                // points and search matches are labelled as well. Labels are added
                // after every bubble and the frontier line because egui_plot paints
                // elements in insertion order — this keeps text above the geometry
                // instead of getting overdrawn by it.
                for (p, matches_search) in visible.iter().zip(search_matches.iter().copied()) {
                    if !matches_search && search_active {
                        continue;
                    }
                    let is_frontier = frontier_names.contains(p.model_id.as_str());
                    let is_selected = selected_before.as_deref() == Some(p.model_id.as_str());
                    if !is_frontier && !is_selected && !(search_active && matches_search) {
                        continue;
                    }
                    let plot_x = price_to_plot_x(p.cost, log_price_axis);
                    // Search-only matches carry their provider colour; frontier and
                    // selection labels stay in the calm near-white.
                    let label_color = if search_active && !is_frontier && !is_selected {
                        creator_color(&p.creator)
                    } else {
                        egui::Color32::from_rgb(238, 243, 250)
                    };
                    plot_ui.text(
                        Text::new(
                            format!("label_{}", p.model_id),
                            PlotPoint::new(plot_x, p.score + 0.8),
                            p.model.clone(),
                        )
                        .anchor(egui::Align2::CENTER_BOTTOM)
                        .color(label_color)
                        .allow_hover(false),
                    );
                }

                let nearest_orb = |click_pos: egui::Pos2| {
                    visible
                        .iter()
                        .filter_map(|p| {
                            let plot_x = price_to_plot_x(p.cost, log_price_axis);
                            let screen_pos = plot_ui.screen_from_plot(PlotPoint::new(plot_x, p.score));
                            let distance = (screen_pos - click_pos).length();
                            (distance <= 16.0).then_some((distance, p.model_id.clone()))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, model_id)| model_id)
                };
                let response = plot_ui.response();
                if response.clicked() {
                    if let Some(click_pos) = response.interact_pointer_pos() {
                        (nearest_orb(click_pos), None)
                    } else {
                        (None, None)
                    }
                } else if response.secondary_clicked() {
                    // A right-click without drag is not a box zoom; near an
                    // orb it pins a floating price widget.
                    if let Some(click_pos) = response.interact_pointer_pos() {
                        (None, nearest_orb(click_pos).map(|model_id| (model_id, click_pos)))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            })
            .inner;
        ui.set_style(saved_style);

        if let Some(model_id) = clicked_model {
            self.selected_pareto_model = Some(model_id);
        }
        if let Some((model_id, at)) = right_clicked_model {
            self.widgets.spawn(
                ui.ctx(),
                self.price_snapshot.as_ref(),
                &self.settings,
                self.liquidity_filter,
                &model_id,
                at,
            );
        }

        let selected_id = self.selected_pareto_model.clone();
        let selected = selected_id
            .as_deref()
            .and_then(|id| visible.iter().find(|p| p.model_id.as_str() == id));

        ui.horizontal(|ui| {
            ui.strong(format!("{} matched models", visible.len()));
            if !self.hidden_groups.is_empty() {
                ui.small(format!(
                    "{} provider{} hidden in Providers",
                    self.hidden_groups.len(),
                    if self.hidden_groups.len() == 1 { "" } else { "s" }
                ));
            }
            if !self.settings.hidden_frontier_models.is_empty() {
                ui.separator();
                ui.small(format!(
                    "{} model{} hidden from the frontier (faded)",
                    self.settings.hidden_frontier_models.len(),
                    if self.settings.hidden_frontier_models.len() == 1 { "" } else { "s" }
                ));
                if ui.small_button("Show all on frontier").clicked() {
                    self.restore_all_frontier_hidden();
                }
            }
            // One quiet line listing every non-default filter, so a thin
            // chart always says why it is thin.
            let mut active_filters: Vec<String> = Vec::new();
            if self.comparison_mode != ComparisonMode::BestAvailableAgent {
                active_filters.push(self.comparison_mode.label().to_owned());
            }
            if self.liquidity_filter != LiquidityFilter::Any {
                active_filters.push(self.liquidity_filter.label().to_owned());
            }
            if self.modality_filter != ModalityFilter::All {
                active_filters.push(self.modality_filter.label().to_owned());
            }
            if self.uncensored_filter != UncensoredFilter::All {
                active_filters.push(self.uncensored_filter.label().to_owned());
            }
            match self.pricing_source {
                PricingSource::All => {}
                PricingSource::LiveMarket => active_filters.push("live market only".to_owned()),
                PricingSource::Catalog => active_filters.push("catalog list pricing".to_owned()),
            }
            if !active_filters.is_empty() {
                ui.separator();
                ui.small(format!("Filters: {}", active_filters.join(" · ")));
            }
            ui.separator();
            ui.strong(format!("{} Pareto-efficient", frontier.len()));
            if search_active {
                ui.separator();
                let total = search_matches.iter().filter(|m| **m).count();
                ui.strong(format!("{total} match search"));
            }
        });

        // Detail card and frontier table share one row when the window is wide
        // enough; narrow windows keep them stacked (card above, table below).
        if let (true, Some(p)) = (ui.available_width() >= 860.0, selected) {
            let card_width = (ui.available_width() * 0.42).clamp(340.0, 560.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(card_width);
                    ui.add_space(6.0);
                    self.pareto_detail_card(ui, p);
                });
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    self.frontier_table(ui, frontier);
                });
            });
        } else {
            if let Some(p) = selected {
                ui.add_space(6.0);
                self.pareto_detail_card(ui, p);
            }
            self.frontier_table(ui, frontier);
        }
        });
    }

    fn pareto_detail_card(&mut self, ui: &mut egui::Ui, p: &JoinedPoint) {
        let selected_quote = self.selected_pareto_quote(&p.model_id);
        let consensus = selected_quote
            .as_ref()
            .and_then(|quote| self.cached_consensus(quote));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("●")
                        .color(creator_color(&p.creator))
                        .size(16.0),
                );
                ui.heading(&p.model);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✗")
                        .on_hover_text("Clear the selection")
                        .clicked()
                    {
                        self.selected_pareto_model = None;
                    }
                    ui.menu_button(egui::RichText::new("🔔").size(16.0), |ui| {
                        if self.cost_basis == CostBasis::PerMillion
                            && ui.button("Price threshold").clicked()
                        {
                            self.alert_model = p.model_id.clone();
                            self.alert_mode = AlertMode::Threshold;
                            self.alert_metric = self.price_metric;
                            self.alert_cost_basis = CostBasis::PerMillion;
                            self.alert_threshold = p.cost;
                            self.tab = Tab::Alerts;
                            ui.close();
                        }
                        if ui.button("Any price change").clicked() {
                            self.alert_model = p.model_id.clone();
                            self.alert_mode = AlertMode::AnyChange;
                            self.tab = Tab::Alerts;
                            ui.close();
                        }
                        if ui.button("Pareto frontier entry").clicked() {
                            self.alert_model = p.model_id.clone();
                            self.alert_mode = AlertMode::EntersFrontier;
                            self.alert_metric = self.price_metric;
                            self.alert_cost_basis = self.cost_basis;
                            self.tab = Tab::Alerts;
                            ui.close();
                        }
                        if ui.button("Pareto frontier exit").clicked() {
                            self.alert_model = p.model_id.clone();
                            self.alert_mode = AlertMode::LeavesFrontier;
                            self.alert_metric = self.price_metric;
                            self.alert_cost_basis = self.cost_basis;
                            self.tab = Tab::Alerts;
                            ui.close();
                        }
                        if ui.button("Cheapest at or above current score").clicked() {
                            self.alert_model = p.model_id.clone();
                            self.alert_mode = AlertMode::CheapestAboveScore;
                            self.alert_metric = self.price_metric;
                            self.alert_cost_basis = self.cost_basis;
                            self.alert_score_threshold = p.score;
                            self.tab = Tab::Alerts;
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Add alarm");
                    // Frontier visibility: hidden models keep a faded orb but
                    // leave the frontier line and its entry/exit alerts.
                    let hidden = self
                        .settings
                        .hidden_frontier_models
                        .iter()
                        .any(|m| m == &p.model_id);
                    let toggle = if hidden {
                        "⊕ Show on frontier"
                    } else {
                        "⊖ Hide from frontier"
                    };
                    if ui
                        .button(toggle)
                        .on_hover_text(
                            "Hidden models stay on the chart (faded) — the frontier line \
                             and frontier entry/exit alerts skip them",
                        )
                        .clicked()
                    {
                        self.toggle_frontier_hidden(&p.model_id);
                    }
                });
            });
            copyable_slug(ui, &p.model_id);
            ui.horizontal_wrapped(|ui| {
                if p.vision {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "vision");
                } else {
                    ui.small("text-only");
                }
                if p.uncensored {
                    ui.colored_label(egui::Color32::from_rgb(240, 170, 60), "uncensored")
                        .on_hover_text(
                            "Listing presents itself as uncensored — refusal behavior removed \
                             (abliterated/uncensored finetune)",
                        );
                }
                if self.pricing_source == PricingSource::Catalog {
                    ui.strong("catalog list price").on_hover_text(
                        "Catalog pricing is active — every model is billed at its published \
                         list rates, not the live-market asks",
                    );
                } else if p.live_market {
                    ui.strong("live market");
                    if p.free_offer_listed {
                        free_offer_badge(ui);
                    }
                } else {
                    ui.label("comparison/catalog price");
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Selected cost: ${:.4} {}",
                    p.cost,
                    self.cost_basis.unit()
                ));
                ui.separator();
                ui.label(match self.benchmark_source {
                    BenchmarkSource::LiveBench => format!(
                        "LiveBench {}: {:.1}",
                        self.benchmark_metric.label(),
                        p.score
                    ),
                    _ => format!("{}: {:.1}", self.benchmark_source.short_label(), p.score),
                });
                ui.separator();
                ui.label(format!("Input ${:.4}", p.input));
                if let Some(cache_read) = p.cache_read {
                    ui.separator();
                    ui.label(format!("Cache read ${:.4}", cache_read));
                }
                ui.separator();
                ui.label(format!("Output ${:.4}", p.output));
            });
            if self.cost_basis == CostBasis::EstimatedPerTask
                && let Some(tokens) = p.tokens_per_task
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "Benchmark workload: {} tokens/task",
                        format_compact_number(tokens)
                    ));
                    if let Some(profile) = &p.token_profile {
                        ui.separator();
                        ui.small(profile);
                    }
                    ui.separator();
                    ui.small("repriced with current Surplus blended rate");
                });
            }
            ui.horizontal_wrapped(|ui| {
                let mut shown = false;
                if !p.creator.is_empty() {
                    ui.label(format!("Creator: {}", p.creator));
                    shown = true;
                }
                if !p.provider.is_empty() {
                    if shown {
                        ui.separator();
                    }
                    ui.label(format!("Provider: {}", p.provider));
                    shown = true;
                }
                // Composite row names carry the full method disclosure and the
                // source-by-source grid below repeats it; skip the duplicate.
                if !self.benchmark_source.is_composite() {
                    if shown {
                        ui.separator();
                    }
                    ui.label(format!(
                        "{} match: {}",
                        self.benchmark_source.short_label(),
                        p.benchmark_name
                    ));
                }
            });

            if let Some(quote) = &selected_quote {
                // Speed summary: AA's standardized medians lead (true
                // percentiles under a uniform protocol); the live-market
                // buckets follow as the marketplace-reality secondary. The
                // per-provider split joins in the market-quality grid below.
                let aa_entry = self
                    .aa_speed_lookup()
                    .and_then(|join| join.get(&quote.model).cloned());
                ui.horizontal_wrapped(|ui| {
                    ui.label("Speed:");
                    if let Some(entry) = &aa_entry {
                        match (entry.median_tps, entry.median_ttft_s) {
                            (Some(tps), Some(ttft)) => {
                                ui.label(format!(
                                    "{} \u{00b7} TTFT {}",
                                    format_throughput(tps),
                                    format_latency_ms(ttft * 1000.0)
                                ));
                            }
                            (Some(tps), None) => {
                                ui.label(format_throughput(tps));
                            }
                            (None, Some(ttft)) => {
                                ui.label(format!("TTFT {}", format_latency_ms(ttft * 1000.0)));
                            }
                            (None, None) => {}
                        };
                        let band = match (entry.tps_p05, entry.tps_p95) {
                            (Some(lo), Some(hi)) => format!(
                                " \u{00b7} p05\u{2013}p95 {}\u{2013}{}",
                                format_throughput(lo),
                                format_throughput(hi)
                            ),
                            _ => String::new(),
                        };
                        ui.small(format!("AA standardized{band}"))
                            .on_hover_text(format!(
                                "Median of AA's controlled generations under a uniform \
                             protocol{} \u{2014} independent of any marketplace route",
                                entry
                                    .effort
                                    .as_deref()
                                    .map(|e| format!(" ({e} variant)"))
                                    .unwrap_or_default(),
                            ));
                    }
                    match (quote.throughput_p50, quote.ttft_p50_ms) {
                        (Some(tps), Some(ttft)) => {
                            ui.label(format!(
                                "{}\u{2020} \u{00b7} TTFT {}\u{2020}",
                                format_throughput(tps),
                                format_latency_ms(ttft)
                            ));
                        }
                        (Some(tps), None) => {
                            ui.label(format!("{}\u{2020}", format_throughput(tps)));
                        }
                        (None, Some(ttft)) => {
                            ui.label(format!("TTFT {}\u{2020}", format_latency_ms(ttft)));
                        }
                        (None, None) => {}
                    };
                    if let Some(pct) = quote.uptime_pct {
                        ui.separator();
                        ui.label(format!("{pct:.1}% uptime"));
                    }
                    if quote.throughput_p50.is_some() || quote.ttft_p50_ms.is_some() {
                        ui.separator();
                        ui.small("\u{2020} market buckets \u{00b7} 24h")
                            .on_hover_text(
                                "Live-market medians pooled across every seller \u{2014} the feed \
                             publishes only coarse histogram midpoints, so treat them as \
                             approximate",
                            );
                    }
                    if aa_entry.is_none()
                        && quote.throughput_p50.is_none()
                        && quote.ttft_p50_ms.is_none()
                    {
                        ui.label(egui::RichText::new("no speed data").weak());
                    }
                });
                // Selecting the card is the on-demand trigger for this
                // model's per-provider breakdown.
                self.ensure_performance(&quote.model, ON_DEMAND_REFRESH_SECS);
                self.market_quality_section(ui, quote);
            }

            if let Some(consensus) = &consensus {
                ui.collapsing("Source-by-source percentile", |ui| {
                    // Bounded so an open grid cannot push the Pareto-efficient
                    // table below the window edge; long boards scroll inside.
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            egui::Grid::new("selected_consensus_grid")
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.strong("Benchmark");
                                    ui.strong("Percentile");
                                    ui.strong("Rank");
                                    ui.strong("Score");
                                    ui.strong("Matched row");
                                    ui.end_row();
                                    for entry in &consensus.entries {
                                        ui.label(entry.source.short_label());
                                        ui.label(format_percentile(entry.percentile));
                                        ui.label(format!("#{}/{}", entry.rank, entry.total));
                                        ui.label(format!("{:.1}", entry.score));
                                        ui.label(&entry.benchmark_name);
                                        ui.end_row();
                                    }
                                });
                        });
                });
            }
        });
    }

    /// "Market quality" collapsible: headline trust/seller/telemetry metadata
    /// plus every provider ask in this model's market with workload pricing.
    /// Shared by the Pareto detail card and the New listings cards.
    pub(super) fn market_quality_section(&self, ui: &mut egui::Ui, quote: &Quote) {
        let header = if quote.live_market {
            let trust = match quote.provider_trusted {
                Some(true) => "trusted",
                Some(false) => "untrusted",
                None => "trust unknown",
            };
            format!(
                "Market quality: {trust} \u{00b7} {} healthy sellers",
                quote.healthy_seller_count.unwrap_or(0)
            )
        } else {
            "Market quality: comparison/catalog quote".to_owned()
        };
        ui.collapsing(header, |ui| {
            if quote.live_market {
                ui.horizontal_wrapped(|ui| {
                    match quote.provider_trusted {
                        Some(true) => { ui.colored_label(egui::Color32::from_rgb(54, 179, 126), "trusted provider"); }
                        Some(false) => { ui.colored_label(egui::Color32::from_rgb(224, 86, 86), "untrusted provider"); }
                        None => { ui.label("trust unknown"); }
                    }
                    ui.separator();
                    ui.label(format!("{} healthy sellers", quote.healthy_seller_count.unwrap_or(0)));
                    if let Some(total) = quote.seller_count {
                        ui.separator();
                        ui.label(format!("{total} total sellers"));
                    }
                    if let Some(requests) = quote.requests_24h {
                        ui.separator();
                        ui.label(format!("{} requests / 24h", format_compact_number(requests as f64)));
                    }
                    if let Some(volume) = quote.volume_24h {
                        ui.separator();
                        ui.label(format!("{} volume / 24h", format_compact_usd(volume)));
                    }
                    if let Some(discount) = quote.discount_pct {
                        ui.separator();
                        let direction = quote.discount_direction.as_deref().unwrap_or("stable");
                        let label = if self.settings.include_cache_read_in_discount {
                            format!("{discount:.1}% workload discount vs list \u{00b7} {direction}")
                        } else {
                            format!("{discount:.1}% discount \u{00b7} {direction}")
                        };
                        ui.colored_label(discount_color(discount), label);
                    }
                });
                if quote.free_offer_listed {
                    ui.small(
                        "* A seller is listing this model free (100% off). Shown prices are the \
                         cheapest ask that actually costs money; the published market best is $0.",
                    );
                }
            } else {
                ui.label("market metadata unavailable on comparison/catalog quote");
            }
            if !quote.market_options.is_empty() {
                ui.add_space(3.0);
                // Per-provider speed joins in only when this model's route
                // breakdown has been fetched (on demand at selection).
                let perf = self.performance.get(&quote.model);
                ui.small(if perf.is_some() {
                    "All providers in this market (workload $/1M at current weights, \u{25cf} = plotted, \
                     tok/s \u{00b7} TTFT \u{00b7} uptime = 24h route telemetry):"
                } else {
                    "All providers in this market (workload $/1M at current weights, \u{25cf} = plotted):"
                });
                egui::Grid::new("market_quality_providers").striped(true).show(ui, |ui| {
                    ui.strong("");
                    ui.strong("Provider");
                    ui.strong("Input");
                    ui.strong("Cache read");
                    ui.strong("Output");
                    ui.strong("Workload");
                    ui.strong("Trust");
                    ui.strong("Healthy");
                    if perf.is_some() {
                        ui.strong("tok/s");
                        ui.strong("TTFT p50");
                        ui.strong("Uptime");
                    }
                    ui.end_row();
                    for option in &quote.market_options {
                        ui.label(if option.provider == quote.provider { "\u{25cf}" } else { "" });
                        ui.label(&option.provider);
                        ui.label(format!("${:.4}", option.input));
                        ui.label(match option.cache_read {
                            Some(rate) => format!("${rate:.4}"),
                            None => "\u{2014}".to_owned(),
                        });
                        ui.label(format!("${:.4}", option.output));
                        ui.label(format!(
                            "${:.4}",
                            option.workload_price(
                                self.settings.input_weight,
                                self.settings.cache_read_weight,
                                self.settings.output_weight,
                                self.settings.cache_read_fallback_pct,
                            )
                        ));
                        ui.label(match option.trusted {
                            Some(true) => "trusted",
                            Some(false) => "untrusted",
                            None => "\u{2014}",
                        });
                        ui.label(match option.healthy_seller_count {
                            Some(count) => format!("{count}"),
                            None => "\u{2014}".to_owned(),
                        });
                        if perf.is_some() {
                            let route = perf.and_then(|p| p.provider(&option.provider));
                            ui.label(match route.and_then(|r| r.tps_p50) {
                                Some(tps) => format_throughput(tps),
                                None => "\u{2014}".to_owned(),
                            });
                            ui.label(match route.and_then(|r| r.ttft_p50_ms) {
                                Some(ms) => format_latency_ms(ms),
                                None => "\u{2014}".to_owned(),
                            });
                            ui.label(match route.and_then(|r| r.uptime_pct) {
                                Some(pct) => format!("{pct:.1}%"),
                                None => "\u{2014}".to_owned(),
                            });
                        }
                        ui.end_row();
                    }
                });
            }
        });
    }

    fn frontier_table(&mut self, ui: &mut egui::Ui, frontier: &[JoinedPoint]) {
        ui.horizontal(|ui| {
            ui.strong("Pareto frontier");
            if ui.small_button("◆ Watch any entry").clicked() {
                self.alert_model = ANY_MODEL.into();
                self.alert_mode = AlertMode::EntersFrontier;
                self.alert_metric = self.price_metric;
                self.alert_cost_basis = self.cost_basis;
                self.tab = Tab::Alerts;
            }
            if ui.small_button("◇ Watch any exit").clicked() {
                self.alert_model = ANY_MODEL.into();
                self.alert_mode = AlertMode::LeavesFrontier;
                self.alert_metric = self.price_metric;
                self.alert_cost_basis = self.cost_basis;
                self.tab = Tab::Alerts;
            }
        });
        // Latest recorded blended-price move per frontier model, for the
        // ▲/▼ arrows next to prices. The log is newest-first, so the first
        // entry seen per model wins.
        let latest_moves: HashMap<String, f64> = {
            let frontier_ids: std::collections::HashSet<&str> =
                frontier.iter().map(|p| p.model_id.as_str()).collect();
            self.notifications
                .lock()
                .map(|log| {
                    let mut map = HashMap::new();
                    for change in log.price_moves() {
                        if frontier_ids.contains(change.model.as_str())
                            && !map.contains_key(&change.model)
                        {
                            map.insert(change.model.clone(), change.delta());
                        }
                    }
                    map
                })
                .unwrap_or_default()
        };
        let cheapest = frontier
            .iter()
            .map(|p| p.cost)
            .fold(f64::INFINITY, f64::min);
        egui::Grid::new("frontier_table")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Model");
                ui.strong(if self.cost_basis == CostBasis::EstimatedPerTask {
                    "Est. $/task"
                } else {
                    "Price"
                });
                ui.strong("Score");
                ui.strong("Source");
                ui.end_row();
                for p in frontier {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("●")
                                .color(creator_color(&p.creator))
                                .size(11.0),
                        );
                        if p.cost <= cheapest {
                            ui.label(
                                egui::RichText::new("★")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(246, 190, 80)),
                            )
                            .on_hover_text("Cheapest model on the current frontier");
                        }
                        let label = if p.free_offer_listed {
                            format!("{}*", p.model)
                        } else {
                            p.model.clone()
                        };
                        if ui
                            .selectable_label(
                                self.selected_pareto_model.as_deref() == Some(p.model_id.as_str()),
                                &label,
                            )
                            .clicked()
                        {
                            self.selected_pareto_model = Some(p.model_id.clone());
                        }
                        if p.free_offer_listed {
                            free_offer_badge(ui);
                        }
                    });
                    ui.horizontal(|ui| {
                        if let Some(delta) = latest_moves.get(&p.model_id) {
                            let (arrow, color) = if *delta > 0.0 {
                                ("▲", PRICE_UP)
                            } else {
                                ("▼", PRICE_DOWN)
                            };
                            ui.label(egui::RichText::new(arrow).size(9.0).color(color));
                        }
                        ui.label(format!("${:.4}", p.cost));
                    });
                    ui.label(format!("{:.1}", p.score));
                    ui.label(match self.pricing_source {
                        PricingSource::Catalog => "Catalog list".to_owned(),
                        _ if p.live_market => "Surplus market".to_owned(),
                        _ => "Surplus comparison/catalog".to_owned(),
                    });
                    ui.end_row();
                }
            });
    }
}

/// The selectable price-axis settings. Metric and cost basis pair up in one
/// selector because the estimated per-task basis only exists for the blended
/// metric — a standalone basis combo next to the metric combo mostly
/// offered impossible combinations.
const PRICE_AXES: [(PriceMetric, CostBasis); 4] = [
    (PriceMetric::Blended, CostBasis::PerMillion),
    (PriceMetric::Blended, CostBasis::EstimatedPerTask),
    (PriceMetric::Input, CostBasis::PerMillion),
    (PriceMetric::Output, CostBasis::PerMillion),
];

fn price_axis_label(axis: (PriceMetric, CostBasis)) -> &'static str {
    match axis {
        (PriceMetric::Blended, CostBasis::PerMillion) => "Blended · $/1M",
        (PriceMetric::Blended, CostBasis::EstimatedPerTask) => "Blended · est. $/task",
        (PriceMetric::Input, CostBasis::PerMillion) => "Input · $/1M",
        (PriceMetric::Output, CostBasis::PerMillion) => "Output · $/1M",
        _ => unreachable!("input/output metrics have no per-task basis"),
    }
}

/// Hover note for the benchmark selector: the fixed metric each raw board
/// reports. Composites disclose their method on the axis label instead, and
/// LiveBench chooses its metric from the Quality selector.
fn benchmark_metric_note(source: BenchmarkSource) -> Option<&'static str> {
    match source {
        BenchmarkSource::ArtificialAnalysisSnapshot => {
            Some("Metric: Intelligence Index (live + snapshot fallback)")
        }
        BenchmarkSource::SWERebench => Some("Metric: Result@1"),
        BenchmarkSource::TerminalBench3 => Some("Metric: accuracy"),
        BenchmarkSource::TerminalBench4 => Some("Metric: accuracy"),
        BenchmarkSource::DeepSWE11 => Some("Metric: pass@1"),
        BenchmarkSource::ReveloCodeIndex => Some("Metric: Code Index"),
        BenchmarkSource::DesignArena => Some("Metric: Design Elo (blinded votes)"),
        BenchmarkSource::FrontierCode => Some("Metric: mergeability score"),
        BenchmarkSource::AABriefcase => Some("Metric: Elo (blind pairwise, private tasks)"),
        BenchmarkSource::AutomationBenchAA => Some("Metric: partial score (no violations)"),
        BenchmarkSource::SWEBenchPro => Some("Metric: resolve rate (HARD set)"),
        BenchmarkSource::ValsSREBench => Some("Metric: accuracy (binary RE tasks)"),
        BenchmarkSource::CompositeAgentic
        | BenchmarkSource::CompositeDeployment
        | BenchmarkSource::LiveBench => None,
    }
}

/// Everything the hover tooltip needs beyond the orb itself, resolved once per
/// frame before the plot is built.
struct HoverContext<'a> {
    quote: Option<&'a Quote>,
    history: Option<&'a ModelSeries>,
    low_metric: PriceMetric,
    cost_basis: CostBasis,
    weights: (f64, f64, f64, f64),
    score_source: String,
    pricing_source: PricingSource,
}

/// Multi-line hover tooltip for a chart orb. The label formatter only gets a
/// plain string, so everything the user asked for — raw pricing legs, the
/// all-time low from the history log, liquidity, benchmark source — is folded
/// into one compact block. The first line is the model name; egui_plot bolds
/// nothing, so the name carries the structure on its own.
fn pareto_hover_label(p: &JoinedPoint, ctx: &HoverContext<'_>, score: f64) -> String {
    let mut lines = vec![p.model.clone()];

    // Headline: the plotted point.
    lines.push(format!(
        "{} {} · {} score {}",
        format_price_tick(p.cost),
        ctx.cost_basis.unit(),
        ctx.score_source,
        score
    ));

    // Raw per-leg pricing, exactly as the market quotes them.
    let mut legs = vec![format!("In ${:.4}", p.input)];
    if let Some(cache_read) = p.cache_read {
        legs.push(format!("Cache ${:.4}", cache_read));
    }
    legs.push(format!("Out ${:.4}", p.output));
    lines.push(legs.join(" · ") + " /1M");

    // On the per-task basis the benchmark's observed token volume is what
    // scales the blended rate; surface it so the $/task figure is auditable.
    if ctx.cost_basis == CostBasis::EstimatedPerTask
        && let Some(tokens) = p.tokens_per_task
    {
        let profile = p
            .token_profile
            .as_deref()
            .map(|profile| format!(" ({profile})"))
            .unwrap_or_default();
        lines.push(format!(
            "Workload: {} tokens/task{profile}",
            format_compact_number(tokens)
        ));
    }

    // All-time low from the persistent history log, on the metric that keeps
    // it comparable with the plotted price (blended when the chart shows
    // est. $/task, since that price is the blended rate scaled by token volume).
    // Catalog pricing skips it: the log records observed market legs, and
    // comparing them against a list price would overstate "% above low".
    if ctx.pricing_source != PricingSource::Catalog
        && let Some(series) = ctx.history
        && let Some((low, low_ts)) = historical_low(
            series,
            ctx.low_metric,
            ctx.weights.0,
            ctx.weights.1,
            ctx.weights.2,
            ctx.weights.3,
        )
    {
        let current = match ctx.low_metric {
            PriceMetric::Input => p.input,
            PriceMetric::Output => p.output,
            PriceMetric::Blended => blended_price(
                p.input,
                p.cache_read,
                p.output,
                ctx.weights.0,
                ctx.weights.1,
                ctx.weights.2,
                ctx.weights.3,
            ),
        };
        let pct = if low > 0.0 {
            format!(" · {:.0}% above", (current - low) / low * 100.0)
        } else {
            String::new()
        };
        lines.push(format!(
            "Lowest {}: {} · {}{}",
            ctx.low_metric.label().to_lowercase(),
            format_price_tick(low),
            format_short_date(low_ts),
            pct
        ));
    }

    if let Some(q) = ctx.quote {
        let mut market: Vec<String> = Vec::new();
        if let Some(count) = q.healthy_seller_count {
            market.push(format!("{count} healthy sellers"));
        }
        if let Some(requests) = q.requests_24h {
            market.push(format!("{}/24h", format_compact_number(requests as f64)));
        }
        if let Some(discount) = q.discount_pct {
            market.push(format!("{discount:.0}% off"));
        }
        if !market.is_empty() {
            lines.push(market.join(" · "));
        }
    }

    let mut badges: Vec<String> = Vec::new();
    if ctx.pricing_source == PricingSource::Catalog {
        badges.push("catalog list".into());
    }
    if !p.creator.is_empty() {
        badges.push(p.creator.clone());
    }
    if p.vision {
        badges.push("vision".into());
    }
    if p.uncensored {
        badges.push("uncensored".into());
    }
    if !badges.is_empty() {
        lines.push(badges.join(" · "));
    }

    lines.join("\n")
}

/// Compact date for the historical-low line: month + day, or month + year
/// when the low predates the current year.
fn format_short_date(ts: f64) -> String {
    let Some(dt) = DateTime::from_timestamp(ts as i64, 0) else {
        return String::new();
    };
    let now = chrono::Utc::now();
    if dt.format("%Y").to_string() == now.format("%Y").to_string() {
        dt.format("%d %b").to_string()
    } else {
        dt.format("%b %Y").to_string()
    }
}
