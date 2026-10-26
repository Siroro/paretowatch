//! Session UI state, persisted across restarts: which tab and board the
//! user was on, every chart/table filter and sort, the legend chips toggled
//! off, the selected models, and the set of pinned price widgets.
//!
//! `config.json` keeps deliberate settings (poll interval, weights, alerts);
//! this file (`ui_state.json` beside it) restores the working view so a
//! reopen lands where the user left off. Search boxes, the half-composed
//! alert form, and per-frame caches are deliberately NOT persisted — they
//! are transient by nature, and a stale search filter would hide models on
//! reopen with no visible reason.
//!
//! Like the benchmark cache, this is best-effort: a missing or corrupt file
//! falls back to defaults, and writes happen only when something actually
//! changed.

use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::models_tab::{ModelsCacheFilter, ModelsSort, ModelsSource};
use super::{ActivityTab, NewListingsWindow, ParetoWatchApp, Tab};
use crate::notifications::NotificationKind;
use crate::types::{
    BenchmarkMetric, BenchmarkSource, ComparisonMode, CostBasis, LiquidityFilter, ModalityFilter,
    PriceMetric, SpeedFilter, UncensoredFilter,
};

const UI_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct UiState {
    version: u32,
    tab: Tab,
    activity_tab: ActivityTab,
    activity_kind_filter: Option<NotificationKind>,
    new_listings_window: NewListingsWindow,
    // Pareto chart
    price_metric: PriceMetric,
    cost_basis: CostBasis,
    benchmark_source: BenchmarkSource,
    benchmark_metric: BenchmarkMetric,
    comparison_mode: ComparisonMode,
    common_scaffold: String,
    liquidity_filter: LiquidityFilter,
    modality_filter: ModalityFilter,
    uncensored_filter: UncensoredFilter,
    hide_catalog_prices: bool,
    log_price_axis: bool,
    hidden_groups: Vec<String>,
    selected_pareto_model: Option<String>,
    // Models tab
    models_creator: Option<String>,
    models_source: ModelsSource,
    models_modality: ModalityFilter,
    models_uncensored: UncensoredFilter,
    models_cache: ModelsCacheFilter,
    models_speed: SpeedFilter,
    models_sort: ModelsSort,
    models_sort_desc: bool,
    calc_model: Option<String>,
    calc_provider: Option<String>,
    /// Model ids pinned as floating price widgets, spawn order preserved.
    pinned_models: Vec<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: UI_STATE_VERSION,
            tab: Tab::Pareto,
            activity_tab: ActivityTab::PriceMoves,
            activity_kind_filter: None,
            new_listings_window: NewListingsWindow::default(),
            price_metric: PriceMetric::Blended,
            cost_basis: CostBasis::PerMillion,
            benchmark_source: BenchmarkSource::CompositeAgentic,
            benchmark_metric: BenchmarkMetric::Overall,
            comparison_mode: ComparisonMode::BestAvailableAgent,
            common_scaffold: String::new(),
            liquidity_filter: LiquidityFilter::Any,
            modality_filter: ModalityFilter::All,
            uncensored_filter: UncensoredFilter::All,
            hide_catalog_prices: false,
            log_price_axis: true,
            hidden_groups: vec![],
            selected_pareto_model: None,
            models_creator: None,
            models_source: ModelsSource::All,
            models_modality: ModalityFilter::All,
            models_uncensored: UncensoredFilter::All,
            models_cache: ModelsCacheFilter::Any,
            models_speed: SpeedFilter::Any,
            models_sort: ModelsSort::Name,
            models_sort_desc: false,
            calc_model: None,
            calc_provider: None,
            pinned_models: vec![],
        }
    }
}

pub(super) fn ui_state_path() -> PathBuf {
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.config_dir().join("ui_state.json")
    } else {
        PathBuf::from("paretowatch-ui-state.json")
    }
}

pub(super) fn load_ui_state() -> Option<UiState> {
    let bytes = fs::read(ui_state_path()).ok()?;
    let state: UiState = serde_json::from_slice(&bytes).ok()?;
    (state.version == UI_STATE_VERSION).then_some(state)
}

pub(super) fn save_ui_state(state: &UiState) {
    let path = ui_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        let _ = fs::write(path, bytes);
    }
}

impl ParetoWatchApp {
    /// Capture the persisted slice of the current UI state.
    pub(super) fn snapshot_ui_state(&self) -> UiState {
        UiState {
            version: UI_STATE_VERSION,
            tab: self.tab,
            activity_tab: self.activity_tab,
            activity_kind_filter: self.activity_kind_filter,
            new_listings_window: self.new_listings_window,
            price_metric: self.price_metric,
            cost_basis: self.cost_basis,
            benchmark_source: self.benchmark_source,
            benchmark_metric: self.benchmark_metric,
            comparison_mode: self.comparison_mode,
            common_scaffold: self.common_scaffold.clone(),
            liquidity_filter: self.liquidity_filter,
            modality_filter: self.modality_filter,
            uncensored_filter: self.uncensored_filter,
            hide_catalog_prices: self.hide_catalog_prices,
            log_price_axis: self.log_price_axis,
            hidden_groups: self.hidden_groups.iter().cloned().collect(),
            selected_pareto_model: self.selected_pareto_model.clone(),
            models_creator: self.models_creator.clone(),
            models_source: self.models_source,
            models_modality: self.models_modality,
            models_uncensored: self.models_uncensored,
            models_cache: self.models_cache,
            models_speed: self.models_speed,
            models_sort: self.models_sort,
            models_sort_desc: self.models_sort_desc,
            calc_model: self.calc_model.clone(),
            calc_provider: self.calc_provider.clone(),
            pinned_models: self.widgets.pinned_model_ids(),
        }
    }

    /// Apply a restored state onto a freshly constructed app. The pin list is
    /// held for later: the widgets need the first price snapshot before they
    /// can spawn.
    pub(super) fn apply_ui_state(&mut self, state: &UiState) {
        self.tab = state.tab;
        self.activity_tab = state.activity_tab;
        self.activity_kind_filter = state.activity_kind_filter;
        self.new_listings_window = state.new_listings_window;
        self.price_metric = state.price_metric;
        self.cost_basis = state.cost_basis;
        self.benchmark_source = state.benchmark_source;
        self.benchmark_metric = state.benchmark_metric;
        self.comparison_mode = state.comparison_mode;
        if !state.common_scaffold.is_empty() {
            self.common_scaffold = state.common_scaffold.clone();
        }
        self.liquidity_filter = state.liquidity_filter;
        self.modality_filter = state.modality_filter;
        self.uncensored_filter = state.uncensored_filter;
        self.hide_catalog_prices = state.hide_catalog_prices;
        self.log_price_axis = state.log_price_axis;
        self.hidden_groups = state.hidden_groups.iter().cloned().collect();
        self.selected_pareto_model = state.selected_pareto_model.clone();
        self.models_creator = state.models_creator.clone();
        self.models_source = state.models_source;
        self.models_modality = state.models_modality;
        self.models_uncensored = state.models_uncensored;
        self.models_cache = state.models_cache;
        self.models_speed = state.models_speed;
        self.models_sort = state.models_sort;
        self.models_sort_desc = state.models_sort_desc;
        self.calc_model = state.calc_model.clone();
        self.calc_provider = state.calc_provider.clone();
        self.pending_pin_restore =
            (!state.pinned_models.is_empty()).then(|| state.pinned_models.clone());
    }

    /// Write the UI state when it changed since the last write. Called from
    /// the update tick — comparing a small struct per frame is cheap, and a
    /// force-killed app only loses state changed in its final moments.
    pub(super) fn persist_ui_state_if_changed(&mut self) {
        let current = self.snapshot_ui_state();
        if self.last_saved_ui_state.as_ref() != Some(&current) {
            save_ui_state(&current);
            self.last_saved_ui_state = Some(current);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // One test, sequentially: every half shares the real ui_state.json path,
    // and separate #[test]s would race on that file under the parallel runner.
    fn ui_state_round_trips_and_tolerates_bad_files() {
        let state = UiState {
            version: UI_STATE_VERSION,
            tab: Tab::Models,
            benchmark_source: BenchmarkSource::TerminalBench4,
            log_price_axis: false,
            hidden_groups: vec!["xAI".into()],
            selected_pareto_model: Some("claude-opus-5.5".into()),
            models_sort: ModelsSort::Blended,
            models_sort_desc: true,
            pinned_models: vec!["gpt-6-sol".into(), "muse-wonder".into()],
            ..UiState::default()
        };
        let path = ui_state_path();
        save_ui_state(&state);
        let loaded = load_ui_state().expect("state loads after save");
        assert_eq!(loaded, state);
        // A corrupt file falls back to defaults rather than failing startup.
        fs::write(&path, b"{ nope").unwrap();
        assert!(load_ui_state().is_none());
        // So does an older file version.
        let old = serde_json::json!({"version": 0, "tab": "Models"});
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(load_ui_state().is_none());
        // And a missing file.
        let _ = fs::remove_file(&path);
        assert!(load_ui_state().is_none());
    }
}
