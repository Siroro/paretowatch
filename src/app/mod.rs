//! Application shell: owns all UI state and the worker channel endpoints,
//! pumps messages into that state, and delegates each tab's view to its
//! sibling tab module.

mod alerts_tab;
mod pareto_tab;
mod settings_tab;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use chrono::Utc;
use eframe::egui;

use crate::alerts::{
    ListingChange, PriceChangeEvent, detect_delisted, detect_price_changes, frontier_changes,
    semantic_alert_status,
};
use crate::artificial_analysis_snapshot::artificial_analysis_snapshot;
use crate::bench::{
    BenchmarkConsensus, available_scaffolds, benchmark_consensus_for_quote, benchmarks_for_source,
};
use crate::fetch::start_worker;
use crate::history;
use crate::history::track::blended_series;
use crate::notifications::{NotificationLog, SharedNotifications, notify};
use crate::pareto::{ParetoCache, ParetoCacheKey, joined_points, pareto_frontier};
use crate::settings_store::{load_settings, save_settings};
#[cfg(not(target_os = "linux"))]
use crate::tray::create_tray;
use crate::tray::{UiCommand, install_tray_event_handlers};
use crate::types::{
    ANY_MODEL, AlertDirection, AlertMode, AlertRule, Benchmark, BenchmarkMetric, BenchmarkSource,
    ComparisonMode, CostBasis, LiquidityFilter, ModalityFilter, MoveDirection, PriceMetric,
    PriceSnapshot, Quote, Settings, default_common_scaffold,
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
    alert_move_direction: MoveDirection,
    alert_min_move_pct: f64,
    alert_discount_pct: f64,
    alert_healthy_floor: u64,
    semantic_alert_state: HashMap<u64, bool>,
    /// Last-known frontier membership (model id → display name) per wildcard
    /// frontier alert, so "any model" rules can diff the set instead of a
    /// single bool.
    semantic_frontier_state: HashMap<u64, HashMap<String, String>>,
    /// Last-known leader (cheapest model above the score floor, if any) per
    /// wildcard cheapest rule.
    semantic_leader_state: HashMap<u64, Option<String>>,
    /// Toast history shared with the fetch worker; every notification path
    /// appends here so the Alerts tab can replay what was fired.
    notifications: SharedNotifications,
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
        let notifications: SharedNotifications = Arc::new(Mutex::new(NotificationLog::default()));
        let (worker_tx, worker_rx) = start_worker(
            cc.egui_ctx.clone(),
            settings.clone(),
            Arc::clone(&notifications),
        );
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
            alert_move_direction: MoveDirection::Any,
            alert_min_move_pct: 0.0,
            alert_discount_pct: 50.0,
            alert_healthy_floor: 0,
            semantic_alert_state: HashMap::new(),
            semantic_frontier_state: HashMap::new(),
            semantic_leader_state: HashMap::new(),
            notifications,
            data_version: 0,
            scaffold_cache: None,
            pareto_cache: None,
            alert_status_cache: HashMap::new(),
            consensus_cache: None,
            status: "Starting…".into(),
            last_price_error: None,
            is_visible: true,
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
        let mut prices_arrived = false;
        let mut first_price_poll = false;
        let mut delisted: Vec<ListingChange> = Vec::new();
        loop {
            match self.worker_rx.try_recv() {
                Ok(WorkerMessage::Prices(Ok(snapshot))) => {
                    first_price_poll = self.price_snapshot.is_none();
                    if let Some(previous) = &self.price_snapshot {
                        for change in detect_price_changes(previous, &snapshot, &self.settings) {
                            self.recent_changes.push_front(change);
                        }
                        while self.recent_changes.len() > 80 {
                            self.recent_changes.pop_back();
                        }
                        delisted = detect_delisted(previous, &snapshot, &self.settings);
                    }
                    if self.alert_model.is_empty()
                        && let Some(q) = snapshot.quotes.first()
                    {
                        self.alert_model = q.model.clone();
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
                    prices_arrived = true;
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
            if prices_arrived {
                // Floor rules compare the new blended price against the
                // history log's all-time minimum, so they must run before this
                // poll is recorded into that log.
                self.evaluate_floor_alerts();
            }
            // Frontier/cheapest alerts depend on benchmark boards too, so a
            // board refresh (every six hours) must re-evaluate them instead
            // of waiting for the next price poll.
            self.evaluate_semantic_alerts();
            // Long-term history: diff each delivered poll against the last
            // recorded state. Writes only when something actually changed,
            // and reports models seen for the first time ever.
            let newly_listed = self
                .price_snapshot
                .as_ref()
                .map(|snapshot| self.history.record(snapshot, Utc::now()))
                .unwrap_or_default();
            if prices_arrived {
                // The session's first poll only seeds the baseline: it would
                // otherwise replay every model added since the last run (or
                // the whole catalog on a fresh install) as notifications.
                self.evaluate_listing_alerts(&newly_listed, &delisted, !first_price_poll);
            }
        }
    }

    /// All-time-low rules: fire when the current blended price undercuts
    /// every value in the persistent history. Self-resetting by construction
    /// — once the new low is recorded it becomes the floor, so the same price
    /// cannot re-fire. The margin (one quantization step) keeps sub-step
    /// wobble from oscillating around the minimum.
    fn evaluate_floor_alerts(&mut self) {
        let Some(snapshot) = self.price_snapshot.as_ref() else {
            return;
        };
        let notifications = Arc::clone(&self.notifications);
        for alert in &self.settings.alerts {
            if !alert.enabled || alert.mode != AlertMode::PriceFloor {
                continue;
            }
            let Some(quote) = snapshot.quotes.iter().find(|q| q.model == alert.model) else {
                continue;
            };
            let Some(series) = self.history.series(&alert.model) else {
                continue;
            };
            let current = quote.price(
                PriceMetric::Blended,
                self.settings.input_weight,
                self.settings.cache_read_weight,
                self.settings.output_weight,
            );
            let Some(previous_low) = blended_series(
                series,
                self.settings.input_weight,
                self.settings.cache_read_weight,
                self.settings.output_weight,
            )
            .into_iter()
            .map(|(_, value)| value)
            .fold(None, |acc: Option<f64>, value| match acc {
                Some(min) if min <= value => Some(min),
                _ => Some(value),
            }) else {
                continue;
            };
            const FLOOR_MARGIN: f64 = 0.001; // one history quantization step, $/M
            if current >= previous_low - FLOOR_MARGIN {
                continue;
            }
            let pct = if previous_low.abs() > f64::EPSILON {
                format!(
                    " · {:.2}% below",
                    (previous_low - current) / previous_low * 100.0
                )
            } else {
                String::new()
            };
            notify(
                &notifications,
                &format!("▽ {} all-time low", quote.display_name),
                &format!(
                    "Blended ${:.6}/1M — previous low ${:.6}/1M{pct}\nInput ${:.4} · Output ${:.4}",
                    current, previous_low, quote.input, quote.output,
                ),
            );
        }
    }

    /// Feed-wide rules: new models appearing in the price feed and models
    /// disappearing from it. Event-driven, so unlike the edge-triggered
    /// rules there is no per-alert state to maintain.
    fn evaluate_listing_alerts(
        &self,
        newly_listed: &[(String, String)],
        delisted: &[ListingChange],
        notify_enabled: bool,
    ) {
        if !notify_enabled {
            return;
        }
        let watch_new = self
            .settings
            .alerts
            .iter()
            .any(|a| a.enabled && a.mode == AlertMode::NewModelListed);
        let watch_delisted = self
            .settings
            .alerts
            .iter()
            .any(|a| a.enabled && a.mode == AlertMode::ModelDelisted);

        if watch_new && !newly_listed.is_empty() {
            let quotes = self.price_snapshot.as_ref().map(|s| &s.quotes);
            let lookup =
                |slug: &str| quotes.and_then(|quotes| quotes.iter().find(|q| q.model == slug));
            let (summary, body) = if newly_listed.len() == 1 {
                let (slug, display) = &newly_listed[0];
                let detail = lookup(slug).map_or_else(String::new, |quote| {
                    let source = if quote.live_market {
                        "live market"
                    } else {
                        "price matrix"
                    };
                    format!(
                        "{} · {}\nInput ${:.4} · Output ${:.4} · Blend ${:.4}/1M",
                        quote.creator,
                        source,
                        quote.input,
                        quote.output,
                        quote.price(
                            PriceMetric::Blended,
                            self.settings.input_weight,
                            self.settings.cache_read_weight,
                            self.settings.output_weight,
                        ),
                    )
                });
                (format!("✚ New model listed: {display}"), detail)
            } else {
                let lines = newly_listed
                    .iter()
                    .map(|(slug, display)| {
                        let blend = lookup(slug).map_or(f64::NAN, |quote| {
                            quote.price(
                                PriceMetric::Blended,
                                self.settings.input_weight,
                                self.settings.cache_read_weight,
                                self.settings.output_weight,
                            )
                        });
                        format!("{display} · blend ${blend:.4}/1M")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (format!("✚ {} new models listed", newly_listed.len()), lines)
            };
            notify(&self.notifications, &summary, &body);
        }

        if watch_delisted && !delisted.is_empty() {
            let (summary, body) = if delisted.len() == 1 {
                (
                    format!("✖ Model delisted: {}", delisted[0].display_name),
                    format!(
                        "No longer in the price feed · last blend ${:.4}/1M",
                        delisted[0].last_blended
                    ),
                )
            } else {
                let lines = delisted
                    .iter()
                    .map(|d| format!("{} · last blend ${:.4}/1M", d.display_name, d.last_blended))
                    .collect::<Vec<_>>()
                    .join("\n");
                (format!("✖ {} models delisted", delisted.len()), lines)
            };
            notify(&self.notifications, &summary, &body);
        }
    }

    /// Scaffold list only changes when benchmark data changes, but computing it
    /// walks every row of every leaderboard; never do that per UI frame.
    fn cached_scaffolds(&mut self) -> Vec<String> {
        if let Some((version, options)) = &self.scaffold_cache
            && *version == self.data_version
        {
            return options.clone();
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
        if let Some(cache) = &self.pareto_cache
            && cache.key == key
        {
            return Some(Arc::clone(cache));
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
            weights,
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
        if let Some((version, mode, scaffold, cached_id, consensus)) = &self.consensus_cache
            && *version == self.data_version
            && *mode == self.comparison_mode
            && *scaffold == self.common_scaffold
            && cached_id == model_id
        {
            return consensus.clone();
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
    /// and frontier, and floor statuses walk the history series, so both are
    /// cached per alert until prices, boards, or the workload weights
    /// change; threshold/discount/seller rows are a cheap quote lookup and
    /// are formatted fresh each call.
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
            let status = if alert.mode.benchmark_dependent() || alert.mode == AlertMode::PriceFloor
            {
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
                        let text = if alert.mode == AlertMode::PriceFloor {
                            self.floor_alert_status(alert, quotes)
                        } else {
                            semantic_alert_status(
                                alert,
                                quotes,
                                &self.benchmark_sets,
                                input_weight,
                                cache_read_weight,
                                output_weight,
                            )
                        };
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
            } else if alert.mode.feed_wide() {
                format!("{} models tracked", quotes.len())
            } else if alert.mode == AlertMode::Discount {
                quotes
                    .iter()
                    .find(|q| q.model == alert.model)
                    .and_then(|q| q.discount_pct)
                    .map(|pct| format!("{pct:.0}% off"))
                    .unwrap_or_else(|| "—".into())
            } else if alert.mode == AlertMode::SellerHealth {
                quotes
                    .iter()
                    .find(|q| q.model == alert.model)
                    .and_then(|q| q.healthy_seller_count)
                    .map(|count| format!("{count} healthy"))
                    .unwrap_or_else(|| "—".into())
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

    /// Status text for an all-time-low rule: current blended price beside
    /// the lowest value the history log has ever recorded for the model.
    fn floor_alert_status(&self, alert: &AlertRule, quotes: &[Quote]) -> String {
        let (input_weight, cache_read_weight, output_weight) = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
        );
        let Some(quote) = quotes.iter().find(|q| q.model == alert.model) else {
            return "—".into();
        };
        let current = quote.price(
            PriceMetric::Blended,
            input_weight,
            cache_read_weight,
            output_weight,
        );
        let Some(series) = self.history.series(&alert.model) else {
            return format!("${current:.4} · no history yet");
        };
        let low = blended_series(series, input_weight, cache_read_weight, output_weight)
            .into_iter()
            .map(|(_, value)| value)
            .fold(f64::INFINITY, f64::min);
        if low.is_finite() {
            format!("${current:.4} · low ${low:.4}")
        } else {
            format!("${current:.4} · no history yet")
        }
    }

    fn evaluate_semantic_alerts(&mut self) {
        let Some(snapshot) = self.price_snapshot.clone() else {
            return;
        };
        let notifications = Arc::clone(&self.notifications);
        let alerts = self.settings.alerts.clone();
        for alert in alerts {
            if !alert.enabled || !alert.mode.benchmark_dependent() {
                self.semantic_alert_state.remove(&alert.id);
                self.semantic_frontier_state.remove(&alert.id);
                self.semantic_leader_state.remove(&alert.id);
                continue;
            }
            let wildcard = alert.model == ANY_MODEL;
            if !wildcard {
                self.semantic_frontier_state.remove(&alert.id);
                self.semantic_leader_state.remove(&alert.id);
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
                (
                    self.settings.input_weight,
                    self.settings.cache_read_weight,
                    self.settings.output_weight,
                ),
            );

            if wildcard {
                if alert.mode == AlertMode::CheapestAboveScore {
                    let leader = joined
                        .iter()
                        .filter(|point| point.score >= alert.score_threshold)
                        .min_by(|a, b| a.cost.total_cmp(&b.cost));
                    let current = leader.map(|best| best.model_id.clone());
                    let Some(previous) = self.semantic_leader_state.remove(&alert.id) else {
                        // First sighting only seeds the baseline leader.
                        self.semantic_leader_state.insert(alert.id, current);
                        continue;
                    };
                    if previous != current {
                        let context = format!(
                            "{} · {} · {}",
                            alert.benchmark_source.short_label(),
                            alert.comparison_mode.label(),
                            alert.liquidity_filter.label(),
                        );
                        let (summary, body) = match (previous.as_deref(), leader) {
                            (Some(_), Some(best)) => (
                                format!(
                                    "★ {} took the cheapest-≥{:.0} lead",
                                    best.model, alert.score_threshold
                                ),
                                format!(
                                    "{}: {:.1} · {} ${:.4}{}\nPrevious: {}\n{context}",
                                    alert.benchmark_source.short_label(),
                                    best.score,
                                    alert.metric.label(),
                                    best.cost,
                                    alert.cost_basis.unit(),
                                    previous.as_deref().unwrap_or("?"),
                                ),
                            ),
                            (None, Some(best)) => (
                                format!(
                                    "★ {} is now cheapest ≥ {:.0}",
                                    best.model, alert.score_threshold
                                ),
                                format!(
                                    "{}: {:.1} · {} ${:.4}{}\nNo model cleared the score before.\n{context}",
                                    alert.benchmark_source.short_label(),
                                    best.score,
                                    alert.metric.label(),
                                    best.cost,
                                    alert.cost_basis.unit(),
                                ),
                            ),
                            (Some(old), None) => (
                                format!(
                                    "◇ No model clears score ≥ {:.0} anymore",
                                    alert.score_threshold
                                ),
                                format!("Previous leader: {old}\n{context}"),
                            ),
                            (None, None) => unreachable!("leader unchanged"),
                        };
                        notify(&notifications, &summary, &body);
                    }
                    self.semantic_leader_state.insert(alert.id, current);
                    continue;
                }

                let frontier = pareto_frontier(&joined);
                let membership: HashMap<String, String> = frontier
                    .iter()
                    .map(|p| (p.model_id.clone(), p.model.clone()))
                    .collect();
                let Some(previous) = self.semantic_frontier_state.remove(&alert.id) else {
                    // First sighting only seeds the baseline set.
                    self.semantic_frontier_state.insert(alert.id, membership);
                    continue;
                };
                let diff = frontier_changes(&previous, &frontier);
                let fired = match alert.mode {
                    AlertMode::EntersFrontier => !diff.entered.is_empty(),
                    AlertMode::LeavesFrontier => !diff.exited.is_empty(),
                    _ => false,
                };
                if fired {
                    let context = format!(
                        "{} · {} · {}",
                        alert.benchmark_source.short_label(),
                        alert.comparison_mode.label(),
                        alert.liquidity_filter.label(),
                    );
                    const MAX_LINES: usize = 6;
                    let (summary, body) = match alert.mode {
                        AlertMode::EntersFrontier => {
                            let summary = if diff.entered.len() == 1 {
                                format!("◆ {} entered the Pareto frontier", diff.entered[0].model)
                            } else {
                                format!(
                                    "◆ {} models entered the Pareto frontier",
                                    diff.entered.len()
                                )
                            };
                            let mut lines = diff
                                .entered
                                .iter()
                                .take(MAX_LINES)
                                .map(|p| {
                                    format!(
                                        "{} · score {:.1} · {} ${:.4}{}",
                                        p.model,
                                        p.score,
                                        alert.metric.label(),
                                        p.cost,
                                        alert.cost_basis.unit()
                                    )
                                })
                                .collect::<Vec<_>>();
                            if diff.entered.len() > MAX_LINES {
                                lines
                                    .push(format!("… and {} more", diff.entered.len() - MAX_LINES));
                            }
                            (summary, format!("{}\n{context}", lines.join("\n")))
                        }
                        AlertMode::LeavesFrontier => {
                            let summary = if diff.exited.len() == 1 {
                                format!("◇ {} left the Pareto frontier", diff.exited[0].1)
                            } else {
                                format!("{} models left the Pareto frontier", diff.exited.len())
                            };
                            let mut lines = diff
                                .exited
                                .iter()
                                .take(MAX_LINES)
                                .map(
                                    |(id, name)| match joined.iter().find(|p| &p.model_id == id) {
                                        Some(p) => format!(
                                            "{name} · score {:.1} · {} ${:.4}{}",
                                            p.score,
                                            alert.metric.label(),
                                            p.cost,
                                            alert.cost_basis.unit()
                                        ),
                                        // Vanished from the join entirely (delisted or
                                        // no longer benchmark-matched), not just dominated.
                                        None => format!("{name} · no longer matched"),
                                    },
                                )
                                .collect::<Vec<_>>();
                            if diff.exited.len() > MAX_LINES {
                                lines.push(format!("… and {} more", diff.exited.len() - MAX_LINES));
                            }
                            (summary, format!("{}\n{context}", lines.join("\n")))
                        }
                        _ => unreachable!("wildcard frontier is only entered for frontier modes"),
                    };
                    notify(&notifications, &summary, &body);
                }
                self.semantic_frontier_state.insert(alert.id, membership);
                continue;
            }

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
                AlertMode::Threshold
                | AlertMode::AnyChange
                | AlertMode::PriceFloor
                | AlertMode::Discount
                | AlertMode::SellerHealth
                | AlertMode::NewModelListed
                | AlertMode::ModelDelisted => continue,
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
                    AlertMode::Threshold
                    | AlertMode::AnyChange
                    | AlertMode::PriceFloor
                    | AlertMode::Discount
                    | AlertMode::SellerHealth
                    | AlertMode::NewModelListed
                    | AlertMode::ModelDelisted => unreachable!(),
                };
                notify(&notifications, &summary, &body);
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
        self.semantic_frontier_state
            .retain(|id, _| valid_ids.contains(id));
        self.semantic_leader_state
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
