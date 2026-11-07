//! Pareto tab: the cost/score scatter chart, the selected-model detail
//! card, and the Pareto-efficient table.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::DateTime;
use eframe::egui;
use egui_plot::{HoverPosition, Line, Plot, PlotPoint, PlotPoints, Points, Text};

use crate::artificial_analysis_snapshot::ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE;
use crate::bench::scoring::format_percentile;
use crate::fetch::performance::ON_DEMAND_REFRESH_SECS;
use crate::format::{
    format_compact_number, format_compact_usd, format_latency_ms, format_price_tick,
    format_throughput,
};
use crate::history::track::{ModelSeries, blended_series, clip_series_to_window, historical_low};
use crate::model_id::group_label;
use crate::model_id::normalize;
use crate::pareto::{JoinedPoint, pareto_search_matches, price_from_plot_x, price_to_plot_x};
use crate::theme::{
    PRICE_DOWN, PRICE_UP, age_color, age_label, copyable_slug, creator_color, discount_color,
    free_offer_badge,
};
use crate::types::{
    ANY_MODEL, AgeFilter, AlertMode, BenchmarkMetric, BenchmarkSource, ComparisonMode, CostBasis,
    LiquidityFilter, ModalityFilter, PriceMetric, PricingSource, PricingWeights, Quote,
    UncensoredFilter, WORKLOAD_PRESETS, blended_price,
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
                    for source in BenchmarkSource::selectable_sources() {
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
                        for (metric, label) in BenchmarkMetric::CHOICES {
                            if ui
                                .selectable_value(&mut self.benchmark_metric, metric, label)
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
                    + usize::from(self.age_filter != AgeFilter::All)
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
                Some("Pricing source, comparison basis, market quality, modality, safety, release age, and price-axis options"),
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
                            if option_row(ui, &mut self.comparison_mode, &ComparisonMode::CHOICES) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Market quality").on_hover_text(
                                "Keep only models whose market clears a liquidity bar — a \
                                 trusted provider, or a floor on healthy sellers.",
                            );
                            if option_row(ui, &mut self.liquidity_filter, &LiquidityFilter::CHOICES) {
                                chart_axes_changed = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Pricing").on_hover_text(
                                "Which prices the chart bills: live-market asks with catalogue \
                                 list prices filling in for models that have no live ask, \
                                 live-market asks only, or every model at its published list \
                                 rates — official prices, ignoring marketplace discounts. \
                                 Free (0.00) catalogue listings are excluded from catalog \
                                 list mode.",
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
                            if option_row(ui, &mut self.modality_filter, &ModalityFilter::CHOICES) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Safety");
                            if option_row(ui, &mut self.uncensored_filter, &UncensoredFilter::CHOICES) {
                                chart_axes_changed = true;
                                reset_zoom = true;
                                self.selected_pareto_model = None;
                            }
                            ui.end_row();

                            ui.label("Age").on_hover_text(
                                "Keep only models released — or, when no release date is on \
                                 file, first listed on Surplus — within the window. Models \
                                 with no date at all pass only 'Any'.",
                            );
                            let mut age_changed = false;
                            for option in AgeFilter::ALL {
                                age_changed |= ui
                                    .selectable_value(
                                        &mut self.age_filter,
                                        option,
                                        option.label(),
                                    )
                                    .changed();
                            }
                            if age_changed {
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
                            for (label, (input, cache_read, output)) in WORKLOAD_PRESETS {
                                if ui.small_button(*label).clicked() {
                                    self.settings.input_weight = *input;
                                    self.settings.cache_read_weight = *cache_read;
                                    self.settings.output_weight = *output;
                                    self.settings_dirty = true;
                                }
                            }
                        });
                        if let Some(mix) = self.settings.workload_mix_label() {
                            ui.small(mix);
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
                    "{} has no matched rows with usable tokens-per-task telemetry for this comparison mode. Switch to $/1M or choose {}, {}, or a composite model with one of those profiles.",
                    // The two boards whose rows carry observed token volumes.
                    BenchmarkSource::DeepSWE11.short_label(),
                    BenchmarkSource::TerminalBench4.short_label(),
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
                if self.age_filter != AgeFilter::All {
                    ui.small(format!(
                        "'{}' age filter is active — models with no release or listing date are hidden.",
                        self.age_filter.label(),
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
        let weights = self.settings.pricing_weights();
        // Every orb is hoverable, so the tooltip formatter must be able to
        // resolve any series name back to its model: plain orbs are named
        // after their model id (unique even when two slugs share a display
        // name — egui_plot derives hover ids from the name), while the
        // selection/search halos carry a `*_ring_{model_id}` prefix.
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
            BenchmarkSource::TerminalBench4 => "Terminal-Bench 4.0 accuracy".to_owned(),
            BenchmarkSource::DeepSWE11 => "DeepSWE v1.1 pass@1".to_owned(),
            BenchmarkSource::LiveBench => format!("LiveBench {} score", self.benchmark_metric.label()),
            BenchmarkSource::DesignArena => "Design Arena Elo".to_owned(),
            BenchmarkSource::FrontierCode => "FrontierCode 1.1 mergeability score".to_owned(),
            BenchmarkSource::FrontierCode2 => "FrontierCode 2.0 mergeability score".to_owned(),
            BenchmarkSource::AABriefcase => "AA-Briefcase Elo (blind pairwise)".to_owned(),
            BenchmarkSource::AutomationBenchAA => {
                "AutomationBench-AA partial score".to_owned()
            }
            BenchmarkSource::AA3D => "AA-3D Elo (three.js scenes, blind votes)".to_owned(),
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
                    let model_id = hover_series_model_id(plot_name);
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
                        history: history.series(model_id),
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
                    // Named by the unique model id: a display name can be
                    // shared by two slugs, and egui_plot derives each item's
                    // hover id from the name.
                    plot_ui.points(Points::new(p.model_id.clone(), pts).radius(radius).color(color));
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
            self.spawn_pinned_widget(ui.ctx(), &model_id, at);
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
            if self.age_filter != AgeFilter::All {
                active_filters.push(format!("age {}", self.age_filter.label()));
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
            let card_width = (ui.available_width() * 0.5).clamp(380.0, 680.0);
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

        // Age provenance: the true release date when the bundled table knows
        // one, the marketplace listing date otherwise. One color drives the
        // accent bar, the age chip, and the date line.
        let now = chrono::Utc::now().timestamp();
        let age = p
            .released_at
            .map(|ts| (ts, true))
            .or_else(|| p.listed_at.map(|ts| (ts, false)));
        let age_days = age.map(|(ts, _)| (((now - ts) as f64) / 86_400.0).max(0.0).floor());
        let age_color = age_days.map(age_color);
        // Live-market discount: the same workload billed at catalog list
        // rates under the current metric and cost basis. Some only when the
        // plotted price is genuinely below list, so the hero band shows a
        // real saving instead of a 0% footnote. Catalog-priced quotes (and
        // comparison rows with no list of their own) bill their own legs and
        // drop out on their own.
        let list_discount = selected_quote.as_ref().and_then(|quote| {
            let per_million = quote.price_for(
                PricingSource::Catalog,
                self.price_metric,
                self.settings.pricing_weights(),
            );
            let list_cost = match self.cost_basis {
                CostBasis::PerMillion => per_million,
                CostBasis::EstimatedPerTask => p.tokens_per_task? * per_million / 1_000_000.0,
            };
            if !list_cost.is_finite() || list_cost <= 0.0 || p.cost >= list_cost {
                return None;
            }
            let discount_pct = (1.0 - p.cost / list_cost) * 100.0;
            if !discount_pct.is_finite() || discount_pct < 0.5 {
                return None;
            }
            let mut hover = format!(
                "The same workload at catalog list rates: {} {} \u{2014} the plotted price \
                 is {discount_pct:.1}% below list. Tracks the current pricing source, \
                 metric, and workload mix.",
                format_price_tick(list_cost),
                self.cost_basis.unit(),
            );
            // A zero-priced live cache ask collapses the blend under
            // cache-heavy weights; say so when the discount is that extreme.
            if p.cache_read == Some(0.0) {
                hover.push_str(
                    " This market's live cache-read ask is $0, which deepens the \
                     blended discount under cache-heavy workloads.",
                );
            }
            Some((list_cost, discount_pct, hover))
        });

        let card = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.vertical(|ui| {
                // Header: creator dot, name, vision eye, uncensored lock on
                // the left — the eye is the only modality mark, so its
                // absence means text-only, matching the models browser rows —
                // and the selection actions riding the right edge.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("\u{25cf}")
                            .color(creator_color(&p.creator))
                            .size(16.0),
                    );
                    ui.heading(&p.model);
                    if p.vision {
                        ui.label(
                            egui::RichText::new("\u{1f441}")
                                .size(14.0)
                                .color(egui::Color32::from_rgb(120, 200, 255)),
                        )
                        .on_hover_text("Accepts image input (vision LLM)");
                    }
                    if p.uncensored {
                        ui.label(
                            egui::RichText::new("\u{1f513}")
                                .size(14.0)
                                .color(egui::Color32::from_rgb(240, 170, 60)),
                        )
                        .on_hover_text(
                            "Uncensored \u{2014} refusal behavior removed \
                             (abliterated/uncensored finetune)",
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("\u{2715}")
                            .on_hover_text("Clear the selection")
                            .clicked()
                        {
                            self.selected_pareto_model = None;
                        }
                        ui.menu_button(egui::RichText::new("\u{1f514}").size(16.0), |ui| {
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
                        // Frontier visibility: hidden models keep a faded orb
                        // but leave the frontier line and its entry/exit
                        // alerts. Glyph-only button — the hover names the
                        // action and its consequences.
                        let hidden = self
                            .settings
                            .hidden_frontier_models
                            .iter()
                            .any(|m| m == &p.model_id);
                        let (glyph, action) = if hidden {
                            ("\u{2295}", "Show on frontier")
                        } else {
                            ("\u{2296}", "Hide from frontier")
                        };
                        if ui
                            .button(glyph)
                            .on_hover_text(format!(
                                "{action} \u{2014} hidden models stay on the chart (faded); \
                                 the frontier line and frontier entry/exit alerts skip them"
                            ))
                            .clicked()
                        {
                            self.toggle_frontier_hidden(&p.model_id);
                        }
                    });
                });
                // Slug on the left, age chip on the right: the identifier to
                // copy shares its line with the freshness glance.
                ui.horizontal(|ui| {
                    copyable_slug(ui, &p.model_id);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let (Some(days), Some(color)) = (age_days, age_color)
                            && let Some((ts, is_release)) = age
                        {
                            let date_text = DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%d").to_string())
                                .unwrap_or_default();
                            let chip_hover = if is_release {
                                format!(
                                    "Released {date_text} \u{2014} age from the bundled \
                                     release-date table; color marks freshness"
                                )
                            } else {
                                format!(
                                    "Listed on Surplus {date_text} \u{2014} no release date \
                                     on file for this model, so its age runs from the \
                                     marketplace listing"
                                )
                            };
                            pill(ui, &age_label(days), color, Some(&chip_hover));
                        }
                    });
                });
                // Hero band: the plotted price is the headline; the score is
                // its counterpart on the right.
                ui.add_space(2.0);
                let score_label = match self.benchmark_source {
                    BenchmarkSource::LiveBench => {
                        format!("LiveBench {}", self.benchmark_metric.label())
                    }
                    source => source.short_label().to_owned(),
                };
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // Same magnitude-aware formatting as the axis
                            // ticks and hover: a fixed 4 decimals renders
                            // cheap models as "$0.0000".
                            ui.label(
                                egui::RichText::new(format_price_tick(p.cost))
                                    .size(24.0)
                                    .strong(),
                            );
                            ui.label(egui::RichText::new(self.cost_basis.unit()).size(11.5).weak())
                                .on_hover_text(
                                    "Plotted price under the current pricing source, metric, and \
                                     workload mix",
                                );
                            // Live-market discount: the list-rate counterpart
                            // of the plotted price, struck through, with the
                            // magnitude-colored saving. A listed free ask
                            // displaces the headline price, so its badge
                            // qualifies the number it sits next to.
                            if let Some((list_cost, discount_pct, hover)) = &list_discount {
                                ui.label(
                                    egui::RichText::new(format_price_tick(*list_cost))
                                        .size(12.5)
                                        .weak()
                                        .strikethrough(),
                                )
                                .on_hover_text(hover);
                                // One decimal near 100%: a 99.6% saving must
                                // not round into a "\u{2212}100%" that reads
                                // as free.
                                let pct = if *discount_pct >= 99.5 {
                                    format!("\u{2212}{discount_pct:.1}%")
                                } else {
                                    format!("\u{2212}{discount_pct:.0}%")
                                };
                                ui.label(
                                    egui::RichText::new(pct)
                                        .size(12.5)
                                        .color(discount_color(*discount_pct)),
                                )
                                .on_hover_text(hover);
                            }
                            if p.free_offer_listed {
                                free_offer_badge(ui);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:.1}", p.score))
                                        .size(17.0)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new(&score_label).size(10.5).weak());
                            });
                        });
                    });
                ui.add_space(3.0);

                // Speed provenance: AA's standardized medians lead; the
                // live-market buckets follow (daggered) where AA has no read.
                let aa_entry = selected_quote.as_ref().and_then(|quote| {
                    self.aa_speed_lookup()
                        .and_then(|join| join.get(&quote.model).cloned())
                });
                let aa_tps = aa_entry.as_ref().and_then(|e| e.median_tps);
                let aa_ttft = aa_entry.as_ref().and_then(|e| e.median_ttft_s);
                let mkt_tps = selected_quote.as_ref().and_then(|q| q.throughput_p50);
                let mkt_ttft = selected_quote.as_ref().and_then(|q| q.ttft_p50_ms);
                let uptime = selected_quote.as_ref().and_then(|q| q.uptime_pct);

                let (speed_value, speed_hover) = if let Some(tps) = aa_tps {
                    let band = match (
                        aa_entry.as_ref().and_then(|e| e.tps_p05),
                        aa_entry.as_ref().and_then(|e| e.tps_p95),
                    ) {
                        (Some(lo), Some(hi)) => format!(
                            " p05\u{2013}p95 {}\u{2013}{}",
                            format_throughput(lo),
                            format_throughput(hi)
                        ),
                        _ => String::new(),
                    };
                    let effort = aa_entry
                        .as_ref()
                        .and_then(|e| e.effort.as_deref())
                        .map(|e| format!(" ({e} variant)"))
                        .unwrap_or_default();
                    (
                        format_throughput(tps),
                        Some(format!(
                            "Median of AA's controlled generations under a uniform protocol{effort} \
                             \u{2014} independent of any marketplace route.{band}"
                        )),
                    )
                } else if let Some(tps) = mkt_tps {
                    (
                        format!("{}\u{2020}", format_throughput(tps)),
                        Some(
                            "Live-market median pooled across every seller over the last 24h \
                             \u{2014} the feed publishes only coarse histogram midpoints, so treat \
                             it as approximate"
                                .to_owned(),
                        ),
                    )
                } else {
                    ("\u{2014}".to_owned(), None)
                };
                let (ttft_value, ttft_hover) = if let Some(ttft) = aa_ttft {
                    (
                        format_latency_ms(ttft * 1000.0),
                        Some(
                            "Median time-to-first-token of AA's controlled generations under a \
                             uniform protocol"
                                .to_owned(),
                        ),
                    )
                } else if let Some(ms) = mkt_ttft {
                    (
                        format!("{}\u{2020}", format_latency_ms(ms)),
                        Some(
                            "Live-market median TTFT over the last 24h \u{2014} coarse feed \
                             buckets, treat as approximate"
                                .to_owned(),
                        ),
                    )
                } else {
                    ("\u{2014}".to_owned(), None)
                };

                // The stat grid: pricing legs on one row, telemetry on the
                // next, and the benchmark workload when the per-task basis
                // needs it for the hero price to stay auditable. Each cell
                // claims a third of the card so values never cramp onto a
                // second line when there is width to spare.
                let stat_w = ((ui.available_width() - 36.0) / 3.0).max(90.0);
                egui::Grid::new("pareto_card_stats")
                    .num_columns(3)
                    .spacing([18.0, 10.0])
                    .show(ui, |ui| {
                        stat_cell(
                            ui,
                            "Input",
                            format!("${:.4}", p.input),
                            "/1M",
                            Some("Per-1M ask billed under the current pricing source".to_owned()),
                            stat_w,
                        );
                        stat_cell(
                            ui,
                            "Cache read",
                            match p.cache_read {
                                Some(rate) => format!("${rate:.4}"),
                                None => "\u{2014}".to_owned(),
                            },
                            "",
                            match p.cache_read {
                                Some(_) => Some(
                                    "Resolved cache-read rate: provider ask, market best, or \
                                     catalogue list under the current pricing source"
                                        .to_owned(),
                                ),
                                None => Some(
                                    "No cache-read ask published anywhere in this market; blended \
                                     pricing assumes a share of the input price"
                                        .to_owned(),
                                ),
                            },
                            stat_w,
                        );
                        stat_cell(
                            ui,
                            "Output",
                            format!("${:.4}", p.output),
                            "/1M",
                            Some("Per-1M ask billed under the current pricing source".to_owned()),
                            stat_w,
                        );
                        ui.end_row();
                        stat_cell(ui, "Speed", speed_value, "", speed_hover, stat_w);
                        stat_cell(ui, "TTFT", ttft_value, "", ttft_hover, stat_w);
                        stat_cell(
                            ui,
                            "Uptime",
                            match uptime {
                                Some(pct) => format!("{pct:.1}%"),
                                None => "\u{2014}".to_owned(),
                            },
                            "",
                            uptime.map(|_| {
                                "Share of requests across the whole market that did not fail, over \
                                 the last 24h"
                                    .to_owned()
                            }),
                            stat_w,
                        );
                        ui.end_row();
                        if self.cost_basis == CostBasis::EstimatedPerTask
                            && let Some(tokens) = p.tokens_per_task
                        {
                            let profile_hover = p.token_profile.clone().unwrap_or_else(|| {
                                "Benchmark-reported token volume per task".to_owned()
                            });
                            stat_cell(
                                ui,
                                "Workload",
                                format_compact_number(tokens),
                                "/task",
                                Some(profile_hover),
                                stat_w,
                            );
                            ui.label("");
                            ui.label("");
                            ui.end_row();
                        }
                    });
                let market_fallback = (aa_tps.is_none() && mkt_tps.is_some())
                    || (aa_ttft.is_none() && mkt_ttft.is_some());
                let no_speed = aa_tps.is_none()
                    && aa_ttft.is_none()
                    && mkt_tps.is_none()
                    && mkt_ttft.is_none()
                    && uptime.is_none();
                if market_fallback {
                    ui.small("\u{2020} market buckets \u{00b7} 24h").on_hover_text(
                        "Live-market medians pooled across every seller \u{2014} the feed \
                         publishes only coarse histogram midpoints, so treat them as approximate",
                    );
                } else if no_speed {
                    ui.label(egui::RichText::new("no speed data").weak());
                }
                if self.cost_basis == CostBasis::EstimatedPerTask && p.tokens_per_task.is_some() {
                    ui.small("est. $/task = workload \u{00d7} current Surplus blended rate");
                }

                // Provenance footer: the age basis, then who made, sells, and
                // benchmarked this row.
                ui.horizontal_wrapped(|ui| {
                    let mut shown = false;
                    if let Some((ts, is_release)) = age {
                        let date_text = DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.format("%Y-%m-%d").to_string())
                            .unwrap_or_default();
                        let (basis, hover): (&str, String) = if is_release {
                            (
                                "Released",
                                "True release date from the app's bundled release-date table"
                                    .to_owned(),
                            )
                        } else {
                            (
                                "Listed on Surplus",
                                "No release date on file for this model \u{2014} showing the day \
                                 it first appeared on the Surplus marketplace"
                                    .to_owned(),
                            )
                        };
                        ui.small(format!("{basis} {date_text}"))
                            .on_hover_text(hover);
                        shown = true;
                    }
                    if !p.creator.is_empty() {
                        if shown {
                            ui.separator();
                        }
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
                    // Composite row names carry the full method disclosure
                    // and the source-by-source grid below repeats it; skip
                    // the duplicate.
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
                    // Selecting the card is the on-demand trigger for this
                    // model's per-provider breakdown.
                    self.ensure_performance(&quote.model, ON_DEMAND_REFRESH_SECS);
                    self.market_quality_section(ui, quote);
                }

                // Price history: the recorded blended price over the
                // configured window (24h by default, change it in Settings),
                // as a small always-on sparkline that expands to a full
                // log-scale plot.
                let weights = self.settings.pricing_weights();
                let history_window = self.settings.history_window;
                if let Some(series) = self.history.series(&p.model_id) {
                    let all_points = blended_series(series, weights);
                    let now_ts = chrono::Utc::now().timestamp() as f64;
                    let cutoff = history_window.secs().map(|s| now_ts - s);
                    let points = clip_series_to_window(&all_points, cutoff);
                    if !points.is_empty() {
                        // Extend the last recorded price to the right edge so
                        // the line reads as "held to now", not "ended".
                        let mut step: Vec<(f64, f64)> = Vec::with_capacity(points.len() * 2 + 1);
                        let mut prev: Option<(f64, f64)> = None;
                        for &(ts, price) in &points {
                            if let Some((pts, pprice)) = prev {
                                step.push((pts, pprice));
                                step.push((ts, pprice));
                            }
                            prev = Some((ts, price));
                        }
                        if let Some((pts, pprice)) = prev {
                            step.push((pts, pprice));
                            step.push((now_ts, pprice));
                        }
                        let (lo, hi) = points.iter().fold(
                            (f64::MAX, f64::MIN),
                            |(lo, hi), &(_, v)| (lo.min(v), hi.max(v)),
                        );
                        let (low_ts, low_price) = points
                            .iter()
                            .copied()
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .unwrap_or((0.0, 0.0));
                        let expand_id =
                            egui::Id::new("pw_price_history_expanded").with(p.model_id.as_str());
                        let mut expanded = ui
                            .ctx()
                            .data_mut(|d| d.get_temp::<bool>(expand_id).unwrap_or(false));
                        ui.horizontal(|ui| {
                            ui.strong("Price history");
                            ui.label(
                                egui::RichText::new(history_window.label())
                                    .size(10.5)
                                    .weak(),
                            )
                            .on_hover_text(
                                "How far back this graph reaches — change it in Settings \
                                 \u{2192} Price history",
                            );
                            ui.small(format!(
                                "low {} on {} \u{00b7} now {}",
                                format_price_tick(low_price),
                                format_short_date(low_ts),
                                format_price_tick(points[points.len() - 1].1),
                            ))
                            .on_hover_text(format!(
                                "Blended $/1M at the current workload mix over the last {} \
                                 \u{2014} the history log itself records everything regardless",
                                history_window.label(),
                            ));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if expanded { "Collapse" } else { "Expand" };
                                if ui.small_button(label).clicked() {
                                    expanded = !expanded;
                                    ui.ctx().data_mut(|d| d.insert_temp(expand_id, expanded));
                                }
                            });
                        });
                        if expanded {
                            let line: PlotPoints = step
                                .iter()
                                .map(|&(ts, price)| [ts, price_to_plot_x(price, true)])
                                .collect();
                            Plot::new("pw_price_history_plot")
                                .height(220.0)
                                .x_axis_formatter(|mark, _range| {
                                    DateTime::from_timestamp(mark.value as i64, 0)
                                        .map(|dt| dt.format("%d %b %H:%M").to_string())
                                        .unwrap_or_default()
                                })
                                .y_axis_formatter(|mark, _range| {
                                    format_price_tick(10_f64.powf(mark.value))
                                })
                                .allow_zoom(true)
                                .allow_drag(false)
                                .allow_scroll(false)
                                .show(ui, |plot_ui| {
                                    plot_ui.line(
                                        Line::new("blended", line)
                                            .width(1.5)
                                            .color(egui::Color32::from_rgb(120, 180, 255))
                                            .allow_hover(false),
                                    );
                                    plot_ui.points(
                                        Points::new(
                                            "low",
                                            PlotPoints::from(vec![[
                                                low_ts,
                                                price_to_plot_x(low_price, true),
                                            ]]),
                                        )
                                        .radius(3.5)
                                        .color(PRICE_DOWN)
                                        // Decoration like the line above: hover
                                        // would show a bare "low" tooltip instead
                                        // of the card's own price readouts.
                                        .allow_hover(false),
                                    );
                                });
                        } else {
                            // Sparkline: a step line (prices change in jumps)
                            // with a soft halo, linear over the observed range.
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 40.0),
                                egui::Sense::hover(),
                            );
                            let span = (step[step.len() - 1].0 - step[0].0).max(1.0);
                            let range = (hi - lo).max(1e-9);
                            let mut line_pts: Vec<egui::Pos2> = Vec::with_capacity(step.len());
                            for &(ts, price) in &step {
                                let x =
                                    rect.left() + ((ts - step[0].0) / span) as f32 * rect.width();
                                let y =
                                    rect.bottom() - ((price - lo) / range) as f32 * rect.height();
                                let pos = egui::pos2(x, y);
                                if let Some(last) = line_pts.last() {
                                    line_pts.push(egui::pos2(pos.x, last.y));
                                }
                                line_pts.push(pos);
                            }
                            ui.painter().add(egui::Shape::line(
                                line_pts.clone(),
                                egui::Stroke::new(
                                    7.0,
                                    egui::Color32::from_rgba_unmultiplied(120, 180, 255, 26),
                                ),
                            ));
                            ui.painter().add(egui::Shape::line(
                                line_pts,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 180, 255)),
                            ));
                            response.on_hover_text(format!(
                                "Blended {} \u{2013} {} \u{00b7} low {} on {}",
                                format_price_tick(lo),
                                format_price_tick(hi),
                                format_price_tick(low_price),
                                format_short_date(low_ts),
                            ));
                        }
                    } else {
                        ui.small("no price history yet (recording since first launch)");
                    }
                }

                if let Some(consensus) = &consensus {
                    ui.collapsing("Source-by-source percentile", |ui| {
                        // Bounded and contained: the grid scrolls inside the card on
                        // both axes, so long benchmark names and deep histories can
                        // never stretch the card wider or push the frontier table
                        // out of reach.
                        egui::ScrollArea::both()
                            .max_height(320.0)
                            .auto_shrink([false, true])
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
        });
        // Age accent bar over the card's left edge: the same color the chip
        // uses, so the selection reads as "this model and its freshness" from
        // across the chart.
        if let Some(color) = age_color {
            let rect = card.response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(0.0, 1.0),
                    egui::pos2(rect.left() + 3.0, rect.bottom() - 1.0),
                ),
                egui::CornerRadius::same(2),
                color,
            );
        }
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
                egui::ScrollArea::both()
                    .id_salt(format!("mq_scroll_{}", quote.model))
                    .max_height(280.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
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
                            option.workload_price(self.settings.pricing_weights())
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
                        if let Some((arrow, color)) = latest_moves
                            .get(&p.model_id)
                            .copied()
                            .and_then(price_move_arrow)
                        {
                            ui.label(egui::RichText::new(arrow).size(9.0).color(color));
                        }
                        ui.label(format_price_tick(p.cost));
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
        // Both fields are restored from ui_state.json with only a version
        // check, so a stale or hand-edited file can carry a pairing the
        // selector never offers. The chart can't plot it, but degrading to
        // the metric's plain label beats panicking on the first frame.
        _ => axis.0.label(),
    }
}

/// Series name → model id for the Pareto plot's hover resolution. Orbs are
/// named by their model id — unique even when two slugs share a display
/// name, so egui_plot's name-derived hover ids can't collide — while the
/// selection/search halos carry a `*_ring_{model_id}` prefix.
fn hover_series_model_id(plot_name: &str) -> &str {
    plot_name
        .strip_prefix("sel_ring_")
        .or_else(|| plot_name.strip_prefix("search_ring_"))
        .unwrap_or(plot_name)
}

/// Arrow + color for a frontier row's latest blended-price move. An exact
/// zero is reachable (any-leg-changed moves whose blended contribution nets
/// out under the current workload mix) and is neither a rise nor a drop, so
/// it gets no direction glyph at all.
fn price_move_arrow(delta: f64) -> Option<(&'static str, egui::Color32)> {
    if delta > 0.0 {
        Some(("▲", PRICE_UP))
    } else if delta < 0.0 {
        Some(("▼", PRICE_DOWN))
    } else {
        None
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
        BenchmarkSource::TerminalBench4 => Some("Metric: accuracy"),
        BenchmarkSource::DeepSWE11 => Some("Metric: pass@1"),
        BenchmarkSource::DesignArena => Some("Metric: Design Elo (blinded votes)"),
        BenchmarkSource::FrontierCode => Some("Metric: mergeability score"),
        BenchmarkSource::FrontierCode2 => Some("Metric: mergeability score"),
        BenchmarkSource::AABriefcase => Some("Metric: Elo (blind pairwise, private tasks)"),
        BenchmarkSource::AutomationBenchAA => Some("Metric: partial score (no violations)"),
        BenchmarkSource::AA3D => Some("Metric: Elo (blind votes, 3D scene building)"),
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
    weights: PricingWeights,
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
        && let Some((low, low_ts)) = historical_low(series, ctx.low_metric, ctx.weights)
    {
        let current = match ctx.low_metric {
            PriceMetric::Input => p.input,
            PriceMetric::Output => p.output,
            PriceMetric::Blended => blended_price(
                p.input,
                p.cache_read,
                p.output,
                ctx.weights.input,
                ctx.weights.cache_read,
                ctx.weights.output,
                ctx.weights.fallback_pct,
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

/// A small rounded pill with colored text on a translucent fill of the same
/// hue — the detail card's badge primitive (age chip, modality, pricing
/// source, frontier rank). Hover carries the disclosure, keeping the pill
/// itself to a glanceable word or two.
fn pill(
    ui: &mut egui::Ui,
    text: &str,
    color: egui::Color32,
    hover: Option<&str>,
) -> egui::Response {
    let response = egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.55)))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.set_min_height(14.0);
            ui.label(egui::RichText::new(text).size(11.0).color(color))
        })
        .response;
    match hover {
        Some(text) => response.on_hover_text(text),
        None => response,
    }
}

/// One labeled statistic in the detail card's stat grid: a tiny uppercase
/// caption over a value (with an optional muted unit), hovering to disclose
/// provenance. `min_width` stretches the cell to its grid share so values
/// use the card's full width instead of hugging their content.
fn stat_cell(
    ui: &mut egui::Ui,
    caption: &str,
    value: String,
    unit: &str,
    hover: Option<String>,
    min_width: f32,
) {
    let cell = ui
        .vertical(|ui| {
            ui.set_min_width(min_width);
            ui.label(egui::RichText::new(caption.to_uppercase()).size(9.5).weak());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(value).size(14.0));
                if !unit.is_empty() {
                    ui.label(egui::RichText::new(unit).size(11.0).weak());
                }
            });
        })
        .response;
    if let Some(hover) = hover {
        cell.on_hover_text(hover);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_series_names_resolve_to_model_ids() {
        // Orbs are named by their unique model id, so two slugs sharing a
        // display name still get distinct hover targets and each resolves to
        // its own tooltip data.
        assert_eq!(hover_series_model_id("gpt-6-sol"), "gpt-6-sol");
        assert_eq!(hover_series_model_id("claude-opus-5.5"), "claude-opus-5.5");
        assert_eq!(hover_series_model_id("sel_ring_gpt-6-sol"), "gpt-6-sol");
        assert_eq!(
            hover_series_model_id("search_ring_muse-wonder"),
            "muse-wonder"
        );
    }

    #[test]
    fn price_move_arrow_skips_exact_zero_deltas() {
        assert_eq!(price_move_arrow(0.5), Some(("▲", PRICE_UP)));
        assert_eq!(price_move_arrow(-0.5), Some(("▼", PRICE_DOWN)));
        // A move whose legs cancel out under the workload mix is neither a
        // rise nor a drop — no direction glyph.
        assert_eq!(price_move_arrow(0.0), None);
    }

    #[test]
    fn price_axis_label_tolerates_impossible_metric_basis_pairs() {
        // The pairs the selector offers.
        assert_eq!(
            price_axis_label((PriceMetric::Blended, CostBasis::PerMillion)),
            "Blended · $/1M"
        );
        assert_eq!(
            price_axis_label((PriceMetric::Blended, CostBasis::EstimatedPerTask)),
            "Blended · est. $/task"
        );
        assert_eq!(
            price_axis_label((PriceMetric::Input, CostBasis::PerMillion)),
            "Input · $/1M"
        );
        assert_eq!(
            price_axis_label((PriceMetric::Output, CostBasis::PerMillion)),
            "Output · $/1M"
        );
        // A stale or hand-edited ui_state.json can carry the rest; it must
        // degrade to a plain label, not panic on the first Pareto frame.
        assert_eq!(
            price_axis_label((PriceMetric::Input, CostBasis::EstimatedPerTask)),
            "Input"
        );
        assert_eq!(
            price_axis_label((PriceMetric::Output, CostBasis::EstimatedPerTask)),
            "Output"
        );
    }
}
