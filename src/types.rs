//! Shared data model: settings, alert rules, market quotes, and benchmark
//! rows, plus their small pure helpers (price blending, liquidity filters,
//! labels). No fetching, scoring, or UI code lives in this module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const SWE_REBENCH_URL: &str = "https://swe-rebench.com/";
pub(crate) const REVELO_CODE_INDEX_URL: &str = "https://research.revelo.com/code-index/";
pub(crate) const FRONTIERCODE_URL: &str = "https://cognition.com/frontiercode";
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
    TerminalBench4,
    DeepSWE11,
    LiveBench,
    ReveloCodeIndex,
    DesignArena,
    FrontierCode,
    AABriefcase,
    AutomationBenchAA,
    SWEBenchPro,
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
            Self::ArtificialAnalysisSnapshot => "Artificial Analysis (live + snapshot)",
            Self::SWERebench => "SWE-rebench",
            Self::TerminalBench3 => "Terminal-Bench 3.0",
            Self::TerminalBench4 => "Terminal-Bench 4.0",
            Self::DeepSWE11 => "DeepSWE v1.1",
            Self::LiveBench => "LiveBench",
            Self::ReveloCodeIndex => "Revelo Code Index",
            Self::DesignArena => "Frontend Design Elo (Design Arena)",
            Self::FrontierCode => "FrontierCode (Cognition)",
            Self::AABriefcase => "AA-Briefcase (Elo)",
            Self::AutomationBenchAA => "AutomationBench-AA",
            Self::SWEBenchPro => "SWE-Bench Pro V2 HARD (Scale)",
        }
    }

    pub(crate) fn short_label(self) -> &'static str {
        match self {
            Self::CompositeAgentic => "Composite",
            Self::CompositeDeployment => "Deploy comp",
            Self::ArtificialAnalysisSnapshot => "AA Intelligence",
            Self::SWERebench => "SWE-rebench",
            Self::TerminalBench3 => "Terminal-Bench 3",
            Self::TerminalBench4 => "Terminal-Bench 4",
            Self::DeepSWE11 => "DeepSWE",
            Self::LiveBench => "LiveBench",
            Self::ReveloCodeIndex => "Code Index",
            Self::DesignArena => "Design Elo",
            Self::FrontierCode => "FrontierCode",
            Self::AABriefcase => "AA-Briefcase",
            Self::AutomationBenchAA => "AutomationBench",
            Self::SWEBenchPro => "SWE-Bench Pro",
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
    /// The AA evaluation boards run every model under AA's own standardized
    /// protocol, so they measure the model rather than a vendor scaffold.
    pub(crate) fn is_model_level(self) -> bool {
        matches!(
            self,
            Self::ArtificialAnalysisSnapshot
                | Self::LiveBench
                | Self::DesignArena
                | Self::AABriefcase
                | Self::AutomationBenchAA
        )
    }

    pub(crate) fn is_harness_specific(self) -> bool {
        matches!(
            self,
            Self::TerminalBench3
                | Self::TerminalBench4
                | Self::DeepSWE11
                | Self::ReveloCodeIndex
                | Self::FrontierCode
                | Self::SWEBenchPro
        )
    }

    pub(crate) fn remote_sources() -> [Self; 12] {
        [
            // The AA Intelligence Index fetch overlays live leaderboard values
            // onto the bundled snapshot (it never fails — the bundle is the
            // offline fallback), so it joins the periodic refresh.
            Self::ArtificialAnalysisSnapshot,
            Self::SWERebench,
            Self::TerminalBench3,
            Self::TerminalBench4,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::DesignArena,
            Self::FrontierCode,
            Self::AABriefcase,
            Self::AutomationBenchAA,
            Self::SWEBenchPro,
        ]
    }

    pub(crate) fn consensus_sources() -> [Self; 12] {
        [
            Self::ArtificialAnalysisSnapshot,
            Self::SWERebench,
            Self::TerminalBench3,
            Self::TerminalBench4,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::DesignArena,
            Self::FrontierCode,
            Self::AABriefcase,
            Self::AutomationBenchAA,
            Self::SWEBenchPro,
        ]
    }

    pub(crate) fn display_sources() -> [Self; 12] {
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
            Self::TerminalBench4 => {
                "https://hub.harborframework.com/datasets/terminal-bench/terminal-bench/latest?tab=leaderboard&leaderboard=4-0-0"
            }
            Self::DeepSWE11 => "https://deepswe.datacurve.ai/",
            Self::LiveBench => "https://livebench.ai/",
            Self::ReveloCodeIndex => REVELO_CODE_INDEX_URL,
            Self::DesignArena => "https://designarena.ai/leaderboard",
            Self::FrontierCode => FRONTIERCODE_URL,
            Self::AABriefcase => "https://artificialanalysis.ai/evaluations/aa-briefcase",
            Self::AutomationBenchAA => {
                "https://artificialanalysis.ai/evaluations/automationbench-aa"
            }
            Self::SWEBenchPro => "https://labs.scale.com/leaderboard/swe_bench_pro_public_v2",
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

/// Safety-tuning filter, shared by the Pareto chart and the Models tab.
/// Uncensored detection comes from the listing text (see
/// [`is_uncensored_model`]); everything else counts as safety-tuned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum UncensoredFilter {
    #[default]
    All,
    Uncensored,
    SafetyTuned,
}

impl UncensoredFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All models",
            Self::Uncensored => "Uncensored",
            Self::SafetyTuned => "Safety-tuned",
        }
    }

    pub(crate) fn allows(self, uncensored: bool) -> bool {
        match self {
            Self::All => true,
            Self::Uncensored => uncensored,
            Self::SafetyTuned => !uncensored,
        }
    }
}

/// True when a listing identifies itself as an uncensored deployment: refusal
/// behavior removed from the weights (abliterated finetunes such as Abliterated
/// Large V2 or GLM 4.7 Flash Heretic), self-declared uncensored builds, or
/// creators whose whole catalogue is served without safety tuning (Venice AI,
/// Abliteration AI, the Dolphin finetune family). Surplus publishes no policy
/// metadata, so the catalog text is the only signal — matched as substrings
/// over the id, display name, and creator, case-insensitively.
pub(crate) fn is_uncensored_model(id: &str, name: &str, creator: &str) -> bool {
    let haystack = format!("{id} {name} {creator}").to_lowercase();
    [
        "abliterated",
        "abliteration",
        "uncensored",
        "heretic",
        "dolphin",
        "venice",
    ]
    .iter()
    .any(|token| haystack.contains(token))
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
        cache_fallback_pct: f64,
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
            a.workload_price(
                input_weight,
                cache_read_weight,
                output_weight,
                cache_fallback_pct,
            )
            .total_cmp(&b.workload_price(
                input_weight,
                cache_read_weight,
                output_weight,
                cache_fallback_pct,
            ))
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
    /// Assumed cache-read price, as a percentage of the fresh-input price,
    /// for models where nothing published a cache rate at all — no provider
    /// ask in the market and no catalogue list price (roughly half the
    /// catalogue). Published rates are always preferred; observed direct-
    /// cost billing clusters at 5–25% of input (10% is the classic
    /// Anthropic/DeepSeek/GLM-style rate), so 10 is the default.
    pub(crate) cache_read_fallback_pct: f64,
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
            cache_read_fallback_pct: 10.0,
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
        cache_fallback_pct: f64,
    ) -> f64 {
        blended_price(
            self.input,
            self.cache_read,
            self.output,
            input_weight,
            cache_read_weight,
            output_weight,
            cache_fallback_pct,
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
    /// Cache-read list price published for this model by the catalogue or
    /// comparison matrix. Unlike `cache_read`, the market overlay never
    /// overwrites it, so it survives as the published fallback when no
    /// market provider publishes a cache ask. Observed direct-cost billing
    /// tracks these published rates closely.
    pub(crate) catalog_cache_read: Option<f64>,
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
    /// Listing presents itself as uncensored (refusal behavior removed), per
    /// [`is_uncensored_model`] over the catalog text. The feed publishes no
    /// policy metadata, so this is name/creator detection, not a claim about
    /// the weights' internals.
    pub(crate) uncensored: bool,
}

impl Quote {
    /// Cache-read rate a cost calculation should bill when the quote is used
    /// as-is: the selected provider's own cache ask, else the cheapest cache
    /// ask anywhere in this model's market. `None` when no provider in the
    /// market publishes a cache price.
    pub(crate) fn best_available_cache_read(&self) -> Option<f64> {
        if let Some(rate) = self.cache_read {
            return Some(rate);
        }
        let best = self
            .market_options
            .iter()
            .filter_map(|option| option.cache_read)
            .fold(f64::INFINITY, f64::min);
        best.is_finite().then_some(best)
    }

    /// The cache-read rate blended pricing should bill: published numbers
    /// first — the selected provider's ask, else the market-wide best ask,
    /// else the model's catalogue list price — and only when nothing is
    /// published an assumed share of the fresh-input price. Always resolves
    /// to a rate (`Some`), matching how prompt caching is billed in
    /// practice: direct-cost usage data shows real cache reads land near
    /// the published rates, never at the full input price.
    pub(crate) fn effective_cache_read(&self, cache_fallback_pct: f64) -> Option<f64> {
        Some(
            self.best_available_cache_read()
                .or(self.catalog_cache_read)
                .unwrap_or_else(|| assumed_cache_price(self.input, cache_fallback_pct)),
        )
    }

    pub(crate) fn price(
        &self,
        metric: PriceMetric,
        input_weight: f64,
        cache_read_weight: f64,
        output_weight: f64,
        cache_fallback_pct: f64,
    ) -> f64 {
        match metric {
            PriceMetric::Input => self.input,
            PriceMetric::Output => self.output,
            PriceMetric::Blended => blended_price(
                self.input,
                self.effective_cache_read(cache_fallback_pct),
                self.output,
                input_weight,
                cache_read_weight,
                output_weight,
                cache_fallback_pct,
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
    cache_fallback_pct: f64,
) -> f64 {
    let input_weight = input_weight.max(0.0);
    let cache_read_weight = cache_read_weight.max(0.0);
    let output_weight = output_weight.max(0.0);
    let total_weight = input_weight + cache_read_weight + output_weight;
    if total_weight <= f64::EPSILON {
        return (input + output) / 2.0;
    }
    let cache_price = cache_read.unwrap_or_else(|| assumed_cache_price(input, cache_fallback_pct));
    (input * input_weight + cache_price * cache_read_weight + output * output_weight) / total_weight
}

/// Cache-read rate assumed when nobody publishes one: the fresh-input price
/// scaled by `cache_fallback_pct` percent. See
/// [`Settings::cache_read_fallback_pct`] for why the default is 10.
pub(crate) fn assumed_cache_price(input_price: f64, cache_fallback_pct: f64) -> f64 {
    (input_price.max(0.0) * cache_fallback_pct.max(0.0)) / 100.0
}

/// Dollar cost of an explicit token workload against one model's per-1M
/// rates. The cache-read leg is billed at the cache rate when one is known
/// and at the assumed percentage of the fresh-input rate otherwise — the
/// same fallback [`blended_price`] applies, so a cache-less model is never
/// billed as free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WorkloadCost {
    /// Fresh (uncached) input-token cost in dollars.
    pub(crate) input: f64,
    /// Cached input-token cost in dollars at the billed cache rate.
    pub(crate) cache_read: f64,
    /// Output-token cost in dollars.
    pub(crate) output: f64,
    /// What the cached tokens would have cost at the fresh-input rate;
    /// the difference is what prompt caching saves.
    pub(crate) cache_read_at_input_rate: f64,
}

impl WorkloadCost {
    pub(crate) fn total(&self) -> f64 {
        self.input + self.cache_read + self.output
    }

    /// Dollars saved by prompt caching on this workload; zero when the cache
    /// leg was billed at the input rate anyway or caching is dearer than
    /// fresh input (some providers price cache reads above input).
    pub(crate) fn cache_savings(&self) -> f64 {
        (self.cache_read_at_input_rate - self.cache_read).max(0.0)
    }
}

/// Price a concrete token volume. Token counts are in millions and rates in
/// $/1M tokens, so each leg is simply `tokens × rate` dollars. Negative
/// token counts clamp to zero.
pub(crate) fn token_workload_cost(
    input_tokens_m: f64,
    cache_read_tokens_m: f64,
    output_tokens_m: f64,
    input_price: f64,
    cache_read_price: Option<f64>,
    output_price: f64,
    cache_fallback_pct: f64,
) -> WorkloadCost {
    let input_tokens_m = input_tokens_m.max(0.0);
    let cache_read_tokens_m = cache_read_tokens_m.max(0.0);
    let output_tokens_m = output_tokens_m.max(0.0);
    let input = input_tokens_m * input_price.max(0.0);
    let output = output_tokens_m * output_price.max(0.0);
    let billed_cache_rate = cache_read_price.filter(|rate| *rate >= 0.0);
    let input_price = input_price.max(0.0);
    WorkloadCost {
        input,
        cache_read: cache_read_tokens_m
            * billed_cache_rate.unwrap_or(assumed_cache_price(input_price, cache_fallback_pct)),
        output,
        cache_read_at_input_rate: cache_read_tokens_m * input_price,
    }
}

/// Workload-weighted discount of market prices versus list prices, including
/// the cache-read leg. Returns `None` when the list blended price is zero or
/// the ratio is not finite, so callers can keep the feed's published number.
pub(crate) fn workload_discount_pct(
    market: (f64, Option<f64>, f64),
    list: (f64, Option<f64>, f64),
    weights: (f64, f64, f64),
    cache_fallback_pct: f64,
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
        cache_fallback_pct,
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
        cache_fallback_pct,
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
        let price = blended_price(2.0, Some(0.2), 10.0, 15.0, 80.0, 5.0, 100.0);
        assert!((price - 0.96).abs() < 1e-12);
        let fallback = blended_price(2.0, None, 10.0, 15.0, 80.0, 5.0, 100.0);
        assert!((fallback - 2.4).abs() < 1e-12);
    }

    #[test]
    fn blended_price_assumes_unpublished_cache_at_configured_input_share() {
        // 10% of the $2 input = $0.20 assumed cache rate, matching a
        // published $0.20 ask exactly.
        let assumed = blended_price(2.0, None, 10.0, 15.0, 80.0, 5.0, 10.0);
        assert!((assumed - 0.96).abs() < 1e-12);
        // 100% reproduces the old bill-cache-at-input behavior.
        let at_input = blended_price(2.0, None, 10.0, 15.0, 80.0, 5.0, 100.0);
        assert!((at_input - 2.4).abs() < 1e-12);
        // A published ask always wins over the assumption.
        let published = blended_price(2.0, Some(0.9), 10.0, 15.0, 80.0, 5.0, 10.0);
        assert!(published > assumed);
    }

    #[test]
    fn token_workload_cost_bills_each_leg_and_clamps() {
        // 2M fresh input at $1.50, 8M cache reads at $0.15, 0.5M output at $8.
        let cost = token_workload_cost(2.0, 8.0, 0.5, 1.5, Some(0.15), 8.0, 100.0);
        assert!((cost.input - 3.0).abs() < 1e-12);
        assert!((cost.cache_read - 1.2).abs() < 1e-12);
        assert!((cost.output - 4.0).abs() < 1e-12);
        assert!((cost.total() - 8.2).abs() < 1e-12);
        // Prompt caching saved 8M × ($1.50 − $0.15) = $10.80.
        assert!((cost.cache_read_at_input_rate - 12.0).abs() < 1e-12);
        assert!((cost.cache_savings() - 10.8).abs() < 1e-12);
    }

    #[test]
    fn token_workload_cost_without_cache_price_bills_at_input_rate() {
        let cost = token_workload_cost(1.0, 4.0, 1.0, 2.0, None, 6.0, 100.0);
        assert!((cost.cache_read - 8.0).abs() < 1e-12);
        assert!((cost.cache_read_at_input_rate - 8.0).abs() < 1e-12);
        assert!((cost.cache_savings() - 0.0).abs() < 1e-12);
        // Negative token counts and rates clamp instead of crediting money.
        let clamped = token_workload_cost(-5.0, -5.0, -5.0, -2.0, Some(-1.0), -6.0, 10.0);
        assert!((clamped.total() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn token_workload_cost_without_cache_price_bills_at_assumed_share() {
        // No published cache rate: 4M cached tokens at 10% of the $2 input =
        // $0.80 instead of $8, so caching is credited with saving $7.20.
        let cost = token_workload_cost(1.0, 4.0, 1.0, 2.0, None, 6.0, 10.0);
        assert!((cost.cache_read - 0.8).abs() < 1e-12);
        assert!((cost.cache_read_at_input_rate - 8.0).abs() < 1e-12);
        assert!((cost.cache_savings() - 7.2).abs() < 1e-12);
    }

    #[test]
    fn token_workload_cost_cache_dearer_than_input_has_no_savings() {
        let cost = token_workload_cost(0.0, 1.0, 0.0, 1.0, Some(2.5), 1.0, 100.0);
        assert!((cost.cache_read - 2.5).abs() < 1e-12);
        assert!((cost.cache_savings() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn best_available_cache_read_prefers_provider_then_market() {
        let mut q = test_quote("glm-5.3", 1.0, true);
        q.cache_read = None;
        q.market_options = vec![
            ProviderMarketQuote {
                provider: "no-cache-ask".into(),
                input: 0.1,
                output: 0.4,
                cache_read: None,
                trusted: Some(true),
                healthy_seller_count: Some(3),
            },
            ProviderMarketQuote {
                provider: "dearer".into(),
                input: 0.2,
                output: 0.8,
                cache_read: Some(0.03),
                trusted: Some(true),
                healthy_seller_count: Some(3),
            },
            ProviderMarketQuote {
                provider: "cheapest-cache".into(),
                input: 0.3,
                output: 0.9,
                cache_read: Some(0.02),
                trusted: Some(true),
                healthy_seller_count: Some(3),
            },
        ];
        assert!((q.best_available_cache_read().unwrap() - 0.02).abs() < 1e-12);
        // The selected provider's own ask always wins over a cheaper other.
        q.cache_read = Some(0.05);
        assert!((q.best_available_cache_read().unwrap() - 0.05).abs() < 1e-12);
        // No cache pricing anywhere.
        q.cache_read = None;
        q.market_options.clear();
        assert_eq!(q.best_available_cache_read(), None);
    }

    #[test]
    fn effective_cache_read_prefers_published_then_assumes() {
        let mut q = test_quote("glm-5.3", 2.0, true);
        q.cache_read = Some(0.30);
        q.catalog_cache_read = Some(0.20);
        // The selected provider's own ask always wins.
        assert!((q.effective_cache_read(10.0).unwrap() - 0.30).abs() < 1e-12);

        // No provider ask: the market-wide best ask (cheapest published
        // option) beats the catalogue price.
        q.cache_read = None;
        q.market_options = vec![ProviderMarketQuote {
            provider: "dear".into(),
            input: 1.0,
            output: 4.0,
            cache_read: Some(0.25),
            trusted: Some(true),
            healthy_seller_count: Some(3),
        }];
        assert!((q.effective_cache_read(10.0).unwrap() - 0.25).abs() < 1e-12);

        // No market asks at all: the published catalogue rate is the default.
        q.market_options.clear();
        assert!((q.effective_cache_read(10.0).unwrap() - 0.20).abs() < 1e-12);

        // Nothing published: assumed share of the $2 input.
        q.catalog_cache_read = None;
        assert!((q.effective_cache_read(10.0).unwrap() - 0.20).abs() < 1e-12);
        assert!((q.effective_cache_read(25.0).unwrap() - 0.50).abs() < 1e-12);

        // And blended pricing bills that resolved rate, not the input price.
        let blended = q.price(PriceMetric::Blended, 15.0, 80.0, 5.0, 10.0);
        let expected = (2.0 * 15.0 + 0.20 * 80.0 + 4.0 * 5.0) / 100.0;
        assert!((blended - expected).abs() < 1e-12);
    }

    #[test]
    fn workload_discount_includes_cache_read_leg() {
        // Same 50% cut on every leg must give 50% regardless of weights.
        let pct = workload_discount_pct(
            (1.0, Some(0.1), 5.0),
            (2.0, Some(0.2), 10.0),
            (15.0, 80.0, 5.0),
            10.0,
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
            10.0,
        )
        .unwrap();
        let no_cache = workload_discount_pct(
            (2.0, Some(0.02), 10.0),
            (2.0, Some(0.2), 10.0),
            (50.0, 0.0, 50.0),
            10.0,
        )
        .unwrap();
        assert!(cache_heavy > 10.0);
        assert!(no_cache.abs() < 1e-9);
    }

    #[test]
    fn workload_discount_missing_cache_falls_back_to_assumed_share() {
        // No cache price on either side: both fall back to the same assumed
        // share of their input prices, and an all-legs-50% market still
        // yields exactly 50% whatever the share.
        for share in [0.0, 10.0, 100.0] {
            let pct = workload_discount_pct(
                (1.0, None, 5.0),
                (2.0, None, 10.0),
                (15.0, 80.0, 5.0),
                share,
            )
            .unwrap();
            assert!((pct - 50.0).abs() < 1e-9);
        }
        // Market side lacking a cache price while the catalogue has one
        // bills that leg at the assumed share of the market input price
        // (share 100 = the old bill-at-input behavior).
        let pct = workload_discount_pct(
            (1.0, None, 10.0),
            (2.0, Some(0.2), 10.0),
            (0.0, 100.0, 0.0),
            100.0,
        )
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
                10.0,
            )
            .is_none()
        );
    }

    #[test]
    fn liquidity_filters_use_live_market_quality() {
        let q = test_quote("model-a", 1.0, true);
        assert!(
            LiquidityFilter::Trusted
                .apply(&q, 15.0, 80.0, 5.0, 10.0)
                .is_some()
        );
        assert!(
            LiquidityFilter::Healthy3
                .apply(&q, 15.0, 80.0, 5.0, 10.0)
                .is_some()
        );
        assert!(
            LiquidityFilter::Healthy10
                .apply(&q, 15.0, 80.0, 5.0, 10.0)
                .is_none()
        );
        let fallback = test_quote("model-a", 1.0, false);
        assert!(
            LiquidityFilter::Trusted
                .apply(&fallback, 15.0, 80.0, 5.0, 10.0)
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
            let selected = filter.apply(&q, 15.0, 80.0, 5.0, 10.0).unwrap();
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
            .apply(&q, 15.0, 80.0, 5.0, 10.0)
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

    #[test]
    fn uncensored_filter_partitions_by_flag() {
        assert!(UncensoredFilter::All.allows(true));
        assert!(UncensoredFilter::All.allows(false));
        assert!(UncensoredFilter::Uncensored.allows(true));
        assert!(!UncensoredFilter::Uncensored.allows(false));
        assert!(!UncensoredFilter::SafetyTuned.allows(true));
        assert!(UncensoredFilter::SafetyTuned.allows(false));
    }

    #[test]
    fn uncensored_detection_matches_listing_text_anywhere() {
        // The 2026-10-13 Surplus listing: GLM 5.3 weights with refusal
        // directions removed, id-prefixed with the creator.
        assert!(is_uncensored_model(
            "abliteration-abliterated-model-large-v2",
            "Abliterated Large V2",
            "Abliteration AI",
        ));
        // Known uncensored finetunes and Venice's uncensored rebrands.
        assert!(is_uncensored_model(
            "glm-4.7-flash-heretic",
            "GLM 4.7 Flash Heretic",
            "Z.AI"
        ));
        assert!(is_uncensored_model(
            "dolphin-mistral-24b",
            "Dolphin Mistral 24B",
            ""
        ));
        assert!(is_uncensored_model(
            "venice-llama-4",
            "Llama 4 (Venice)",
            "api.venice.ai"
        ));
        // Safety-tuned catalogue names must not trip the detector: no
        // substring of these ids/names/creators matches a marker token.
        for (id, name, creator) in [
            ("mistral-large-4-0", "Mistral: Mistral Large 4", "Mistral"),
            ("glm-5.3", "GLM-5.3", "Z.AI"),
            ("claude-haiku-5.5", "Claude Haiku 5.5", "Anthropic"),
            ("gpt-oss-safeguard-20b", "GPT-OSS Safeguard 20B", "OpenAI"),
            ("meta-muse-wonder", "Meta: Muse Wonder", "Meta"),
        ] {
            assert!(
                !is_uncensored_model(id, name, creator),
                "{name} must not read as uncensored"
            );
        }
    }
}
