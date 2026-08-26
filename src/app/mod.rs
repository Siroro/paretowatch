//! Application shell: owns all UI state and the worker channel endpoints,
//! pumps messages into that state, and delegates each tab's view to its
//! sibling tab module.

mod alerts_tab;
mod pareto_tab;
mod settings_tab;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use chrono::Utc;
use eframe::egui;
use notify_rust::Notification;

use crate::alerts::{PriceChangeEvent, detect_price_changes, semantic_alert_status};
use crate::artificial_analysis_snapshot::artificial_analysis_snapshot;
use crate::bench::{
    BenchmarkConsensus, available_scaffolds, benchmark_consensus_for_quote, benchmarks_for_source,
};
use crate::fetch::start_worker;
use crate::history;
use crate::pareto::{ParetoCache, ParetoCacheKey, joined_points, pareto_frontier};
use crate::settings_store::{load_settings, save_settings};
use crate::tray::{UiCommand, create_tray, install_tray_event_handlers};
use crate::types::{
    AlertDirection, AlertMode, Benchmark, BenchmarkMetric, BenchmarkSource, ComparisonMode,
    CostBasis, LiquidityFilter, ModalityFilter, PriceMetric, PriceSnapshot, Quote, Settings,
    default_common_scaffold,
};
use crate::widgets;
use crate::worker::{WorkerCommand, WorkerMessage};

#[cfg(not(target_os = "linux"))]
use tray_icon::TrayIcon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Pareto,
    History,
    Alerts,
    Settings,
}

pub(crate) struct ParetoWatchApp {
    settings: Settings,
    price_snapshot: Option<PriceSnapshot>,
    benchmark_sets: HashMap<BenchmarkSource, Vec<Benchmark>>,
    benchmark_errors: HashMap<BenchmarkSource, String>,
    history: history::HistoryTracker,
    history_ui: history::HistoryUiState,
    recent_changes: VecDeque<PriceChangeEvent>,
    widgets: widgets::PriceWidgetManager,
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
    pareto_cache: Option<Arc<ParetoCache>>,
    /// Status text per alert id for the Alerts tab, keyed to the data version
    /// and workload weights it was computed at.
    alert_status_cache: HashMap<u64, (u64, f64, f64, f64, String)>,
    consensus_cache: Option<(
        u64,
        ComparisonMode,
        String,
        String,
        Option<BenchmarkConsensus>,
    )>,
    status: String,
    last_price_error: Option<String>,
    is_visible: bool,
    quitting: bool,
    settings_dirty: bool,
    #[cfg(not(target_os = "linux"))]
    _tray: Option<TrayIcon>,
}

impl ParetoWatchApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
                sets.insert(
                    BenchmarkSource::ArtificialAnalysisSnapshot,
                    artificial_analysis_snapshot(),
                );
                sets
            },
            benchmark_errors: HashMap::new(),
            history: history::HistoryTracker::open(&history::history_log_path()),
            history_ui: history::HistoryUiState::default(),
            recent_changes: VecDeque::new(),
            widgets: widgets::PriceWidgetManager::default(),
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
            alert_status_cache: HashMap::new(),
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
                        format!(
                            "{} priced models · {} live market quotes",
                            snapshot.quotes.len(),
                            snapshot.market_overlay_count
                        )
                    } else {
                        format!("{} priced models · comparison feed", snapshot.quotes.len())
                    };
                    self.price_snapshot = Some(snapshot);
                    self.last_price_error = None;
                    self.data_version += 1;
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
        if data_dirty {
            // Frontier/cheapest alerts depend on benchmark boards too, so a
            // board refresh (every six hours) must re-evaluate them instead
            // of waiting for the next price poll.
            self.evaluate_semantic_alerts();
            // Long-term history: diff each delivered poll against the last
            // recorded state. Writes only when something actually changed.
            if let Some(snapshot) = &self.price_snapshot {
                self.history.record(
                    snapshot,
                    &self.benchmark_sets,
                    self.data_version,
                    Utc::now(),
                );
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
    /// only when data or a relevant control actually changed. The `Arc` clone
    /// keeps per-frame handoff cheap — the joined points are never copied.
    fn ensure_pareto_view(&mut self) -> Option<Arc<ParetoCache>> {
        let key = self.pareto_cache_key();
        if let Some(cache) = &self.pareto_cache {
            if cache.key == key {
                return Some(Arc::clone(cache));
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
        self.pareto_cache = Some(Arc::new(ParetoCache {
            key,
            benchmarks_present,
            filtered_quotes,
            joined,
            frontier,
        }));
        self.pareto_cache.as_ref().map(Arc::clone)
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

    /// "Current" column text for every alert row, aligned with
    /// `settings.alerts`. Semantic statuses re-run the whole benchmark join
    /// and frontier, so they are cached per alert until prices, boards, or
    /// the workload weights change; threshold/any-change rows are a cheap
    /// quote lookup and are formatted fresh each call.
    fn cached_alert_statuses(&mut self) -> Vec<String> {
        let data_version = self.data_version;
        let (input_weight, cache_read_weight, output_weight) = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
        );
        let quotes: &[Quote] = self
            .price_snapshot
            .as_ref()
            .map(|s| s.quotes.as_slice())
            .unwrap_or(&[]);
        let mut statuses = Vec::with_capacity(self.settings.alerts.len());
        for alert in &self.settings.alerts {
            let status = if alert.mode.benchmark_dependent() {
                let cached = self
                    .alert_status_cache
                    .get(&alert.id)
                    .filter(|(version, iw, cw, ow, _)| {
                        *version == data_version
                            && (*iw, *cw, *ow) == (input_weight, cache_read_weight, output_weight)
                    })
                    .map(|(_, _, _, _, text)| text.clone());
                match cached {
                    Some(text) => text,
                    None => {
                        let text = semantic_alert_status(
                            alert,
                            quotes,
                            &self.benchmark_sets,
                            input_weight,
                            cache_read_weight,
                            output_weight,
                        );
                        self.alert_status_cache.insert(
                            alert.id,
                            (
                                data_version,
                                input_weight,
                                cache_read_weight,
                                output_weight,
                                text.clone(),
                            ),
                        );
                        text
                    }
                }
            } else {
                quotes
                    .iter()
                    .find(|q| q.model == alert.model)
                    .map(|q| {
                        format!(
                            "${:.4}",
                            q.price(alert.metric, input_weight, cache_read_weight, output_weight,)
                        )
                    })
                    .unwrap_or_else(|| "—".into())
            };
            statuses.push(status);
        }
        statuses
    }

    fn evaluate_semantic_alerts(&mut self) {
        let Some(snapshot) = self.price_snapshot.clone() else {
            return;
        };
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
            if benchmarks.is_empty() {
                continue;
            }
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
            let Some(target) = joined.iter().find(|point| point.model_id == alert.model) else {
                continue;
            };
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
                        format!(
                            "★ {} is now cheapest above {:.1}",
                            target.model, alert.score_threshold
                        ),
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
        let valid_ids = self
            .settings
            .alerts
            .iter()
            .map(|a| a.id)
            .collect::<std::collections::HashSet<_>>();
        self.semantic_alert_state
            .retain(|id, _| valid_ids.contains(id));
        self.alert_status_cache
            .retain(|id, _| valid_ids.contains(id));
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
}

impl ParetoWatchApp {
    fn history_tab(&mut self, ui: &mut egui::Ui) {
        history::ui::show(
            ui,
            &self.settings,
            self.price_snapshot.as_ref(),
            &self.history,
            &mut self.history_ui,
        );
    }
}

impl eframe::App for ParetoWatchApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_ui_commands(ctx);
        self.handle_worker_messages();
        // Runs here (not in ui) so floating price widgets keep updating while
        // the main window is hidden in the tray.
        if self.widgets.refresh(
            self.price_snapshot.as_ref(),
            &self.settings,
            self.liquidity_filter,
        ) {
            self.widgets.request_repaints(ctx);
        }
        self.widgets.prune_closed();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_ui_commands(&ctx);
        self.handle_worker_messages();

        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.set_visible(&ctx, false);
        }

        // Deferred widget viewports must be re-registered every root pass.
        self.widgets.show(&ctx);

        // Pin the data-source footer to the bottom of the window so tall tab
        // bodies (chart + detail card + table) can never push it off-screen.
        egui::Panel::bottom("app_footer")
            .frame(
                egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.small("Pricing:");
                    ui.hyperlink_to(
                        "Surplus Intelligence",
                        "https://www.surplusintelligence.ai/",
                    );
                    if let Some(updated) = self
                        .price_snapshot
                        .as_ref()
                        .and_then(|s| s.comparison_updated_at.as_deref())
                    {
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

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Transparent clear lets the floating price widgets be semi-transparent.
        // Every root-window surface is repainted opaquely by its panels each
        // frame, so this is invisible on the main window.
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

impl Drop for ParetoWatchApp {
    fn drop(&mut self) {
        let _ = self.worker_tx.send(WorkerCommand::Quit);
    }
}
