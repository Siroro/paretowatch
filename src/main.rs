#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use eframe::egui;
use egui_plot::{HoverPosition, Line, Plot, PlotPoint, PlotPoints, Points, Text};
use notify_rust::Notification;
use serde_json::Value;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

mod artificial_analysis_snapshot;
mod history;
mod types;
mod bench;
mod fetch;
#[cfg(test)]
mod testfix;

use artificial_analysis_snapshot::{
    artificial_analysis_snapshot, ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
};
use types::*;
use bench::*;
use fetch::*;


const MENU_SHOW: &str = "paretowatch_show";
const MENU_REFRESH: &str = "paretowatch_refresh";
const MENU_QUIT: &str = "paretowatch_quit";

#[derive(Debug)]
enum WorkerCommand {
    Refresh,
    UpdateSettings(Settings),
    Quit,
}

#[derive(Debug)]
enum WorkerMessage {
    Prices(Result<PriceSnapshot, String>),
    Benchmarks(BenchmarkSource, Result<Vec<Benchmark>, String>),
}

#[derive(Debug, Clone)]
struct PriceChangeEvent {
    display_name: String,
    at: DateTime<Utc>,
    old_blended: f64,
    new_blended: f64,
    old_input: f64,
    new_input: f64,
    old_output: f64,
    new_output: f64,
    source: &'static str,
}

impl PriceChangeEvent {
    fn delta(&self) -> f64 {
        self.new_blended - self.old_blended
    }

    fn percent_delta(&self) -> Option<f64> {
        if self.old_blended.abs() <= f64::EPSILON {
            None
        } else {
            Some((self.new_blended - self.old_blended) / self.old_blended * 100.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Pareto,
    History,
    Alerts,
    Settings,
}

#[derive(Debug)]
enum UiCommand {
    Toggle,
    Show,
    Refresh,
    Quit,
}

struct ParetoWatchApp {
    settings: Settings,
    price_snapshot: Option<PriceSnapshot>,
    benchmark_sets: HashMap<BenchmarkSource, Vec<Benchmark>>,
    benchmark_errors: HashMap<BenchmarkSource, String>,
    history: history::HistoryTracker,
    history_ui: history::HistoryUiState,
    recent_changes: VecDeque<PriceChangeEvent>,
    worker_tx: Sender<WorkerCommand>,
    worker_rx: Receiver<WorkerMessage>,
    ui_rx: Receiver<UiCommand>,
    tab: Tab,
    price_metric: PriceMetric,
    cost_basis: CostBasis,
    benchmark_source: BenchmarkSource,
    benchmark_metric: BenchmarkMetric,
    comparison_mode: ComparisonMode,
    common_scaffold: String,
    liquidity_filter: LiquidityFilter,
    modality_filter: ModalityFilter,
    pareto_search: String,
    selected_pareto_model: Option<String>,
    log_price_axis: bool,
    alert_model: String,
    alert_mode: AlertMode,
    alert_metric: PriceMetric,
    alert_cost_basis: CostBasis,
    alert_direction: AlertDirection,
    alert_threshold: f64,
    alert_score_threshold: f64,
    semantic_alert_state: HashMap<u64, bool>,
    data_version: u64,
    scaffold_cache: Option<(u64, Vec<String>)>,
    pareto_cache: Option<ParetoCache>,
    consensus_cache: Option<(u64, ComparisonMode, String, String, Option<BenchmarkConsensus>)>,
    status: String,
    last_price_error: Option<String>,
    is_visible: bool,
    quitting: bool,
    settings_dirty: bool,
    #[cfg(not(target_os = "linux"))]
    _tray: Option<TrayIcon>,
}

impl ParetoWatchApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = load_settings().unwrap_or_default();
        let (worker_tx, worker_rx) = start_worker(cc.egui_ctx.clone(), settings.clone());
        let (ui_tx, ui_rx) = mpsc::channel();

        install_tray_event_handlers(cc.egui_ctx.clone(), ui_tx);

        #[cfg(not(target_os = "linux"))]
        let tray = Some(create_tray().expect("failed to create system tray icon"));

        let _ = worker_tx.send(WorkerCommand::Refresh);

        Self {
            settings,
            price_snapshot: None,
            benchmark_sets: {
                let mut sets = HashMap::new();
                sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, artificial_analysis_snapshot());
                sets
            },
            benchmark_errors: HashMap::new(),
            history: history::HistoryTracker::open(&history::history_log_path()),
            history_ui: history::HistoryUiState::default(),
            recent_changes: VecDeque::new(),
            worker_tx,
            worker_rx,
            ui_rx,
            tab: Tab::Pareto,
            price_metric: PriceMetric::Blended,
            cost_basis: CostBasis::PerMillion,
            benchmark_source: BenchmarkSource::CompositeAgentic,
            benchmark_metric: BenchmarkMetric::Overall,
            comparison_mode: ComparisonMode::BestAvailableAgent,
            common_scaffold: default_common_scaffold(),
            liquidity_filter: LiquidityFilter::Any,
            modality_filter: ModalityFilter::All,
            pareto_search: String::new(),
            selected_pareto_model: None,
            log_price_axis: true,
            alert_model: String::new(),
            alert_mode: AlertMode::Threshold,
            alert_metric: PriceMetric::Blended,
            alert_cost_basis: CostBasis::PerMillion,
            alert_direction: AlertDirection::BelowOrEqual,
            alert_threshold: 1.0,
            alert_score_threshold: 50.0,
            semantic_alert_state: HashMap::new(),
            data_version: 0,
            scaffold_cache: None,
            pareto_cache: None,
            consensus_cache: None,
            status: "Starting…".into(),
            last_price_error: None,
            is_visible: false,
            quitting: false,
            settings_dirty: false,
            #[cfg(not(target_os = "linux"))]
            _tray: tray,
        }
    }

    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.is_visible = visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn handle_ui_commands(&mut self, ctx: &egui::Context) {
        while let Ok(cmd) = self.ui_rx.try_recv() {
            match cmd {
                UiCommand::Toggle => self.set_visible(ctx, !self.is_visible),
                UiCommand::Show => self.set_visible(ctx, true),
                UiCommand::Refresh => {
                    let _ = self.worker_tx.send(WorkerCommand::Refresh);
                    self.status = "Refreshing…".into();
                }
                UiCommand::Quit => {
                    self.quitting = true;
                    let _ = self.worker_tx.send(WorkerCommand::Quit);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn handle_worker_messages(&mut self) {
        let mut data_dirty = false;
        loop {
            match self.worker_rx.try_recv() {
                Ok(WorkerMessage::Prices(Ok(snapshot))) => {
                    if let Some(previous) = &self.price_snapshot {
                        for change in detect_price_changes(previous, &snapshot, &self.settings) {
                            self.recent_changes.push_front(change);
                        }
                        while self.recent_changes.len() > 80 {
                            self.recent_changes.pop_back();
                        }
                    }
                    if self.alert_model.is_empty() {
                        if let Some(q) = snapshot.quotes.first() {
                            self.alert_model = q.model.clone();
                        }
                    }
                    self.status = if snapshot.market_overlay_count > 0 {
                        format!("{} priced models · {} live market quotes", snapshot.quotes.len(), snapshot.market_overlay_count)
                    } else {
                        format!("{} priced models · comparison feed", snapshot.quotes.len())
                    };
                    self.price_snapshot = Some(snapshot);
                    self.last_price_error = None;
                    self.data_version += 1;
                    self.evaluate_semantic_alerts();
                    data_dirty = true;
                }
                Ok(WorkerMessage::Prices(Err(err))) => {
                    self.last_price_error = Some(err.clone());
                    self.status = format!("Pricing error: {err}");
                }
                Ok(WorkerMessage::Benchmarks(source, Ok(models))) => {
                    self.benchmark_sets.insert(source, models);
                    self.benchmark_errors.remove(&source);
                    self.data_version += 1;
                    data_dirty = true;
                }
                Ok(WorkerMessage::Benchmarks(source, Err(err))) => {
                    self.benchmark_errors.insert(source, err);
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        // Long-term history: diff each delivered poll against the last
        // recorded state. Writes only when something actually changed.
        if data_dirty {
            if let Some(snapshot) = &self.price_snapshot {
                self.history.record(snapshot, &self.benchmark_sets, self.data_version, Utc::now());
            }
        }
    }

    /// Scaffold list only changes when benchmark data changes, but computing it
    /// walks every row of every leaderboard; never do that per UI frame.
    fn cached_scaffolds(&mut self) -> Vec<String> {
        if let Some((version, options)) = &self.scaffold_cache {
            if *version == self.data_version {
                return options.clone();
            }
        }
        let options = available_scaffolds(&self.benchmark_sets);
        self.scaffold_cache = Some((self.data_version, options.clone()));
        options
    }

    fn pareto_cache_key(&self) -> ParetoCacheKey {
        ParetoCacheKey {
            data_version: self.data_version,
            price_metric: self.price_metric,
            cost_basis: self.cost_basis,
            benchmark_source: self.benchmark_source,
            benchmark_metric: if self.benchmark_source == BenchmarkSource::LiveBench {
                self.benchmark_metric
            } else {
                BenchmarkMetric::AgenticCoding
            },
            comparison_mode: self.comparison_mode,
            common_scaffold: self.common_scaffold.clone(),
            liquidity_filter: self.liquidity_filter,
            modality_filter: self.modality_filter,
            input_weight: self.settings.input_weight,
            cache_read_weight: self.settings.cache_read_weight,
            output_weight: self.settings.output_weight,
        }
    }

    /// Returns the cached join/frontier for the current inputs, recomputing it
    /// only when data or a relevant control actually changed.
    fn ensure_pareto_view(&mut self) -> Option<ParetoView> {
        let key = self.pareto_cache_key();
        if let Some(cache) = self.pareto_cache.as_ref() {
            if cache.key == key {
                return Some(cache.view());
            }
        }
        let snapshot = self.price_snapshot.as_ref()?;
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
        );
        let filtered_quotes = snapshot
            .quotes
            .iter()
            .filter(|quote| self.modality_filter.allows(quote.vision))
            .filter_map(|quote| {
                self.liquidity_filter
                    .apply(quote, weights.0, weights.1, weights.2)
            })
            .collect::<Vec<_>>();
        let active_benchmarks = benchmarks_for_source(
            self.benchmark_source,
            &self.benchmark_sets,
            self.comparison_mode,
            &self.common_scaffold,
        );
        let benchmarks_present = !active_benchmarks.is_empty();
        let joined = joined_points(
            &filtered_quotes,
            &active_benchmarks,
            key.price_metric,
            key.cost_basis,
            key.benchmark_metric,
            weights.0,
            weights.1,
            weights.2,
        );
        let frontier = pareto_frontier(&joined);
        self.pareto_cache = Some(ParetoCache {
            key,
            benchmarks_present,
            filtered_quotes,
            joined,
            frontier,
        });
        self.pareto_cache.as_ref().map(ParetoCache::view)
    }

    fn selected_pareto_quote(&self, model_id: &str) -> Option<Quote> {
        self.pareto_cache
            .as_ref()?
            .filtered_quotes
            .iter()
            .find(|quote| quote.model == model_id)
            .cloned()
    }

    /// Consensus panel ranks a quote across all eight leaderboards; cache it per
    /// selection instead of re-ranking every frame.
    fn cached_consensus(&mut self, model_id: &str) -> Option<BenchmarkConsensus> {
        if let Some((version, mode, scaffold, cached_id, consensus)) = &self.consensus_cache {
            if *version == self.data_version
                && *mode == self.comparison_mode
                && *scaffold == self.common_scaffold
                && cached_id == model_id
            {
                return consensus.clone();
            }
        }
        let quote = self.selected_pareto_quote(model_id)?;
        let consensus = benchmark_consensus_for_quote(
            &quote,
            &self.benchmark_sets,
            self.comparison_mode,
            &self.common_scaffold,
        );
        self.consensus_cache = Some((
            self.data_version,
            self.comparison_mode,
            self.common_scaffold.clone(),
            model_id.to_owned(),
            consensus.clone(),
        ));
        consensus
    }

    fn evaluate_semantic_alerts(&mut self) {
        let Some(snapshot) = self.price_snapshot.clone() else { return };
        let alerts = self.settings.alerts.clone();
        for alert in alerts {
            if !alert.enabled || !alert.mode.benchmark_dependent() {
                self.semantic_alert_state.remove(&alert.id);
                continue;
            }
            let benchmarks = benchmarks_for_source(
                alert.benchmark_source,
                &self.benchmark_sets,
                alert.comparison_mode,
                &alert.common_scaffold,
            );
            if benchmarks.is_empty() { continue; }
            let benchmark_metric = if alert.benchmark_source == BenchmarkSource::LiveBench {
                alert.benchmark_metric
            } else {
                BenchmarkMetric::AgenticCoding
            };
            let quotes = snapshot
                .quotes
                .iter()
                .filter_map(|quote| {
                    alert.liquidity_filter.apply(
                        quote,
                        self.settings.input_weight,
                        self.settings.cache_read_weight,
                        self.settings.output_weight,
                    )
                })
                .collect::<Vec<_>>();
            let joined = joined_points(
                &quotes,
                &benchmarks,
                alert.metric,
                alert.cost_basis,
                benchmark_metric,
                self.settings.input_weight,
                self.settings.cache_read_weight,
                self.settings.output_weight,
            );
            let Some(target) = joined.iter().find(|point| point.model_id == alert.model) else { continue };
            let frontier = pareto_frontier(&joined);
            let on_frontier = frontier.iter().any(|point| point.model_id == alert.model);
            let condition = match alert.mode {
                AlertMode::EntersFrontier => on_frontier,
                AlertMode::LeavesFrontier => !on_frontier,
                AlertMode::CheapestAboveScore => joined
                    .iter()
                    .filter(|point| point.score >= alert.score_threshold)
                    .min_by(|a, b| a.cost.total_cmp(&b.cost))
                    .map(|point| point.model_id == alert.model)
                    .unwrap_or(false),
                AlertMode::Threshold | AlertMode::AnyChange => continue,
            };

            let Some(previous) = self.semantic_alert_state.get(&alert.id).copied() else {
                self.semantic_alert_state.insert(alert.id, condition);
                continue;
            };
            if condition && !previous {
                let (summary, body) = match alert.mode {
                    AlertMode::EntersFrontier => (
                        format!("◆ {} entered the Pareto frontier", target.model),
                        format!(
                            "{}: {:.1} · {} ${:.4}{}\n{} · {}",
                            alert.benchmark_source.short_label(),
                            target.score,
                            alert.metric.label(),
                            target.cost,
                            alert.cost_basis.unit(),
                            alert.comparison_mode.label(),
                            alert.liquidity_filter.label(),
                        ),
                    ),
                    AlertMode::LeavesFrontier => (
                        format!("◇ {} left the Pareto frontier", target.model),
                        format!(
                            "{}: {:.1} · {} ${:.4}{}\n{} · {}",
                            alert.benchmark_source.short_label(),
                            target.score,
                            alert.metric.label(),
                            target.cost,
                            alert.cost_basis.unit(),
                            alert.comparison_mode.label(),
                            alert.liquidity_filter.label(),
                        ),
                    ),
                    AlertMode::CheapestAboveScore => (
                        format!("★ {} is now cheapest above {:.1}", target.model, alert.score_threshold),
                        format!(
                            "{} score {:.1} · {} ${:.4}{}\n{} · {}",
                            alert.benchmark_source.short_label(),
                            target.score,
                            alert.metric.label(),
                            target.cost,
                            alert.cost_basis.unit(),
                            alert.comparison_mode.label(),
                            alert.liquidity_filter.label(),
                        ),
                    ),
                    AlertMode::Threshold | AlertMode::AnyChange => unreachable!(),
                };
                let _ = Notification::new().summary(&summary).body(&body).show();
            }
            self.semantic_alert_state.insert(alert.id, condition);
        }
    }

    fn save_and_push_settings(&mut self) {
        self.settings.poll_seconds = self.settings.poll_seconds.max(30);
        if self.settings.input_weight.max(0.0)
            + self.settings.cache_read_weight.max(0.0)
            + self.settings.output_weight.max(0.0)
            <= f64::EPSILON
        {
            self.settings.input_weight = 15.0;
            self.settings.cache_read_weight = 80.0;
            self.settings.output_weight = 5.0;
        }
        if let Err(err) = save_settings(&self.settings) {
            self.status = format!("Could not save settings: {err}");
        } else {
            self.status = "Settings saved".into();
        }
        let valid_ids = self.settings.alerts.iter().map(|a| a.id).collect::<std::collections::HashSet<_>>();
        self.semantic_alert_state.retain(|id, _| valid_ids.contains(id));
        let _ = self
            .worker_tx
            .send(WorkerCommand::UpdateSettings(self.settings.clone()));
        self.settings_dirty = false;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("ParetoWatch");
            ui.separator();
            ui.label(&self.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Refresh").clicked() {
                    let _ = self.worker_tx.send(WorkerCommand::Refresh);
                    self.status = "Refreshing…".into();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Pareto, "Pareto"),
                (Tab::History, "History"),
                (Tab::Alerts, "Alerts"),
                (Tab::Settings, "Settings"),
            ] {
                if ui.selectable_label(self.tab == tab, label).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
    }

    fn pareto_tab(&mut self, ui: &mut egui::Ui) {
        let mut chart_axes_changed = false;
        let scaffold_options = self.cached_scaffolds();
        ui.horizontal_wrapped(|ui| {
            ui.label("Price:");
            egui::ComboBox::from_id_salt("price_metric")
                .selected_text(self.price_metric.label())
                .show_ui(ui, |ui| {
                    for metric in [PriceMetric::Blended, PriceMetric::Input, PriceMetric::Output] {
                        if ui.selectable_value(&mut self.price_metric, metric, metric.label()).changed() {
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
                            if ui.selectable_value(&mut self.cost_basis, basis, basis.label()).changed() {
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
                        BenchmarkSource::ArtificialAnalysisSnapshot,
                        BenchmarkSource::SWERebench,
                        BenchmarkSource::TerminalBench3,
                        BenchmarkSource::DeepSWE11,
                        BenchmarkSource::LiveBench,
                        BenchmarkSource::ReveloCodeIndex,
                        BenchmarkSource::SWEBenchLive,
                        BenchmarkSource::SWEBenchVerified,
                    ] {
                        if ui.selectable_value(&mut self.benchmark_source, source, source.label()).changed() {
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
                        for metric in [BenchmarkMetric::Overall, BenchmarkMetric::Coding, BenchmarkMetric::AgenticCoding] {
                            if ui.selectable_value(&mut self.benchmark_metric, metric, metric.label()).changed() {
                                chart_axes_changed = true;
                            }
                        }
                    });
            } else if !self.benchmark_source.is_composite() {
                // Composite row names already disclose their method; a separate
                // metric note would just repeat it in the toolbar.
                ui.label(match self.benchmark_source {
                    BenchmarkSource::ArtificialAnalysisSnapshot => "Metric: Intelligence Index snapshot",
                    BenchmarkSource::SWERebench => "Metric: Result@1",
                    BenchmarkSource::TerminalBench3 => "Metric: accuracy",
                    BenchmarkSource::DeepSWE11 => "Metric: pass@1",
                    BenchmarkSource::ReveloCodeIndex => "Metric: Code Index",
                    BenchmarkSource::SWEBenchLive => "Metric: resolved %",
                    BenchmarkSource::SWEBenchVerified => "Metric: resolved % (legacy)",
                    _ => unreachable!(),
                });
            }

            if self.price_metric == PriceMetric::Blended {
                ui.separator();
                ui.label("Fresh input");
                if ui.add(egui::DragValue::new(&mut self.settings.input_weight).speed(0.1).range(0.0..=100.0)).changed() {
                    self.settings_dirty = true;
                }
                ui.label("Cache read");
                if ui.add(egui::DragValue::new(&mut self.settings.cache_read_weight).speed(0.1).range(0.0..=100.0)).changed() {
                    self.settings_dirty = true;
                }
                ui.label("Output");
                if ui.add(egui::DragValue::new(&mut self.settings.output_weight).speed(0.1).range(0.0..=100.0)).changed() {
                    self.settings_dirty = true;
                }
                if self.settings_dirty && ui.button("Apply weights").clicked() {
                    self.save_and_push_settings();
                    chart_axes_changed = true;
                    let _ = self.worker_tx.send(WorkerCommand::Refresh);
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
                    for mode in [ComparisonMode::ModelCapability, ComparisonMode::BestAvailableAgent, ComparisonMode::CommonScaffold] {
                        if ui.selectable_value(&mut self.comparison_mode, mode, mode.label()).changed() {
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
                            if ui.selectable_value(&mut self.common_scaffold, scaffold.clone(), scaffold).changed() {
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
                    for filter in [LiquidityFilter::Any, LiquidityFilter::Trusted, LiquidityFilter::Healthy3, LiquidityFilter::Healthy10] {
                        if ui.selectable_value(&mut self.liquidity_filter, filter, filter.label()).changed() {
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
                    for filter in [ModalityFilter::All, ModalityFilter::Vision, ModalityFilter::TextOnly] {
                        if ui.selectable_value(&mut self.modality_filter, filter, filter.label()).changed() {
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
            if !self.pareto_search.trim().is_empty() && ui.small_button("Clear search").clicked() {
                self.pareto_search.clear();
            }
            ui.separator();
            if ui.small_button("Reset zoom").clicked() {
                reset_zoom = true;
            }
            if ui.checkbox(&mut self.log_price_axis, "Log price axis").changed() {
                reset_zoom = true;
            }
            if self.selected_pareto_model.is_some() && ui.small_button("Clear selection").clicked() {
                self.selected_pareto_model = None;
            }
        });
        ui.add_space(2.0);
        ui.small("Wheel: zoom · middle-drag: pan · right-drag: box zoom · double-click: reset · click an orb: inspect");

        // The whole chart body scrolls as one page so the detail card and
        // frontier table can never be pushed out of the viewport.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
        ui.add_space(6.0);
        if !self.benchmark_source.is_composite() {
            if let Some(err) = self.benchmark_errors.get(&self.benchmark_source) {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{}: {err}", self.benchmark_source.label()),
                );
            }
        }

        if self.price_snapshot.is_none() {
            ui.spinner();
            ui.label("Waiting for pricing data…");
            return;
        }
        let ParetoView { joined, frontier, benchmarks_present } = self
            .ensure_pareto_view()
            .unwrap_or_else(|| ParetoView {
                joined: Vec::new(),
                frontier: Vec::new(),
                benchmarks_present: false,
            });

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
        let frontier_names: std::collections::HashSet<&str> = frontier.iter().map(|p| p.model_id.as_str()).collect();
        let selected_before = self.selected_pareto_model.clone();
        let search_query = self.pareto_search.clone();
        let search_active = !normalize(&search_query).replace(' ', "").is_empty();
        let search_matches: Vec<bool> = joined
            .iter()
            .map(|p| !search_active || pareto_search_matches(&search_query, p))
            .collect();

        // Colour legend for the model groups (creators) currently on the chart.
        let mut group_counts: HashMap<&str, usize> = HashMap::new();
        for p in &joined {
            *group_counts.entry(group_label(&p.creator)).or_insert(0) += 1;
        }
        let mut group_counts: Vec<(&str, usize)> = group_counts.into_iter().collect();
        group_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ui.horizontal_wrapped(|ui| {
            ui.strong("Groups");
            ui.separator();
            for (name, count) in group_counts.iter().take(12) {
                ui.label(egui::RichText::new("●").color(creator_color(name)).size(13.0));
                ui.small(format!("{name} ({count})"));
                ui.add_space(8.0);
            }
        });
        ui.add_space(4.0);
        if search_active {
            let total = search_matches.iter().filter(|m| **m).count();
            ui.small(format!(
                "Search “{}”: {} of {} shown models match (non-matches are dimmed).",
                search_query.trim(),
                total,
                joined.len()
            ));
            ui.add_space(4.0);
        }

        let log_price_axis = self.log_price_axis;
        let cost_basis = self.cost_basis;
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
            BenchmarkSource::DeepSWE11 => "DeepSWE v1.1 pass@1".to_owned(),
            BenchmarkSource::LiveBench => format!("LiveBench {} score", self.benchmark_metric.label()),
            BenchmarkSource::ReveloCodeIndex => "Revelo Code Index".to_owned(),
            BenchmarkSource::SWEBenchLive => "SWE-bench Live resolved %".to_owned(),
            BenchmarkSource::SWEBenchVerified => "SWE-bench Verified resolved % (legacy)".to_owned(),
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
        let clicked_model = Plot::new("pareto_plot")
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
                    let price = price_from_plot_x(position.x, log_price_axis);
                    Some(format!("{plot_name}\n${:.6} {} · score {:.1}", price, cost_basis.unit(), position.y))
                }
                _ => None,
            })
            .show(ui, |plot_ui| {
                if reset_zoom {
                    plot_ui.set_auto_bounds(true);
                }

                for (p, matches_search) in joined.iter().zip(search_matches.iter().copied()) {
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
                for (p, matches_search) in joined.iter().zip(search_matches.iter().copied()) {
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

                if plot_ui.response().clicked() {
                    if let Some(click_pos) = plot_ui.response().interact_pointer_pos() {
                        return joined
                            .iter()
                            .filter_map(|p| {
                                let plot_x = price_to_plot_x(p.cost, log_price_axis);
                                let screen_pos = plot_ui.screen_from_plot(PlotPoint::new(plot_x, p.score));
                                let distance = (screen_pos - click_pos).length();
                                (distance <= 16.0).then_some((distance, p.model_id.clone()))
                            })
                            .min_by(|a, b| a.0.total_cmp(&b.0))
                            .map(|(_, model_id)| model_id);
                    }
                }
                None
            })
            .inner;
        *ui.style_mut() = (*saved_style).clone();

        if let Some(model_id) = clicked_model {
            self.selected_pareto_model = Some(model_id);
        }

        let selected_id = self.selected_pareto_model.clone();
        let selected = selected_id
            .as_deref()
            .and_then(|id| joined.iter().find(|p| p.model_id.as_str() == id));

        ui.horizontal(|ui| {
            ui.strong(format!("{} matched models", joined.len()));
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
                    self.frontier_table(ui, &frontier);
                });
            });
        } else {
            if let Some(p) = selected {
                ui.add_space(6.0);
                self.pareto_detail_card(ui, p);
            }
            self.frontier_table(ui, &frontier);
        }
        });
    }

    fn pareto_detail_card(&mut self, ui: &mut egui::Ui, p: &JoinedPoint) {
        let selected_quote = self.selected_pareto_quote(&p.model_id);
        let consensus = self.cached_consensus(&p.model_id);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("●").color(creator_color(&p.creator)).size(16.0));
                ui.heading(&p.model);
                if p.vision {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "vision");
                } else {
                    ui.small("text-only");
                }
                if p.live_market {
                    ui.strong("live market");
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
            if self.cost_basis == CostBasis::EstimatedPerTask {
                if let Some(tokens) = p.tokens_per_task {
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
                                ui.label(format!("{discount:.1}% discount \u{00b7} {direction}"));
                            }
                        });
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

            if self.benchmark_source.is_composite() {
                // The composite row name carries the full per-benchmark
                // breakdown (measured/adjusted, prior, every board's raw
                // score, pending boards). Show it — otherwise the method
                // exists only in data and never reaches the user.
                ui.collapsing("Composite breakdown", |ui| {
                    ui.small(&p.benchmark_name);
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

            ui.horizontal_wrapped(|ui| {
                if self.cost_basis == CostBasis::PerMillion {
                    if ui.button("Set price alert").clicked() {
                        self.alert_model = p.model_id.clone();
                        self.alert_mode = AlertMode::Threshold;
                        self.alert_metric = self.price_metric;
                        self.alert_cost_basis = CostBasis::PerMillion;
                        self.alert_threshold = p.cost;
                        self.tab = Tab::Alerts;
                    }
                }
                if ui.button("Watch any change").clicked() {
                    self.alert_model = p.model_id.clone();
                    self.alert_mode = AlertMode::AnyChange;
                    self.tab = Tab::Alerts;
                }
                if ui.button("Watch frontier entry").clicked() {
                    self.alert_model = p.model_id.clone();
                    self.alert_mode = AlertMode::EntersFrontier;
                    self.alert_metric = self.price_metric;
                    self.alert_cost_basis = self.cost_basis;
                    self.tab = Tab::Alerts;
                }
                if ui.button("Watch frontier exit").clicked() {
                    self.alert_model = p.model_id.clone();
                    self.alert_mode = AlertMode::LeavesFrontier;
                    self.alert_metric = self.price_metric;
                    self.alert_cost_basis = self.cost_basis;
                    self.tab = Tab::Alerts;
                }
                if ui.button("Watch cheapest ≥ current score").clicked() {
                    self.alert_model = p.model_id.clone();
                    self.alert_mode = AlertMode::CheapestAboveScore;
                    self.alert_metric = self.price_metric;
                    self.alert_cost_basis = self.cost_basis;
                    self.alert_score_threshold = p.score;
                    self.tab = Tab::Alerts;
                }
            });
        });
    }

    fn frontier_table(&mut self, ui: &mut egui::Ui, frontier: &[JoinedPoint]) {
        egui::Grid::new("frontier_table").striped(true).show(ui, |ui| {
            ui.strong("Model");
            ui.strong(if self.cost_basis == CostBasis::EstimatedPerTask { "Est. $/task" } else { "Price" });
            ui.strong("Score");
            ui.strong("Source");
            ui.end_row();
            for p in frontier {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("●").color(creator_color(&p.creator)).size(11.0));
                    if ui.selectable_label(self.selected_pareto_model.as_deref() == Some(p.model_id.as_str()), &p.model).clicked() {
                        self.selected_pareto_model = Some(p.model_id.clone());
                    }
                });
                ui.label(format!("${:.4}", p.cost));
                ui.label(format!("{:.1}", p.score));
                ui.label(if p.live_market { "Surplus market" } else { "Surplus comparison/catalog" });
                ui.end_row();
            }
        });
    }

    fn history_tab(&mut self, ui: &mut egui::Ui) {
        history::ui::show(
            ui,
            &self.settings,
            self.price_snapshot.as_ref(),
            &self.history,
            &mut self.history_ui,
        );
    }

    fn alerts_tab(&mut self, ui: &mut egui::Ui) {
        let quotes = self.price_snapshot.as_ref().map(|s| s.quotes.clone()).unwrap_or_default();
        let scaffold_options = self.cached_scaffolds();

        ui.group(|ui| {
            ui.strong("New alert");
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("alert_model")
                    .width(260.0)
                    .selected_text(
                        quotes.iter().find(|q| q.model == self.alert_model)
                            .map(|q| q.display_name.as_str()).unwrap_or("Select model"),
                    )
                    .show_ui(ui, |ui| {
                        for q in &quotes {
                            ui.selectable_value(&mut self.alert_model, q.model.clone(), &q.display_name);
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
                            for metric in [PriceMetric::Blended, PriceMetric::Input, PriceMetric::Output] {
                                ui.selectable_value(&mut self.alert_metric, metric, metric.label());
                            }
                        });
                    egui::ComboBox::from_id_salt("alert_direction")
                        .selected_text(self.alert_direction.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.alert_direction, AlertDirection::BelowOrEqual, "at or below");
                            ui.selectable_value(&mut self.alert_direction, AlertDirection::AboveOrEqual, "at or above");
                        });
                    ui.label("$");
                    ui.add(egui::DragValue::new(&mut self.alert_threshold).speed(0.05).range(0.0..=100_000.0));
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
                                BenchmarkSource::ArtificialAnalysisSnapshot,
                                BenchmarkSource::SWERebench,
                                BenchmarkSource::TerminalBench3,
                                BenchmarkSource::DeepSWE11,
                                BenchmarkSource::LiveBench,
                                BenchmarkSource::ReveloCodeIndex,
                                BenchmarkSource::SWEBenchLive,
                                BenchmarkSource::SWEBenchVerified,
                            ] {
                                ui.selectable_value(&mut self.benchmark_source, source, source.label());
                            }
                        });
                    if self.benchmark_source == BenchmarkSource::LiveBench {
                        egui::ComboBox::from_id_salt("alert_benchmark_metric")
                            .selected_text(self.benchmark_metric.label())
                            .show_ui(ui, |ui| {
                                for metric in [BenchmarkMetric::Overall, BenchmarkMetric::Coding, BenchmarkMetric::AgenticCoding] {
                                    ui.selectable_value(&mut self.benchmark_metric, metric, metric.label());
                                }
                            });
                    }
                    ui.separator();
                    egui::ComboBox::from_id_salt("alert_comparison_mode")
                        .selected_text(self.comparison_mode.label())
                        .show_ui(ui, |ui| {
                            for mode in [ComparisonMode::ModelCapability, ComparisonMode::BestAvailableAgent, ComparisonMode::CommonScaffold] {
                                ui.selectable_value(&mut self.comparison_mode, mode, mode.label());
                            }
                        });
                    if self.comparison_mode == ComparisonMode::CommonScaffold {
                        egui::ComboBox::from_id_salt("alert_common_scaffold")
                            .width(180.0)
                            .selected_text(&self.common_scaffold)
                            .show_ui(ui, |ui| {
                                for scaffold in &scaffold_options {
                                    ui.selectable_value(&mut self.common_scaffold, scaffold.clone(), scaffold);
                                }
                            });
                    }
                    ui.separator();
                    egui::ComboBox::from_id_salt("alert_liquidity_filter")
                        .selected_text(self.liquidity_filter.label())
                        .show_ui(ui, |ui| {
                            for filter in [LiquidityFilter::Any, LiquidityFilter::Trusted, LiquidityFilter::Healthy3, LiquidityFilter::Healthy10] {
                                ui.selectable_value(&mut self.liquidity_filter, filter, filter.label());
                            }
                        });
                    ui.separator();
                    ui.label("Price axis:");
                    egui::ComboBox::from_id_salt("semantic_alert_price_metric")
                        .selected_text(self.alert_metric.label())
                        .show_ui(ui, |ui| {
                            for metric in [PriceMetric::Blended, PriceMetric::Input, PriceMetric::Output] {
                                if ui.selectable_value(&mut self.alert_metric, metric, metric.label()).changed()
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
                                    ui.selectable_value(&mut self.alert_cost_basis, basis, basis.label());
                                }
                            });
                    }
                    if self.alert_mode == AlertMode::CheapestAboveScore {
                        ui.separator();
                        ui.label("Minimum score");
                        ui.add(egui::DragValue::new(&mut self.alert_score_threshold).speed(1.0).range(0.0..=100.0));
                    }
                });
            }

            if ui.add_enabled(!self.alert_model.is_empty(), egui::Button::new("Add alert")).clicked() {
                self.settings.alerts.push(AlertRule {
                    id: next_alert_id(&self.settings.alerts),
                    model: self.alert_model.clone(),
                    mode: self.alert_mode,
                    metric: self.alert_metric,
                    cost_basis: if self.alert_mode.benchmark_dependent() { self.alert_cost_basis } else { CostBasis::PerMillion },
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
                let name = quotes.iter().find(|q| q.model == alert.model)
                    .map(|q| q.display_name.as_str()).unwrap_or(&alert.model);
                ui.label(name);
                let rule_text = match alert.mode {
                    AlertMode::Threshold => format!("{} {} ${:.4}", alert.metric.label(), alert.direction.label(), alert.threshold),
                    AlertMode::AnyChange => "Any price change".into(),
                    AlertMode::EntersFrontier => format!("Enters frontier · {} · {} · {}", alert.benchmark_source.short_label(), alert.comparison_mode.label(), alert.cost_basis.label()),
                    AlertMode::LeavesFrontier => format!("Leaves frontier · {} · {} · {}", alert.benchmark_source.short_label(), alert.comparison_mode.label(), alert.cost_basis.label()),
                    AlertMode::CheapestAboveScore => format!("Cheapest ≥ {:.1} · {} · {}", alert.score_threshold, alert.benchmark_source.short_label(), alert.cost_basis.label()),
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
                    quotes.iter().find(|q| q.model == alert.model).map(|q| {
                        format!("${:.4} blend", q.price(
                            PriceMetric::Blended,
                            input_weight,
                            cache_read_weight,
                            output_weight,
                        ))
                    }).unwrap_or_else(|| "—".into())
                } else {
                    quotes.iter().find(|q| q.model == alert.model).map(|q| {
                        format!("${:.4}", q.price(
                            alert.metric,
                            input_weight,
                            cache_read_weight,
                            output_weight,
                        ))
                    }).unwrap_or_else(|| "—".into())
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
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
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
                            ui.colored_label(color, egui::RichText::new(arrow).size(22.0).strong());
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
                            ui.label(format!("Blended ${:.6} → ${:.6}", change.old_blended, change.new_blended));
                            ui.separator();
                            ui.label(format!("Input ${:.6} → ${:.6}", change.old_input, change.new_input));
                            ui.separator();
                            ui.label(format!("Output ${:.6} → ${:.6}", change.old_output, change.new_output));
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Polling");
        ui.horizontal(|ui| {
            ui.label("Refresh every");
            if ui.add(egui::DragValue::new(&mut self.settings.poll_seconds).speed(10).range(30..=3600)).changed() {
                self.settings_dirty = true;
            }
            ui.label("seconds");
        });
        ui.label("The live Surplus market feed currently advertises a 30-second cache; the comparison/catalog feeds update more slowly.");

        ui.add_space(14.0);
        ui.heading("Benchmarks");
        ui.label("All remote benchmark feeds are public and require no API key. Each remote feed refreshes independently every six hours.");
        ui.label(format!("Artificial Analysis is bundled as a static {} snapshot from {}; it never requires an API key and stays in the composite until we manually bump it.", ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION, ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE));
        ui.label("Default composite is a robust precision-weighted posterior. Board weights: 20% SWE-rebench + 15% Terminal-Bench 3.0 + 15% DeepSWE + 10% LiveBench Agentic + 7.5% SWE-bench Live + 5% Revelo Code Index + 2.5% SWE-bench Verified (deployment flavor demotes model-level boards, capability flavor demotes harness-specific ones). Each board's weight is scaled by n/(n+2) over its anchored population — percentiles from tiny boards barely count. A small AA prior (10%) enters on the same anchored scale, ranked within the models the boards actually evaluate. Two Huber passes symmetrically downweight boards far from the consensus instead of trimming the lowest. Boards that have evaluated other models but not this one add 25%-weight pseudo-evidence at the model's prior, so a model topping two boards cannot outrank broad top-tier coverage; models no board has evaluated yet enter at 75% of their AA standing. Execution-mode SKUs (e.g. GPT-5.6 Sol Pro) inherit their base variant's rows.");
        ui.label("Comparison modes: Model capability prefers model-only rows, but automatically falls back to the best model+harness row for agent-only leaderboards; Best available agent always picks the strongest observed model+harness row per source; Same/common scaffold keeps only rows matching the selected harness.");
        for source in BenchmarkSource::display_sources() {
            let mapped_models = self.price_snapshot.as_ref().map(|snapshot| {
                let active = benchmarks_for_source(
                    source,
                    &self.benchmark_sets,
                    self.comparison_mode,
                    &self.common_scaffold,
                );
                snapshot.quotes.iter()
                    .filter(|quote| best_benchmark_match(quote, &active).is_some())
                    .count()
            });
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to(source.label(), source.source_url());
                if let Some(rows) = self.benchmark_sets.get(&source) {
                    ui.label(format!("· {} source rows", rows.len()));
                    if let Some(mapped) = mapped_models {
                        ui.label(format!("· {mapped} Surplus text models mapped"));
                    }
                } else {
                    ui.label("· waiting");
                }
                if let Some(err) = self.benchmark_errors.get(&source) {
                    ui.colored_label(ui.visuals().error_fg_color, format!("· {err}"));
                }
            });
        }

        ui.add_space(14.0);
        ui.heading("Pricing diagnostics");
        if let Some(snapshot) = &self.price_snapshot {
            ui.label(format!("Base feed: {} · {} priced text-output models", snapshot.base_source, snapshot.quotes.len()));
            ui.small("Model universe is gated by /v1/models architecture metadata: multimodal input is allowed, but output must be text-only. Image/video/audio/embedding/rerank products are excluded.");
            if snapshot.market_overlay_count > 0 {
                ui.label(format!("Live marketplace overlay: {} quotes", snapshot.market_overlay_count));
            } else if let Some(err) = &snapshot.market_error {
                ui.collapsing("Live marketplace overlay unavailable", |ui| {
                    ui.label(err);
                    ui.label("The app continues using Surplus comparison/catalog prices, so charts and alerts still work.");
                });
            } else {
                ui.label("Live marketplace overlay: no quotes returned");
            }
        } else {
            ui.label("Waiting for pricing data…");
        }
        if let Some(err) = &self.last_price_error {
            ui.colored_label(ui.visuals().error_fg_color, format!("Last base pricing error: {err}"));
        }

        ui.add_space(14.0);
        ui.heading("Blended price workload");
        ui.label("Blended price is a normalized $/1M-token rate, not the cost of a fixed-size request. Agentic coding is input-heavy, and repeated context is often served from prompt cache, so cached reads get their own weight.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Agentic coding 15/80/5").clicked() {
                self.settings.input_weight = 15.0;
                self.settings.cache_read_weight = 80.0;
                self.settings.output_weight = 5.0;
                self.settings_dirty = true;
            }
            if ui.button("Input-heavy no-cache 95/0/5").clicked() {
                self.settings.input_weight = 95.0;
                self.settings.cache_read_weight = 0.0;
                self.settings.output_weight = 5.0;
                self.settings_dirty = true;
            }
            if ui.button("Balanced 50/0/50").clicked() {
                self.settings.input_weight = 50.0;
                self.settings.cache_read_weight = 0.0;
                self.settings.output_weight = 50.0;
                self.settings_dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Fresh input");
            if ui.add(egui::DragValue::new(&mut self.settings.input_weight).speed(0.1).range(0.0..=100.0)).changed() {
                self.settings_dirty = true;
            }
            ui.label("Cache read");
            if ui.add(egui::DragValue::new(&mut self.settings.cache_read_weight).speed(0.1).range(0.0..=100.0)).changed() {
                self.settings_dirty = true;
            }
            ui.label("Output");
            if ui.add(egui::DragValue::new(&mut self.settings.output_weight).speed(0.1).range(0.0..=100.0)).changed() {
                self.settings_dirty = true;
            }
        });
        let total = self.settings.input_weight.max(0.0)
            + self.settings.cache_read_weight.max(0.0)
            + self.settings.output_weight.max(0.0);
        if total > f64::EPSILON {
            ui.small(format!(
                "Current mix: {:.1}% fresh input · {:.1}% cache read · {:.1}% output",
                self.settings.input_weight.max(0.0) / total * 100.0,
                self.settings.cache_read_weight.max(0.0) / total * 100.0,
                self.settings.output_weight.max(0.0) / total * 100.0,
            ));
        }
        ui.small("If a model/source does not publish cache-read pricing, ParetoWatch conservatively prices cache-read tokens at the normal input rate.");

        ui.add_space(14.0);
        if ui.add_enabled(self.settings_dirty, egui::Button::new("Save settings")).clicked() {
            self.save_and_push_settings();
            let _ = self.worker_tx.send(WorkerCommand::Refresh);
        }
        ui.label(format!("Config: {}", config_path().display()));
    }
}

impl eframe::App for ParetoWatchApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_ui_commands(ctx);
        self.handle_worker_messages();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_ui_commands(&ctx);
        self.handle_worker_messages();

        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.set_visible(&ctx, false);
        }

        // Pin the data-source footer to the bottom of the window so tall tab
        // bodies (chart + detail card + table) can never push it off-screen.
        egui::Panel::bottom("app_footer")
            .frame(
                egui::Frame::central_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.small("Pricing:");
                    ui.hyperlink_to("Surplus Intelligence", "https://www.surplusintelligence.ai/");
                    if let Some(updated) = self.price_snapshot.as_ref().and_then(|s| s.comparison_updated_at.as_deref()) {
                        ui.separator();
                        ui.small(format!("Market data updated {updated}"));
                    }
                });
                if let Some(err) = &self.last_price_error {
                    ui.small(format!("Last pricing error: {err}"));
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.top_bar(ui);
            match self.tab {
                Tab::Pareto => self.pareto_tab(ui),
                Tab::History => self.history_tab(ui),
                Tab::Alerts => self.alerts_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            }
        });
    }
}

impl Drop for ParetoWatchApp {
    fn drop(&mut self) {
        let _ = self.worker_tx.send(WorkerCommand::Quit);
    }
}

#[derive(Debug, Clone)]
struct JoinedPoint {
    model_id: String,
    model: String,
    creator: String,
    provider: String,
    benchmark_name: String,
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cost: f64,
    tokens_per_task: Option<f64>,
    token_profile: Option<String>,
    score: f64,
    live_market: bool,
    vision: bool,
}

/// Inputs that fully determine the derived Pareto view. egui redraws on every
/// hover/interaction, so anything derived from these must be cached instead of
/// recomputed per frame.
#[derive(Debug, Clone, PartialEq)]
struct ParetoCacheKey {
    data_version: u64,
    price_metric: PriceMetric,
    cost_basis: CostBasis,
    benchmark_source: BenchmarkSource,
    benchmark_metric: BenchmarkMetric,
    comparison_mode: ComparisonMode,
    common_scaffold: String,
    liquidity_filter: LiquidityFilter,
    modality_filter: ModalityFilter,
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
}

#[derive(Debug, Clone)]
struct ParetoView {
    joined: Vec<JoinedPoint>,
    frontier: Vec<JoinedPoint>,
    benchmarks_present: bool,
}

#[derive(Debug)]
struct ParetoCache {
    key: ParetoCacheKey,
    benchmarks_present: bool,
    filtered_quotes: Vec<Quote>,
    joined: Vec<JoinedPoint>,
    frontier: Vec<JoinedPoint>,
}

impl ParetoCache {
    fn view(&self) -> ParetoView {
        ParetoView {
            joined: self.joined.clone(),
            frontier: self.frontier.clone(),
            benchmarks_present: self.benchmarks_present,
        }
    }
}

fn price_to_plot_x(price: f64, log_scale: bool) -> f64 {
    if log_scale {
        // A tiny positive floor keeps genuinely-free/zero-priced observations
        // representable on a logarithmic chart without producing -inf.
        price.max(1e-6).log10()
    } else {
        price
    }
}

fn price_from_plot_x(x: f64, log_scale: bool) -> f64 {
    if log_scale { 10_f64.powf(x) } else { x }
}

fn format_price_tick(price: f64) -> String {
    if !price.is_finite() {
        return String::new();
    }
    let abs = price.abs();
    if abs >= 100.0 {
        format!("${price:.0}")
    } else if abs >= 10.0 {
        format!("${price:.1}")
    } else if abs >= 1.0 {
        format!("${price:.2}")
    } else if abs >= 0.01 {
        format!("${price:.3}")
    } else {
        format!("${price:.5}")
    }
}


fn format_compact_number(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{:.1}B", value / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}



fn joined_points(
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
                let per_million = q.price(
                    price_metric,
                    input_weight,
                    cache_read_weight,
                    output_weight,
                );
                let cost = match cost_basis {
                    CostBasis::PerMillion => per_million,
                    CostBasis::EstimatedPerTask => {
                        // Cost/task is intentionally only available for the blended
                        // rate. The benchmark contributes observed token volume; all
                        // dollar pricing comes from the current Surplus quote.
                        if price_metric != PriceMetric::Blended { continue; }
                        let Some(tokens) = b.tokens_per_task.filter(|tokens| tokens.is_finite() && *tokens > 0.0) else { continue };
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
                        vision: q.vision,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    out
}

fn pareto_frontier(points: &[JoinedPoint]) -> Vec<JoinedPoint> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.cost.total_cmp(&b.cost).then_with(|| b.score.total_cmp(&a.score)));
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

fn infer_creator(model: &str) -> String {
    let n = normalize(model);
    if n.starts_with("claude ")
        || n.starts_with("opus ")
        || n.starts_with("sonnet ")
        || n.starts_with("haiku ")
        || n.starts_with("fable ")
    { "Anthropic".into() }
    else if n.starts_with("gpt ") || n.starts_with("o1 ") || n.starts_with("o3 ") || n.starts_with("o4 ") { "OpenAI".into() }
    else if n.starts_with("gemini ") { "Google".into() }
    else if n.starts_with("grok ") { "xAI".into() }
    else if n.starts_with("deepseek ") { "DeepSeek".into() }
    else if n.starts_with("qwen") { "Alibaba".into() }
    else if n.starts_with("kimi ") { "Moonshot".into() }
    else if n.starts_with("glm ") { "Z.AI".into() }
    else if n.starts_with("minimax ") { "MiniMax".into() }
    else if n.starts_with("mimo ") { "Xiaomi".into() }
    else if n.starts_with("nemotron ") || n.starts_with("nvidia ") { "NVIDIA".into() }
    else if n.starts_with("mistral ") { "Mistral AI".into() }
    else if n.starts_with("arcee ") || n.starts_with("trinity ") { "Arcee AI".into() }
    else { String::new() }
}

/// Brand-ish colors for the major labs so model groups read at a glance on
/// the chart. Everyone else gets a deterministic hue from a hash of the name.
fn creator_color(creator: &str) -> egui::Color32 {
    let n = normalize(creator);
    let known: &[(&str, u8, u8, u8)] = &[
        ("anthropic", 224, 122, 95),
        ("openai", 46, 204, 147),
        ("google", 93, 156, 246),
        ("deepseek", 124, 148, 255),
        ("alibaba", 255, 139, 22),
        ("xai", 176, 182, 194),
        ("mistral ai", 250, 200, 60),
        ("moonshot", 158, 134, 255),
        ("z ai", 92, 186, 255),
        ("zai", 92, 186, 255),
        ("nvidia", 132, 193, 30),
        ("minimax", 236, 112, 197),
        ("xiaomi", 255, 133, 51),
        ("arcee ai", 38, 202, 217),
        ("meta", 96, 148, 255),
        ("microsoft", 130, 181, 255),
        ("cohere", 255, 205, 184),
        ("ai21", 30, 200, 200),
    ];
    if let Some(&(_, r, g, b)) = known.iter().find(|(name, _, _, _)| *name == n) {
        return egui::Color32::from_rgb(r, g, b);
    }
    if n.is_empty() {
        return egui::Color32::from_rgb(148, 156, 170);
    }
    // Golden-angle hue spread from a stable hash keeps colors distinct across
    // refreshes without maintaining a central registry.
    let mut hash: u32 = 2166136261;
    for byte in n.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    let hue = (hash % 360) as f32;
    let (r, g, b) = hsl_to_rgb(hue, 0.62, 0.62);
    egui::Color32::from_rgb(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0) % 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

fn group_label(creator: &str) -> &str {
    if creator.is_empty() { "Unknown" } else { creator }
}

/// Case-insensitive, whitespace-insensitive substring match used by the chart
/// search box. Matches the display name, model id, or creator/group.
fn pareto_search_matches(query: &str, p: &JoinedPoint) -> bool {
    let q = normalize(query).replace(' ', "");
    if q.is_empty() {
        return true;
    }
    [p.model.as_str(), p.model_id.as_str(), p.creator.as_str()]
        .iter()
        .any(|field| normalize(field).replace(' ', "").contains(&q))
}

fn json_shape(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().take(12).cloned().collect::<Vec<_>>();
            keys.sort();
            format!("object keys [{}]", keys.join(", "))
        }
        Value::Array(items) => format!("array len {}", items.len()),
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Number(_) => "number".into(),
        Value::String(v) => format!("string len {}", v.len()),
    }
}

fn semantic_alert_status(
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
    let filtered = quotes.iter()
        .filter_map(|quote| {
            alert.liquidity_filter.apply(
                quote,
                input_weight,
                cache_read_weight,
                output_weight,
            )
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
            let on_frontier = pareto_frontier(&joined).iter().any(|point| point.model_id == alert.model);
            if on_frontier { format!("on frontier · {:.1}", target.score) } else { format!("off frontier · {:.1}", target.score) }
        }
        AlertMode::CheapestAboveScore => {
            let cheapest = joined.iter()
                .filter(|point| point.score >= alert.score_threshold)
                .min_by(|a, b| a.cost.total_cmp(&b.cost));
            match cheapest {
                Some(best) if best.model_id == alert.model => format!("cheapest · ${:.4}{}", best.cost, alert.cost_basis.unit()),
                Some(best) => format!("leader: {} · ${:.4}{}", best.model, best.cost, alert.cost_basis.unit()),
                None => "no model clears score".into(),
            }
        }
        AlertMode::Threshold | AlertMode::AnyChange => String::new(),
    }
}

fn evaluate_alerts(
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
            if previous.live_market != quote.live_market || !quote_has_price_change(previous, quote, settings) {
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
                format!(" ({:+.2}%)", (new_blended - old_blended) / old_blended * 100.0)
            } else {
                String::new()
            };
            let source = if quote.live_market { "live market" } else { "price matrix fallback" };
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
            let source = if quote.live_market { "live market" } else { "price matrix fallback" };
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

fn prices_differ(a: f64, b: f64) -> bool {
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

fn detect_price_changes(
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
        let Some(old) = old_by_model.get(new.model.as_str()).copied() else { continue };
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
            source: if new.live_market { "live market" } else { "comparison/catalog" },
        });
    }
    changes.sort_by(|a, b| b.delta().abs().total_cmp(&a.delta().abs()));
    changes
}

fn first_number_path(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths.iter().find_map(|p| value_at_path(value, p).and_then(value_as_f64))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

fn first_string_path(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|p| {
        value_at_path(value, p)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn value_at_path<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(*k).and_then(Value::as_str).map(str::to_owned))
}


fn next_alert_id(alerts: &[AlertRule]) -> u64 {
    alerts.iter().map(|a| a.id).max().unwrap_or(0) + 1
}

fn config_path() -> PathBuf {
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.config_dir().join("config.json")
    } else {
        PathBuf::from("paretowatch-config.json")
    }
}

fn load_settings() -> Result<Settings> {
    let path = config_path();
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let raw: Value = serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let had_cache_weight = raw.get("cache_read_weight").is_some();
    let mut settings: Settings = serde_json::from_value(raw).with_context(|| format!("parse {}", path.display()))?;

    if !had_cache_weight {
        // v0.3 had a two-leg, unnormalized 1:3 input/output default. That is
        // backwards for agentic coding. Migrate only that exact old default to
        // the new agentic preset; preserve custom old mixes as no-cache weights.
        if (settings.input_weight - 1.0).abs() < 1e-9 && (settings.output_weight - 3.0).abs() < 1e-9 {
            settings.input_weight = 15.0;
            settings.cache_read_weight = 80.0;
            settings.output_weight = 5.0;
        } else {
            settings.cache_read_weight = 0.0;
        }
    }
    Ok(settings)
}

fn save_settings(settings: &Settings) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings)?;
    fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))
}

fn install_tray_event_handlers(ctx: egui::Context, ui_tx: Sender<UiCommand>) {
    let tx = ui_tx.clone();
    let repaint = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                let _ = tx.send(UiCommand::Toggle);
                repaint.request_repaint();
            }
            _ => {}
        }
    }));

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id.0.as_str();
        let cmd = match id {
            MENU_SHOW => Some(UiCommand::Show),
            MENU_REFRESH => Some(UiCommand::Refresh),
            MENU_QUIT => Some(UiCommand::Quit),
            _ => None,
        };
        if let Some(cmd) = cmd {
            let _ = ui_tx.send(cmd);
            ctx.request_repaint();
        }
    }));
}

fn create_tray() -> Result<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::with_id(MENU_SHOW, "Show ParetoWatch", true, None);
    let refresh = MenuItem::with_id(MENU_REFRESH, "Refresh now", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);
    menu.append_items(&[&show, &refresh, &separator, &quit])?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("ParetoWatch")
        .with_icon(make_tray_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;
    Ok(tray)
}

fn make_tray_icon() -> Result<tray_icon::Icon> {
    const W: u32 = 32;
    const H: u32 = 32;
    let mut rgba = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            let cx = x as f32 - 15.5;
            let cy = y as f32 - 15.5;
            let r2 = cx * cx + cy * cy;
            if r2 < 220.0 {
                rgba[i] = 28;
                rgba[i + 1] = 170;
                rgba[i + 2] = 220;
                rgba[i + 3] = 255;
            }
            // small rising frontier mark
            if (x >= 8 && x <= 11 && y >= 20 && y <= 23)
                || (x >= 14 && x <= 17 && y >= 14 && y <= 17)
                || (x >= 20 && x <= 23 && y >= 8 && y <= 11)
            {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, W, H).map_err(|e| anyhow!("tray icon: {e}"))
}

#[cfg(target_os = "linux")]
fn spawn_linux_tray() {
    std::thread::spawn(|| {
        if let Err(err) = gtk::init() {
            eprintln!("ParetoWatch tray: GTK init failed: {err}");
            return;
        }
        match create_tray() {
            Ok(_tray) => gtk::main(),
            Err(err) => eprintln!("ParetoWatch tray creation failed: {err:#}"),
        }
    });
}

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    spawn_linux_tray();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ParetoWatch")
            .with_inner_size([1050.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_taskbar(true)
            .with_visible(false),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "ParetoWatch",
        native_options,
        Box::new(|cc| Ok(Box::new(ParetoWatchApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "network diagnostic: cargo test real_feeds -- --ignored --nocapture"]
    fn real_feeds_composite_diagnostic() {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let mut sets: HashMap<BenchmarkSource, Vec<Benchmark>> = HashMap::new();
        for source in BenchmarkSource::remote_sources() {
            match fetch_benchmark_source(&client, source) {
                Ok(rows) => {
                    println!("{}: {} rows", source.short_label(), rows.len());
                    sets.insert(source, rows);
                }
                Err(err) => println!("{}: ERROR {err:#}", source.short_label()),
            }
        }
        sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, artificial_analysis_snapshot());
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        println!("--- top 20 composite ---");
        for row in composite.iter().take(20) {
            println!("{:.1}  {}", row.agentic_coding.unwrap(), clean_benchmark_display_name(&row.name));
            println!("       {}", row.name);
        }
        let deployment = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Deployment);
        println!("--- key models: capability vs deployment ---");
        for name in ["gpt 5 6 sol", "glm 5 3", "kimi k3", "qwen 3 8 2 4t a95b", "opus 5", "grok 4 6"] {
            let cap = composite.iter().find(|b| benchmark_model_key(&b.slug) == name).map(|b| b.agentic_coding.unwrap());
            let dep = deployment.iter().find(|b| benchmark_model_key(&b.slug) == name).map(|b| b.agentic_coding.unwrap());
            println!("{name}: capability {cap:?} · deployment {dep:?}");
        }
        println!("--- evidence breakdown ---");
        for name in ["gpt 5 6 sol", "glm 5 3", "kimi k3", "claude fable 5", "opus 5"] {
            if let Some(b) = composite.iter().find(|b| benchmark_model_key(&b.slug) == name) {
                println!("{name} [{}]", b.name);
            }
            // Raw per-source standings: rank/total and raw score on each board.
            for source in BenchmarkSource::consensus_sources() {
                if source.is_composite() { continue; }
                let rows = benchmarks_for_source(source, &sets, ComparisonMode::ModelCapability, "");
                if rows.is_empty() { continue; }
                let with_scores: Vec<(String, f64)> = rows
                    .iter()
                    .filter_map(|r| r.agentic_coding.filter(|s| s.is_finite()).map(|s| (benchmark_model_key(&r.slug), s)))
                    .collect();
                let Some(key) = with_scores.iter().find(|(k, _)| *k == name).map(|(k, _)| k.clone()) else { continue };
                let mut sorted: Vec<(String, f64)> = with_scores.clone();
                sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
                let rank = sorted.iter().position(|(k, _)| *k == key).unwrap_or(usize::MAX) + 1;
                let score = sorted.iter().find(|(k, _)| *k == key).unwrap().1;
                println!("    {:<22} {score:5.1}  #{rank}/{}", source.short_label(), sorted.len());
            }
        }
        println!("--- board field AA coverage ---");
        let anchor = crate::bench::scoring::aa_anchor_keys(&sets);
        let mut board_union: std::collections::HashSet<String> = std::collections::HashSet::new();
        for source in BenchmarkSource::remote_sources() {
            if crate::bench::scoring::prior_cohort_source(source) {
                for row in benchmarks_for_source(source, &sets, ComparisonMode::ModelCapability, "") {
                    board_union.insert(benchmark_model_key(&row.slug));
                }
            }
        }
        let aa_rows = benchmarks_for_source(BenchmarkSource::ArtificialAnalysisSnapshot, &sets, ComparisonMode::ModelCapability, "");
        let aa_keys: std::collections::HashSet<String> = aa_rows.iter().map(|r| benchmark_model_key(&r.slug)).collect();
        let mut aa_scores: Vec<(String, f64)> = aa_rows.iter()
            .filter_map(|r| r.agentic_coding.filter(|s| s.is_finite()).map(|s| (benchmark_model_key(&r.slug), s)))
            .collect();
        aa_scores.retain(|(k, _)| board_union.contains(k));
        let (aa_map, _) = crate::bench::scoring::anchored_aa_percentiles(aa_rows.iter().filter_map(|r| r.agentic_coding.filter(|s| s.is_finite()).map(|s| (benchmark_model_key(&r.slug), s))), &board_union).unwrap_or((HashMap::new(), 0));
        for source in BenchmarkSource::remote_sources() {
            let rows = benchmarks_for_source(source, &sets, ComparisonMode::ModelCapability, "");
            if rows.is_empty() { continue; }
            let keys: Vec<String> = rows.iter().map(|r| benchmark_model_key(&r.slug)).collect();
            let covered = keys.iter().filter(|k| anchor.as_ref().is_some_and(|a| a.contains(*k))).count();
            let mut field_q: Vec<f64> = keys.iter().filter_map(|k| aa_map.get(k).copied()).collect();
            field_q.sort_by(|a, b| a.total_cmp(b));
            let q_desc: Vec<String> = field_q.iter().rev().take(8).map(|q| format!("{q:.0}")).collect();
            println!(
                "{:<22} rows {:>3} · AA-covered {:>3} · calibrates={} · top field q: {}",
                source.short_label(),
                keys.len(),
                covered,
                field_q.len() >= 10,
                q_desc.join(" "),
            );
        }
        let _ = aa_scores;
    }
    use crate::testfix::test_quote;

    #[test]
    fn log_price_axis_round_trips_prices() {
        for price in [0.0001, 0.01, 0.5, 1.0, 10.0, 250.0] {
            let x = price_to_plot_x(price, true);
            let round_trip = price_from_plot_x(x, true);
            assert!((round_trip - price).abs() < price * 1e-10 + 1e-12);
        }
    }

    #[test]
    fn any_change_detection_ignores_source_switches() {
        let settings = Settings::default();
        let old = PriceSnapshot {
            quotes: vec![test_quote("model-a", 1.0, true)],
            fetched_at: Utc::now(), comparison_updated_at: None,
            market_overlay_count: 1, market_error: None, base_source: "test".into(),
        };
        let changed = PriceSnapshot {
            quotes: vec![test_quote("model-a", 0.9, true)],
            fetched_at: Utc::now(), comparison_updated_at: None,
            market_overlay_count: 1, market_error: None, base_source: "test".into(),
        };
        assert_eq!(detect_price_changes(&old, &changed, &settings).len(), 1);
        let fallback = PriceSnapshot {
            quotes: vec![test_quote("model-a", 0.8, false)],
            fetched_at: Utc::now(), comparison_updated_at: None,
            market_overlay_count: 0, market_error: Some("test".into()), base_source: "test".into(),
        };
        assert!(detect_price_changes(&changed, &fallback, &settings).is_empty());
    }

    #[test]
    fn estimated_task_cost_uses_current_surplus_blend_and_benchmark_volume() {
        let q = test_quote("gpt-5.6-sol", 2.0, true);
        let mut b = score_benchmark("gpt-5.6-sol", 72.7, Some("mini-SWE-agent".into()), Some("max".into()), BenchmarkKind::Model);
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
            input: 1.0, output: 2.0, cache_read: None, cost: 1.5,
            tokens_per_task: None, token_profile: None,
            score: 80.0, live_market: true, vision: true,
        };
        assert!(pareto_search_matches("opus", &p));
        assert!(pareto_search_matches("CLAUDE OPUS", &p));
        assert!(pareto_search_matches("claudeopus5", &p));
        assert!(pareto_search_matches("anthropic", &p));
        assert!(!pareto_search_matches("gemini", &p));
        assert!(pareto_search_matches("", &p));
    }

    #[test]
    fn creator_colors_are_stable_and_distinguish_known_groups() {
        // Known labs get fixed brand colors; empty groups get the neutral gray.
        assert_eq!(creator_color("Anthropic"), creator_color("anthropic"));
        assert_ne!(creator_color("Anthropic"), creator_color("OpenAI"));
        assert_ne!(creator_color("Anthropic"), creator_color(""));
        // Unknown creators hash deterministically.
        assert_eq!(creator_color("Some New Lab"), creator_color("Some New Lab"));
    }

    #[test]
    fn aa_snapshot_covers_current_deepseek_and_glm_models() {
        let rows = artificial_analysis_snapshot();
        let deepseek_release = rows.iter().find(|b| benchmark_model_key(&b.slug) == "deepseek v4 flash 0731").unwrap();
        let deepseek_base = rows.iter().find(|b| benchmark_model_key(&b.slug) == "deepseek v4 flash").unwrap();
        let glm = rows.iter().find(|b| benchmark_model_key(&b.slug) == "glm 5 3").unwrap();
        assert_eq!(deepseek_release.agentic_coding, Some(52.0));
        assert_eq!(deepseek_base.agentic_coding, Some(42.0));
        assert_eq!(glm.agentic_coding, Some(60.0));
    }

}
