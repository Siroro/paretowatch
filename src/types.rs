//! Shared data model: settings, alert rules, market quotes, and benchmark
//! rows, plus their small pure helpers (price blending, liquidity filters,
//! labels). No fetching, scoring, or UI code lives in this module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const SWE_REBENCH_URL: &str = "https://swe-rebench.com/";
pub(crate) const REVELO_CODE_INDEX_URL: &str = "https://research.revelo.com/code-index/";
pub(crate) const ARTIFICIAL_ANALYSIS_URL: &str =
    "https://artificialanalysis.ai/leaderboards/models";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum PriceMetric {
    Blended,
    Input,
    Output,
}

impl PriceMetric {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Blended => "Blended",
            Self::Input => "Input",
            Self::Output => "Output",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum CostBasis {
    #[default]
    PerMillion,
    EstimatedPerTask,
}

impl CostBasis {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PerMillion => "$ / 1M tokens",
            Self::EstimatedPerTask => "Est. $ / benchmark task",
        }
    }

    pub(crate) fn unit(self) -> &'static str {
        match self {
            Self::PerMillion => "/1M",
            Self::EstimatedPerTask => "/task",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum AlertDirection {
    BelowOrEqual,
    AboveOrEqual,
}

impl AlertDirection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::BelowOrEqual => "at or below",
            Self::AboveOrEqual => "at or above",
        }
    }
}

/// Which way an any-change alert's minimum-move filter lets through.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum MoveDirection {
    #[default]
    Any,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum AlertSound {
    None,
    Soft,
    #[default]
    Chime,
    Urgent,
}

impl AlertSound {
    pub(crate) const ALL: [Self; 4] = [Self::None, Self::Soft, Self::Chime, Self::Urgent];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "Silent",
            Self::Soft => "Soft",
            Self::Chime => "Chime",
            Self::Urgent => "Urgent",
        }
    }
}

/// Whether a level-triggered alert must become false before it can fire again,
/// or may repeat while true once its cooldown has elapsed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum AlertRearm {
    #[default]
    ConditionReset,
    AfterCooldown,
}

impl AlertRearm {
    pub(crate) const ALL: [Self; 2] = [Self::ConditionReset, Self::AfterCooldown];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ConditionReset => "After condition resets",
            Self::AfterCooldown => "After cooldown",
        }
    }
}

impl MoveDirection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Any => "any direction",
            Self::Up => "up only",
            Self::Down => "down only",
        }
    }
}

/// Sentinel `AlertRule.model` meaning "do not pin this alert to one model".
/// Frontier entry/exit alerts read it as "watch the whole frontier".
pub(crate) const ANY_MODEL: &str = "*";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum AlertMode {
    #[default]
    Threshold,
    AnyChange,
    /// Fires each time the blended price undercuts every previously
    /// recorded value (baseline = the persistent history log).
    PriceFloor,
    /// Live-market discount percentage at or above a level.
    Discount,
    /// Healthy seller count at or below a level.
    SellerHealth,
    EntersFrontier,
    LeavesFrontier,
    CheapestAboveScore,
    NewModelListed,
    ModelDelisted,
}

impl AlertMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Threshold => "Threshold",
            Self::AnyChange => "Any price change",
            Self::PriceFloor => "All-time price low",
            Self::Discount => "Discount at or above",
            Self::SellerHealth => "Seller health falls to",
            Self::EntersFrontier => "Enters Pareto frontier",
            Self::LeavesFrontier => "Leaves Pareto frontier",
            Self::CheapestAboveScore => "Cheapest above score",
            Self::NewModelListed => "New model listed",
            Self::ModelDelisted => "Model delisted",
        }
    }

    pub(crate) fn benchmark_dependent(self) -> bool {
        matches!(
            self,
            Self::EntersFrontier | Self::LeavesFrontier | Self::CheapestAboveScore
        )
    }

    /// Feed-wide modes ignore the rule's model: they watch the whole price
    /// feed and are evaluated on the UI thread against the tracking history
    /// and consecutive snapshot diffs.
    pub(crate) fn feed_wide(self) -> bool {
        matches!(self, Self::NewModelListed | Self::ModelDelisted)
    }

    /// Modes the fetch worker can decide from quotes alone (no benchmarks,
    /// no long-term history). Everything else is evaluated on the UI thread.
    pub(crate) fn worker_evaluated(self) -> bool {
        matches!(
            self,
            Self::Threshold | Self::AnyChange | Self::Discount | Self::SellerHealth
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub(crate) enum BenchmarkSource {
    #[default]
    CompositeAgentic,
    CompositeDeployment,
    ArtificialAnalysisSnapshot,
    SWERebench,
    TerminalBench3,
    DeepSWE11,
    LiveBench,
    ReveloCodeIndex,
    DesignArena,
}

/// Which question a composite answers. Capability/value asks "how good is the
/// model itself"; deployment asks "how does it perform inside real agent
/// harnesses". The same source percentiles feed both, only the weighting and
/// trimming emphasis differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CompositeFlavor {
    #[default]
    Capability,
    Deployment,
}

/// Harness-specific boards measure "model X inside scaffold Y", so their
/// numbers say as much about the pairing as about the model. They keep full
/// weight for deployment questions and are demoted for capability questions.
pub(crate) const HARNESS_DEMOTION_FACTOR: f64 = 1.0 / 3.0;

impl BenchmarkSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CompositeAgentic => "Composite (capability)",
            Self::CompositeDeployment => "Composite (deployment)",
            Self::ArtificialAnalysisSnapshot => "Artificial Analysis (snapshot)",
            Self::SWERebench => "SWE-rebench",
            Self::TerminalBench3 => "Terminal-Bench 3.0",
            Self::DeepSWE11 => "DeepSWE v1.1",
            Self::LiveBench => "LiveBench",
            Self::ReveloCodeIndex => "Revelo Code Index",
            Self::DesignArena => "Frontend Design Elo (Design Arena)",
        }
    }

    pub(crate) fn short_label(self) -> &'static str {
        match self {
            Self::CompositeAgentic => "Composite",
            Self::CompositeDeployment => "Deploy comp",
            Self::ArtificialAnalysisSnapshot => "AA snapshot",
            Self::SWERebench => "SWE-rebench",
            Self::TerminalBench3 => "Terminal-Bench 3",
            Self::DeepSWE11 => "DeepSWE",
            Self::LiveBench => "LiveBench",
            Self::ReveloCodeIndex => "Code Index",
            Self::DesignArena => "Design Elo",
        }
    }

    pub(crate) fn is_composite(self) -> bool {
        matches!(self, Self::CompositeAgentic | Self::CompositeDeployment)
    }

    pub(crate) fn composite_flavor(self) -> Option<CompositeFlavor> {
        match self {
            Self::CompositeAgentic => Some(CompositeFlavor::Capability),
            Self::CompositeDeployment => Some(CompositeFlavor::Deployment),
            _ => None,
        }
    }

    /// Model-level boards: the published number describes the model itself.
    /// SWE-rebench is deliberately excluded — its uniform harness makes it an
    /// agentic-capability standard that stays fully weighted in both flavors.
    pub(crate) fn is_model_level(self) -> bool {
        matches!(
            self,
            Self::ArtificialAnalysisSnapshot | Self::LiveBench | Self::DesignArena
        )
    }

    pub(crate) fn is_harness_specific(self) -> bool {
        matches!(
            self,
            Self::TerminalBench3 | Self::DeepSWE11 | Self::ReveloCodeIndex
        )
    }

    pub(crate) fn remote_sources() -> [Self; 6] {
        [
            Self::SWERebench,
            Self::TerminalBench3,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::DesignArena,
        ]
    }

    pub(crate) fn consensus_sources() -> [Self; 7] {
        [
            Self::ArtificialAnalysisSnapshot,
            Self::SWERebench,
            Self::TerminalBench3,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::DesignArena,
        ]
    }

    pub(crate) fn display_sources() -> [Self; 7] {
        Self::consensus_sources()
    }

    pub(crate) fn source_url(self) -> &'static str {
        match self {
            Self::CompositeAgentic | Self::CompositeDeployment => "",
            Self::ArtificialAnalysisSnapshot => ARTIFICIAL_ANALYSIS_URL,
            Self::SWERebench => SWE_REBENCH_URL,
            Self::TerminalBench3 => {
                "https://hub.harborframework.com/datasets/terminal-bench/terminal-bench/latest?tab=leaderboard&leaderboard=3-0-0"
            }
            Self::DeepSWE11 => "https://deepswe.datacurve.ai/",
            Self::LiveBench => "https://livebench.ai/",
            Self::ReveloCodeIndex => REVELO_CODE_INDEX_URL,
            Self::DesignArena => "https://designarena.ai/leaderboard",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum BenchmarkMetric {
    Overall,
    Coding,
    #[default]
    AgenticCoding,
}

impl BenchmarkMetric {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overall => "Overall",
            Self::Coding => "Coding",
            Self::AgenticCoding => "Agentic coding",
        }
    }

    pub(crate) fn value(self, b: &Benchmark) -> Option<f64> {
        match self {
            Self::Overall => b.overall,
            Self::Coding => b.coding,
            Self::AgenticCoding => b.agentic_coding,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum ComparisonMode {
    #[default]
    ModelCapability,
    BestAvailableAgent,
    CommonScaffold,
}

impl ComparisonMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ModelCapability => "Model capability",
            Self::BestAvailableAgent => "Best available agent",
            Self::CommonScaffold => "Same/common scaffold",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum BenchmarkKind {
    #[default]
    Model,
    ModelAgent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum LiquidityFilter {
    #[default]
    Any,
    Trusted,
    Healthy3,
    Healthy10,
}

/// Chart modality filter. Vision detection comes from `/v1/models`
/// architecture metadata; models without modality data count as text-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ModalityFilter {
    #[default]
    All,
    Vision,
    TextOnly,
}

impl ModalityFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All models",
            Self::Vision => "Vision-capable",
            Self::TextOnly => "Text-only",
        }
    }

    pub(crate) fn allows(self, vision: bool) -> bool {
        match self {
            Self::All => true,
            Self::Vision => vision,
            Self::TextOnly => !vision,
        }
    }
}

impl LiquidityFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Any => "Any pricing",
            Self::Trusted => "Trusted live provider",
            Self::Healthy3 => "≥3 healthy sellers",
            Self::Healthy10 => "≥10 healthy sellers",
        }
    }

    pub(crate) fn apply(
        self,
        quote: &Quote,
        input_weight: f64,
        cache_read_weight: f64,
        output_weight: f64,
    ) -> Option<Quote> {
        if self == Self::Any {
            return Some(quote.clone());
        }
        if !quote.live_market {
            return None;
        }

        // Prefer the cheapest *real provider* satisfying the selected market-
        // quality constraint. This avoids dropping a model just because its
        // globally cheapest provider is untrusted when a slightly dearer trusted
        // provider is available in the same Surplus market row. Within the
        // qualifying set, zero-priced asks likewise defer to any priced ask.
        let qualifies = |option: &ProviderMarketQuote| match self {
            Self::Any => true,
            Self::Trusted => option.trusted == Some(true),
            Self::Healthy3 => option.healthy_seller_count.unwrap_or(0) >= 3,
            Self::Healthy10 => option.healthy_seller_count.unwrap_or(0) >= 10,
        };
        let by_workload = |a: &&ProviderMarketQuote, b: &&ProviderMarketQuote| {
            a.workload_price(input_weight, cache_read_weight, output_weight)
                .total_cmp(&b.workload_price(input_weight, cache_read_weight, output_weight))
        };
        let qualifying: Vec<&ProviderMarketQuote> = quote
            .market_options
            .iter()
            .filter(|option| qualifies(option))
            .collect();
        if let Some(option) = qualifying
            .iter()
            .filter(|option| !is_free_pair(option.input, option.output))
            .min_by(|a, b| by_workload(a, b))
            .or_else(|| qualifying.iter().min_by(|a, b| by_workload(a, b)))
        {
            let mut selected = quote.clone();
            selected.provider = option.provider.clone();
            selected.input = option.input;
            selected.output = option.output;
            selected.cache_read = option.cache_read;
            selected.provider_trusted = option.trusted;
            selected.healthy_seller_count = option.healthy_seller_count;
            return Some(selected);
        }

        // Compatibility fallback for an older/partial market payload with no
        // provider array: only keep it when the aggregate quote itself proves the
        // requested quality property.
        let aggregate_ok = match self {
            Self::Any => true,
            Self::Trusted => quote.provider_trusted == Some(true),
            Self::Healthy3 => quote.healthy_seller_count.unwrap_or(0) >= 3,
            Self::Healthy10 => quote.healthy_seller_count.unwrap_or(0) >= 10,
        };
        aggregate_ok.then(|| quote.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AlertRule {
    pub(crate) id: u64,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) mode: AlertMode,
    pub(crate) metric: PriceMetric,
    #[serde(default)]
    pub(crate) cost_basis: CostBasis,
    pub(crate) direction: AlertDirection,
    pub(crate) threshold: f64,
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) benchmark_source: BenchmarkSource,
    #[serde(default)]
    pub(crate) benchmark_metric: BenchmarkMetric,
    #[serde(default)]
    pub(crate) comparison_mode: ComparisonMode,
    #[serde(default = "default_common_scaffold")]
    pub(crate) common_scaffold: String,
    #[serde(default)]
    pub(crate) liquidity_filter: LiquidityFilter,
    #[serde(default = "default_score_threshold")]
    pub(crate) score_threshold: f64,
    /// Any-change filter: minimum |%| move (on input, output, or blended)
    /// before the alert fires. 0 keeps the historical "any change" behavior.
    #[serde(default)]
    pub(crate) min_move_pct: f64,
    #[serde(default)]
    pub(crate) move_direction: MoveDirection,
    /// Discount mode: market discount percentage that arms the rule.
    #[serde(default = "default_discount_threshold")]
    pub(crate) discount_threshold_pct: f64,
    /// Seller-health mode: fire when healthy sellers fall to or below this.
    #[serde(default)]
    pub(crate) healthy_seller_floor: u64,
    /// Minimum delay between delivered notifications for this rule.
    #[serde(default)]
    pub(crate) cooldown_minutes: u64,
    /// How a level-triggered condition becomes eligible to fire again.
    #[serde(default)]
    pub(crate) rearm: AlertRearm,
    /// Optional embedded sound played in addition to the desktop toast.
    #[serde(default)]
    pub(crate) sound: AlertSound,
}

pub(crate) fn default_common_scaffold() -> String {
    "mini-SWE-agent".into()
}
pub(crate) fn default_score_threshold() -> f64 {
    50.0
}
pub(crate) fn default_discount_threshold() -> f64 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) poll_seconds: u64,
    pub(crate) input_weight: f64,
    pub(crate) cache_read_weight: f64,
    pub(crate) output_weight: f64,
    /// Replace the feed's published discount with a workload-weighted
    /// market-vs-list discount that includes the cache-read leg. List
    /// cache-read pricing comes from the catalogue; the market feed
    /// publishes no direct cache price.
    pub(crate) include_cache_read_in_discount: bool,
    pub(crate) alerts: Vec<AlertRule>,
    /// Base text size for the floating Pinned Prices window; every size in
    /// that window scales from it (10 is the original hard-coded look).
    pub(crate) pinned_price_font_size: f32,
    /// Global quiet-hours gate. Alert events are still written to Activity,
    /// but desktop toasts/audio are suppressed during the configured window.
    pub(crate) quiet_hours_enabled: bool,
    /// Local wall-clock hour (0-23) when quiet hours begin.
    pub(crate) quiet_hours_start: u8,
    /// Local wall-clock hour (0-23) when quiet hours end.
    pub(crate) quiet_hours_end: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_seconds: 30,
            input_weight: 15.0,
            cache_read_weight: 80.0,
            output_weight: 5.0,
            include_cache_read_in_discount: false,
            alerts: vec![],
            pinned_price_font_size: 10.0,
            quiet_hours_enabled: false,
            quiet_hours_start: 22,
            quiet_hours_end: 7,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderMarketQuote {
    pub(crate) provider: String,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_read: Option<f64>,
    pub(crate) trusted: Option<bool>,
    pub(crate) healthy_seller_count: Option<u64>,
}

impl ProviderMarketQuote {
    pub(crate) fn workload_price(
        &self,
        input_weight: f64,
        cache_read_weight: f64,
        output_weight: f64,
    ) -> f64 {
        blended_price(
            self.input,
            self.cache_read,
            self.output,
            input_weight,
            cache_read_weight,
            output_weight,
        )
    }
}

/// A zero-priced token pair. Surplus markets can carry genuine 100%-off asks
/// (a seller with a 0x cost multiplier); such an ask is real but should not
/// own the headline price while any priced ask exists.
pub(crate) fn is_free_pair(input: f64, output: f64) -> bool {
    input == 0.0 && output == 0.0
}

#[derive(Debug, Clone)]
pub(crate) struct Quote {
    pub(crate) model: String,
    pub(crate) display_name: String,
    pub(crate) creator: String,
    pub(crate) provider: String,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_read: Option<f64>,
    pub(crate) seller_count: Option<u64>,
    pub(crate) healthy_seller_count: Option<u64>,
    pub(crate) provider_trusted: Option<bool>,
    pub(crate) requests_24h: Option<u64>,
    pub(crate) volume_24h: Option<f64>,
    pub(crate) discount_pct: Option<f64>,
    pub(crate) discount_direction: Option<String>,
    pub(crate) market_options: Vec<ProviderMarketQuote>,
    pub(crate) live_market: bool,
    /// The live market also lists a zero-priced ask. Headline prices exclude
    /// it whenever a priced ask exists; the UI marks affected rows so the
    /// published discount never looks cheaper than what money can buy.
    pub(crate) free_offer_listed: bool,
    /// Model accepts image input alongside text (vision LLM), per the
    /// `/v1/models` architecture metadata. Only meaningful when the catalog
    /// feed supplied modality data; the comparison matrix never sets it.
    pub(crate) vision: bool,
}

impl Quote {
    pub(crate) fn price(
        &self,
        metric: PriceMetric,
        input_weight: f64,
        cache_read_weight: f64,
        output_weight: f64,
    ) -> f64 {
        match metric {
            PriceMetric::Input => self.input,
            PriceMetric::Output => self.output,
            PriceMetric::Blended => blended_price(
                self.input,
                self.cache_read,
                self.output,
                input_weight,
                cache_read_weight,
                output_weight,
            ),
        }
    }
}

pub(crate) fn blended_price(
    input: f64,
    cache_read: Option<f64>,
    output: f64,
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
) -> f64 {
    let input_weight = input_weight.max(0.0);
    let cache_read_weight = cache_read_weight.max(0.0);
    let output_weight = output_weight.max(0.0);
    let total_weight = input_weight + cache_read_weight + output_weight;
    if total_weight <= f64::EPSILON {
        return (input + output) / 2.0;
    }
    let cache_price = cache_read.unwrap_or(input);
    (input * input_weight + cache_price * cache_read_weight + output * output_weight) / total_weight
}

/// Workload-weighted discount of market prices versus list prices, including
/// the cache-read leg. Returns `None` when the list blended price is zero or
/// the ratio is not finite, so callers can keep the feed's published number.
pub(crate) fn workload_discount_pct(
    market: (f64, Option<f64>, f64),
    list: (f64, Option<f64>, f64),
    weights: (f64, f64, f64),
) -> Option<f64> {
    let (market_input, market_cache_read, market_output) = market;
    let (list_input, list_cache_read, list_output) = list;
    let (input_weight, cache_read_weight, output_weight) = weights;
    let list_blended = blended_price(
        list_input,
        list_cache_read,
        list_output,
        input_weight,
        cache_read_weight,
        output_weight,
    );
    if list_blended.partial_cmp(&f64::EPSILON) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let market_blended = blended_price(
        market_input,
        market_cache_read,
        market_output,
        input_weight,
        cache_read_weight,
        output_weight,
    );
    let discount = (1.0 - market_blended / list_blended) * 100.0;
    discount.is_finite().then_some(discount)
}

#[derive(Debug, Clone)]
pub(crate) struct PriceSnapshot {
    pub(crate) quotes: Vec<Quote>,
    pub(crate) fetched_at: DateTime<Utc>,
    pub(crate) comparison_updated_at: Option<String>,
    pub(crate) market_overlay_count: usize,
    pub(crate) market_error: Option<String>,
    pub(crate) base_source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Benchmark {
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) creator: String,
    pub(crate) overall: Option<f64>,
    pub(crate) coding: Option<f64>,
    pub(crate) agentic_coding: Option<f64>,
    pub(crate) agent: Option<String>,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) kind: BenchmarkKind,
    /// Average tokens consumed by one benchmark task/attempt when the source
    /// publishes enough telemetry to estimate it. Pricing is *not* taken from
    /// the benchmark: ParetoWatch always reprices this volume with current
    /// Surplus quotes.
    pub(crate) tokens_per_task: Option<f64>,
    pub(crate) token_profile: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;

    #[test]
    fn alert_rules_from_pre_move_filter_configs_keep_defaults() {
        // A config.json written before min-move filtering and the feed-wide
        // modes must load unchanged: every added field is serde-defaulted.
        let legacy = r#"{
            "id": 7,
            "model": "openai/gpt-x",
            "mode": "AnyChange",
            "metric": "Blended",
            "cost_basis": "PerMillion",
            "direction": "BelowOrEqual",
            "threshold": 1.0,
            "enabled": true
        }"#;
        let alert: AlertRule = serde_json::from_str(legacy).expect("legacy alert loads");
        assert_eq!(alert.mode, AlertMode::AnyChange);
        assert_eq!(alert.min_move_pct, 0.0);
        assert_eq!(alert.move_direction, MoveDirection::Any);
        assert_eq!(alert.common_scaffold, default_common_scaffold());
        assert_eq!(alert.discount_threshold_pct, default_discount_threshold());
        assert_eq!(alert.healthy_seller_floor, 0);
        assert_eq!(alert.cooldown_minutes, 0);
        assert_eq!(alert.rearm, AlertRearm::ConditionReset);
        assert_eq!(alert.sound, AlertSound::Chime);
    }

    #[test]
    fn blended_price_is_normalized_and_cache_aware() {
        let price = blended_price(2.0, Some(0.2), 10.0, 15.0, 80.0, 5.0);
        assert!((price - 0.96).abs() < 1e-12);
        let fallback = blended_price(2.0, None, 10.0, 15.0, 80.0, 5.0);
        assert!((fallback - 2.4).abs() < 1e-12);
    }

    #[test]
    fn workload_discount_includes_cache_read_leg() {
        // Same 50% cut on every leg must give 50% regardless of weights.
        let pct = workload_discount_pct(
            (1.0, Some(0.1), 5.0),
            (2.0, Some(0.2), 10.0),
            (15.0, 80.0, 5.0),
        )
        .unwrap();
        assert!((pct - 50.0).abs() < 1e-9);
        // A cache-heavy workload must weigh the cache leg: only the cache
        // leg is discounted here (input/output cost the same on both
        // sides), so a no-cache workload sees none of the cut.
        let cache_heavy = workload_discount_pct(
            (2.0, Some(0.02), 10.0),
            (2.0, Some(0.2), 10.0),
            (15.0, 80.0, 5.0),
        )
        .unwrap();
        let no_cache = workload_discount_pct(
            (2.0, Some(0.02), 10.0),
            (2.0, Some(0.2), 10.0),
            (50.0, 0.0, 50.0),
        )
        .unwrap();
        assert!(cache_heavy > 10.0);
        assert!(no_cache.abs() < 1e-9);
    }

    #[test]
    fn workload_discount_missing_cache_falls_back_to_input() {
        // No cache price on either side: both fall back to their input
        // prices, and an all-legs-50% market still yields exactly 50%.
        let pct =
            workload_discount_pct((1.0, None, 5.0), (2.0, None, 10.0), (15.0, 80.0, 5.0)).unwrap();
        assert!((pct - 50.0).abs() < 1e-9);
        // Market side lacking a cache price while the catalogue has one
        // borrows the market input price for that leg.
        let pct =
            workload_discount_pct((1.0, None, 10.0), (2.0, Some(0.2), 10.0), (0.0, 100.0, 0.0))
                .unwrap();
        assert!((pct - (1.0 - 1.0 / 0.2) * 100.0).abs() < 1e-9);
    }

    #[test]
    fn workload_discount_rejects_degenerate_list_prices() {
        assert!(
            workload_discount_pct(
                (1.0, Some(0.1), 5.0),
                (0.0, Some(0.0), 0.0),
                (15.0, 80.0, 5.0),
            )
            .is_none()
        );
    }

    #[test]
    fn liquidity_filters_use_live_market_quality() {
        let q = test_quote("model-a", 1.0, true);
        assert!(
            LiquidityFilter::Trusted
                .apply(&q, 15.0, 80.0, 5.0)
                .is_some()
        );
        assert!(
            LiquidityFilter::Healthy3
                .apply(&q, 15.0, 80.0, 5.0)
                .is_some()
        );
        assert!(
            LiquidityFilter::Healthy10
                .apply(&q, 15.0, 80.0, 5.0)
                .is_none()
        );
        let fallback = test_quote("model-a", 1.0, false);
        assert!(
            LiquidityFilter::Trusted
                .apply(&fallback, 15.0, 80.0, 5.0)
                .is_none()
        );
    }

    #[test]
    fn liquidity_filters_defer_to_priced_providers_over_free_asks() {
        let mut q = test_quote("model-a", 1.0, true);
        q.market_options = vec![
            ProviderMarketQuote {
                provider: "free".into(),
                input: 0.0,
                output: 0.0,
                cache_read: None,
                trusted: Some(true),
                healthy_seller_count: Some(10),
            },
            ProviderMarketQuote {
                provider: "priced".into(),
                input: 0.6,
                output: 1.1,
                cache_read: Some(0.06),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
        ];
        for filter in [LiquidityFilter::Trusted, LiquidityFilter::Healthy3] {
            let selected = filter.apply(&q, 15.0, 80.0, 5.0).unwrap();
            assert_eq!(
                selected.provider, "priced",
                "{filter:?} picked the free ask"
            );
            assert!((selected.input - 0.6).abs() < 1e-12);
        }
    }

    #[test]
    fn liquidity_filters_keep_all_free_markets_when_no_priced_option_qualifies() {
        let mut q = test_quote("free-model", 0.0, true);
        q.market_options = vec![ProviderMarketQuote {
            provider: "only-free".into(),
            input: 0.0,
            output: 0.0,
            cache_read: None,
            trusted: Some(true),
            healthy_seller_count: Some(4),
        }];
        let selected = LiquidityFilter::Healthy3
            .apply(&q, 15.0, 80.0, 5.0)
            .unwrap();
        assert_eq!(selected.provider, "only-free");
    }

    #[test]
    fn modality_filter_partitions_by_vision_flag() {
        assert!(ModalityFilter::All.allows(true));
        assert!(ModalityFilter::All.allows(false));
        assert!(ModalityFilter::Vision.allows(true));
        assert!(!ModalityFilter::Vision.allows(false));
        assert!(!ModalityFilter::TextOnly.allows(true));
        assert!(ModalityFilter::TextOnly.allows(false));
    }
}
