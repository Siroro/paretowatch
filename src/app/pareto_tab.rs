//! Pareto tab: the cost/score scatter chart, the selected-model detail
//! card, and the Pareto-efficient table.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::DateTime;
use eframe::egui;
use egui_plot::{HoverPosition, Line, Plot, PlotPoint, PlotPoints, Points, Text};

use crate::artificial_analysis_snapshot::ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE;
use crate::bench::{format_percentile, normalize};
use crate::format::{format_compact_number, format_price_tick};
use crate::history::track::{ModelSeries, historical_low};
use crate::pareto::{JoinedPoint, pareto_search_matches, price_from_plot_x, price_to_plot_x};
use crate::theme::{
    PRICE_DOWN, PRICE_UP, creator_color, discount_color, free_offer_badge, group_label,
};
use crate::types::{
    ANY_MODEL, AlertMode, BenchmarkMetric, BenchmarkSource, ComparisonMode, CostBasis,
    LiquidityFilter, ModalityFilter, PriceMetric, Quote, blended_price,
};
use crate::worker::WorkerCommand;

use super::{ParetoWatchApp, Tab};

impl ParetoWatchApp {
    pub(super) fn pareto_tab(&mut self, ui: &mut egui::Ui) {
        let mut chart_axes_changed = false;
        let scaffold_options = self.cached_scaffolds();
        ui.horizontal_wrapped(|ui| {
            ui.label("Price:");
            egui::ComboBox::from_id_salt("price_metric")
                .selected_text(self.price_metric.label())
                .show_ui(ui, |ui| {
                    for metric in [
                        PriceMetric::Blended,
                        PriceMetric::Input,
                        PriceMetric::Output,
                    ] {
                        if ui
                            .selectable_value(&mut self.price_metric, metric, metric.label())
                            .changed()
                        {
                            if self.price_metric != PriceMetric::Blended {
                                self.cost_basis = CostBasis::PerMillion;
                            }
                            chart_axes_changed = true;
                        }
                    }
                });

            if self.price_metric == PriceMetric::Blended {
                ui.separator();
                ui.label("Cost basis:");
                egui::ComboBox::from_id_salt("cost_basis")
                    .selected_text(self.cost_basis.label())
                    .show_ui(ui, |ui| {
                        for basis in [CostBasis::PerMillion, CostBasis::EstimatedPerTask] {
                            if ui
                                .selectable_value(&mut self.cost_basis, basis, basis.label())
                                .changed()
                            {
                                chart_axes_changed = true;
                                self.selected_pareto_model = None;
                            }
                        }
                    });
            }

            ui.separator();
            ui.label("Benchmark:");
            egui::ComboBox::from_id_salt("benchmark_source")
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

            ui.separator();
            if self.benchmark_source == BenchmarkSource::LiveBench {
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
            } else if !self.benchmark_source.is_composite() {
                // Composite row names already disclose their method; a separate
                // metric note would just repeat it in the toolbar.
                ui.label(match self.benchmark_source {
                    BenchmarkSource::ArtificialAnalysisSnapshot => {
                        "Metric: Intelligence Index snapshot"
                    }
                    BenchmarkSource::SWERebench => "Metric: Result@1",
                    BenchmarkSource::TerminalBench3 => "Metric: accuracy",
                    BenchmarkSource::TerminalBench4 => "Metric: accuracy",
                    BenchmarkSource::DeepSWE11 => "Metric: pass@1",
                    BenchmarkSource::ReveloCodeIndex => "Metric: Code Index",
                    BenchmarkSource::DesignArena => "Metric: Design Elo (blinded votes)",
                    _ => unreachable!(),
                });
            }

            if self.price_metric == PriceMetric::Blended {
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
                if self.settings_dirty && ui.button("Apply weights").clicked() {
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
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs_f64(
                                (APPLIED_SECS - age).max(0.05),
                            ));
                    }
                }
            }
        });

        if self.cost_basis == CostBasis::EstimatedPerTask {
            ui.small("Est. $/task = benchmark-observed tokens/task × current Surplus blended $/1M. Benchmark-reported historical dollar costs are never used.");
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Comparison:");
            egui::ComboBox::from_id_salt("comparison_mode")
                .selected_text(self.comparison_mode.label())
                .show_ui(ui, |ui| {
                    for mode in [
                        ComparisonMode::ModelCapability,
                        ComparisonMode::BestAvailableAgent,
                        ComparisonMode::CommonScaffold,
                    ] {
                        if ui
                            .selectable_value(&mut self.comparison_mode, mode, mode.label())
                            .changed()
                        {
                            chart_axes_changed = true;
                            self.selected_pareto_model = None;
                        }
                    }
                });
            if self.comparison_mode == ComparisonMode::CommonScaffold {
                ui.label("Scaffold:");
                egui::ComboBox::from_id_salt("common_scaffold")
                    .width(180.0)
                    .selected_text(&self.common_scaffold)
                    .show_ui(ui, |ui| {
                        for scaffold in &scaffold_options {
                            if ui
                                .selectable_value(
                                    &mut self.common_scaffold,
                                    scaffold.clone(),
                                    scaffold,
                                )
                                .changed()
                            {
                                chart_axes_changed = true;
                                self.selected_pareto_model = None;
                            }
                        }
                    });
            }
            ui.separator();
            ui.label("Market quality:");
            egui::ComboBox::from_id_salt("liquidity_filter")
                .selected_text(self.liquidity_filter.label())
                .show_ui(ui, |ui| {
                    for filter in [
                        LiquidityFilter::Any,
                        LiquidityFilter::Trusted,
                        LiquidityFilter::Healthy3,
                        LiquidityFilter::Healthy10,
                    ] {
                        if ui
                            .selectable_value(&mut self.liquidity_filter, filter, filter.label())
                            .changed()
                        {
                            chart_axes_changed = true;
                            self.selected_pareto_model = None;
                        }
                    }
                });
        });

        // Chart controls: modality filter (e.g. vision-only), search highlight,
        // and zoom/selection toggles — pinned above the scrolling chart body.
        ui.add_space(4.0);
        let mut reset_zoom = chart_axes_changed;
        ui.horizontal_wrapped(|ui| {
            ui.label("Show:");
            egui::ComboBox::from_id_salt("modality_filter")
                .selected_text(self.modality_filter.label())
                .show_ui(ui, |ui| {
                    for filter in [
                        ModalityFilter::All,
                        ModalityFilter::Vision,
                        ModalityFilter::TextOnly,
                    ] {
                        if ui
                            .selectable_value(&mut self.modality_filter, filter, filter.label())
                            .changed()
                        {
                            chart_axes_changed = true;
                            reset_zoom = true;
                            self.selected_pareto_model = None;
                        }
                    }
                });
            ui.separator();
            ui.label("Search:");
            let search_edit = egui::TextEdit::singleline(&mut self.pareto_search)
                .hint_text("highlight models, creators…")
                .desired_width(220.0)
                .clip_text(true);
            ui.add(search_edit);
            if !self.pareto_search.trim().is_empty() && ui.small_button("✗ Clear search").clicked()
            {
                self.pareto_search.clear();
            }
            ui.separator();
            if ui.small_button("Reset zoom").clicked() {
                reset_zoom = true;
            }
            if ui
                .checkbox(&mut self.log_price_axis, "Log price axis")
                .changed()
            {
                reset_zoom = true;
            }
            if self.selected_pareto_model.is_some()
                && ui.small_button("✗ Clear selection").clicked()
            {
                self.selected_pareto_model = None;
            }
        });
        ui.add_space(2.0);
        ui.small("Wheel: zoom · middle-drag: pan · right-drag: box zoom · double-click: reset · click an orb: inspect · right-click an orb: add to price window");

        // Resolved before the controls so the legend can toggle groups without
        // borrowing `self` inside the scroll closure; the call is cached, so
        // the chart body below sees the same view.
        let view = self.ensure_pareto_view();

        // Clickable colour legend for the model groups (creators) in the
        // current join. Clicking a chip hides that group's orbs; clicking a
        // disabled chip brings it back. Hidden chips stay listed (dimmed,
        // struck through) at the end so they are always re-enableable.
        if let Some(cache) = view.as_deref() {
            let mut legend_groups: HashMap<&str, usize> = HashMap::new();
            for p in &cache.joined {
                *legend_groups.entry(group_label(&p.creator)).or_insert(0) += 1;
            }
            let mut legend: Vec<(&str, usize)> = legend_groups.into_iter().collect();
            // Enabled groups first, biggest first; hidden chips trail at the
            // end so a disabled family can always be found again.
            legend.sort_by(|a, b| {
                let a_hidden = self.hidden_groups.contains(a.0);
                let b_hidden = self.hidden_groups.contains(b.0);
                a_hidden
                    .cmp(&b_hidden)
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.cmp(b.0))
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.strong("Groups");
                ui.separator();
                for (name, count) in legend {
                    let is_hidden = self.hidden_groups.contains(name);
                    let base = creator_color(name);
                    let dot = if is_hidden {
                        egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 80)
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
                            // A selection hidden along with its group would
                            // point at an invisible orb; drop it with the
                            // group.
                            if let Some(selected_id) = &self.selected_pareto_model
                                && let Some(p) =
                                    cache.joined.iter().find(|p| &p.model_id == selected_id)
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
            ui.add_space(2.0);
        }

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
            }
            return;
        }
        if visible.is_empty() {
            ui.label("Every group is hidden — click a disabled group chip above to bring models back.");
            return;
        }
        let frontier_names: std::collections::HashSet<&str> = frontier.iter().map(|p| p.model_id.as_str()).collect();
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
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
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
            BenchmarkSource::ArtificialAnalysisSnapshot => format!("AA Intelligence Index snapshot ({ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE})"),
            BenchmarkSource::SWERebench => "SWE-rebench Result@1".to_owned(),
            BenchmarkSource::TerminalBench3 => "Terminal-Bench 3.0 accuracy".to_owned(),
            BenchmarkSource::TerminalBench4 => "Terminal-Bench 4.0 accuracy".to_owned(),
            BenchmarkSource::DeepSWE11 => "DeepSWE v1.1 pass@1".to_owned(),
            BenchmarkSource::LiveBench => format!("LiveBench {} score", self.benchmark_metric.label()),
            BenchmarkSource::ReveloCodeIndex => "Revelo Code Index".to_owned(),
            BenchmarkSource::DesignArena => "Design Arena Elo".to_owned(),
        };

        // The chart body scrolls as one page with the card and table below it,
        // so the plot takes a fixed share of the window instead of whatever is
        // left after the (variable) detail card.
        let screen_height = ui.ctx().input(|i| i.viewport_rect().height());
        let plot_height = (screen_height * 0.55).clamp(320.0, 720.0);
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
                    let is_selected = selected_before.as_deref() == Some(p.model_id.as_str());
                    let base = creator_color(&p.creator);
                    // Non-matching models fade back so matches pop during search.
                    let color = if matches_search {
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
                        // Search matches get a slim halo in their group colour.
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
                    // Search-only matches carry their group colour; frontier and
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
                    "{} group{} hidden via legend chips",
                    self.hidden_groups.len(),
                    if self.hidden_groups.len() == 1 { "" } else { "s" }
                ));
            }
            if self.modality_filter != ModalityFilter::All {
                ui.small(self.modality_filter.label());
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
        let consensus = self.cached_consensus(&p.model_id);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(creator_color(&p.creator)).size(16.0));
                ui.heading(&p.model);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                });
            });
            ui.horizontal_wrapped(|ui| {
                if p.vision {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "vision");
                } else {
                    ui.small("text-only");
                }
                if p.live_market {
                    ui.strong("live market");
                    if p.free_offer_listed {
                        free_offer_badge(ui);
                    }
                } else {
                    ui.label("comparison/catalog price");
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Selected cost: ${:.4} {}", p.cost, self.cost_basis.unit()));
                ui.separator();
                ui.label(match self.benchmark_source {
                    BenchmarkSource::LiveBench => format!("LiveBench {}: {:.1}", self.benchmark_metric.label(), p.score),
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
                && let Some(tokens) = p.tokens_per_task {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Benchmark workload: {} tokens/task", format_compact_number(tokens)));
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
                    ui.label(format!("{} match: {}", self.benchmark_source.short_label(), p.benchmark_name));
                }
            });

            if let Some(quote) = &selected_quote {
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
                                ui.label(format!("{} volume / 24h", format_compact_number(volume)));
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
                        ui.small("All providers in this market (workload $/1M at current weights, \u{25cf} = plotted):");
                        egui::Grid::new("market_quality_providers").striped(true).show(ui, |ui| {
                            ui.strong("");
                            ui.strong("Provider");
                            ui.strong("Input");
                            ui.strong("Cache read");
                            ui.strong("Output");
                            ui.strong("Workload");
                            ui.strong("Trust");
                            ui.strong("Healthy");
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
                                ui.end_row();
                            }
                        });
                    }
                });
            }

            if let Some(consensus) = &consensus {
                ui.collapsing("Source-by-source percentile", |ui| {
                    // Bounded so an open grid cannot push the Pareto-efficient
                    // table below the window edge; long boards scroll inside.
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            egui::Grid::new("selected_consensus_grid").striped(true).show(ui, |ui| {
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
                    ui.label(if p.live_market {
                        "Surplus market"
                    } else {
                        "Surplus comparison/catalog"
                    });
                    ui.end_row();
                }
            });
    }
}

/// Everything the hover tooltip needs beyond the orb itself, resolved once per
/// frame before the plot is built.
struct HoverContext<'a> {
    quote: Option<&'a Quote>,
    history: Option<&'a ModelSeries>,
    low_metric: PriceMetric,
    cost_basis: CostBasis,
    weights: (f64, f64, f64),
    score_source: String,
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
    if let Some(series) = ctx.history
        && let Some((low, low_ts)) = historical_low(
            series,
            ctx.low_metric,
            ctx.weights.0,
            ctx.weights.1,
            ctx.weights.2,
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
    if !p.creator.is_empty() {
        badges.push(p.creator.clone());
    }
    if p.vision {
        badges.push("vision".into());
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
