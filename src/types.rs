//! Shared data model: settings, alert rules, market quotes, and benchmark
//! rows, plus their small pure helpers (price blending, liquidity filters,
//! labels). No fetching, scoring, or UI code lives in this module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const SWE_REBENCH_URL: &str = "https://swe-rebench.com/";
pub(crate) const REVELO_CODE_INDEX_URL: &str = "https://research.revelo.com/code-index/";
pub(crate) const ARTIFICIAL_ANALYSIS_URL: &str = "https://artificialanalysis.ai/leaderboards/models";

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum AlertMode {
    #[default]
    Threshold,
    AnyChange,
    EntersFrontier,
    LeavesFrontier,
    CheapestAboveScore,
}

impl AlertMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Threshold => "Threshold",
            Self::AnyChange => "Any price change",
            Self::EntersFrontier => "Enters Pareto frontier",
            Self::LeavesFrontier => "Leaves Pareto frontier",
            Self::CheapestAboveScore => "Cheapest above score",
        }
    }

    pub(crate) fn benchmark_dependent(self) -> bool {
        matches!(self, Self::EntersFrontier | Self::LeavesFrontier | Self::CheapestAboveScore)
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
    SWEBenchLive,
    SWEBenchVerified,
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
            Self::SWEBenchLive => "SWE-bench Live",
            Self::SWEBenchVerified => "SWE-bench Verified [legacy]",
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
            Self::SWEBenchLive => "SWE-bench Live",
            Self::SWEBenchVerified => "SWE-bench Verified",
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
        matches!(self, Self::ArtificialAnalysisSnapshot | Self::LiveBench | Self::DesignArena)
    }

    pub(crate) fn is_harness_specific(self) -> bool {
        matches!(
            self,
            Self::TerminalBench3
                | Self::DeepSWE11
                | Self::ReveloCodeIndex
                | Self::SWEBenchLive
                | Self::SWEBenchVerified
        )
    }

    pub(crate) fn remote_sources() -> [Self; 8] {
        [
            Self::SWERebench,
            Self::TerminalBench3,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::SWEBenchLive,
            Self::SWEBenchVerified,
            Self::DesignArena,
        ]
    }

    pub(crate) fn consensus_sources() -> [Self; 9] {
        [
            Self::ArtificialAnalysisSnapshot,
            Self::SWERebench,
            Self::TerminalBench3,
            Self::DeepSWE11,
            Self::LiveBench,
            Self::ReveloCodeIndex,
            Self::SWEBenchLive,
            Self::SWEBenchVerified,
            Self::DesignArena,
        ]
    }

    pub(crate) fn display_sources() -> [Self; 9] {
        Self::consensus_sources()
    }

    pub(crate) fn source_url(self) -> &'static str {
        match self {
            Self::CompositeAgentic | Self::CompositeDeployment => "",
            Self::ArtificialAnalysisSnapshot => ARTIFICIAL_ANALYSIS_URL,
            Self::SWERebench => SWE_REBENCH_URL,
            Self::TerminalBench3 => "https://hub.harborframework.com/datasets/terminal-bench/terminal-bench/latest?tab=leaderboard&leaderboard=3-0-0",
            Self::DeepSWE11 => "https://deepswe.datacurve.ai/",
            Self::LiveBench => "https://livebench.ai/",
            Self::ReveloCodeIndex => REVELO_CODE_INDEX_URL,
            Self::SWEBenchLive => "https://swe-bench-live.github.io/",
            Self::SWEBenchVerified => "https://www.swebench.com/",
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
        // provider is available in the same Surplus market row.
        let qualifies = |option: &ProviderMarketQuote| match self {
            Self::Any => true,
            Self::Trusted => option.trusted == Some(true),
            Self::Healthy3 => option.healthy_seller_count.unwrap_or(0) >= 3,
            Self::Healthy10 => option.healthy_seller_count.unwrap_or(0) >= 10,
        };
        if let Some(option) = quote
            .market_options
            .iter()
            .filter(|option| qualifies(option))
            .min_by(|a, b| {
                a.workload_price(input_weight, cache_read_weight, output_weight)
                    .total_cmp(&b.workload_price(input_weight, cache_read_weight, output_weight))
            })
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
}

pub(crate) fn default_common_scaffold() -> String { "mini-SWE-agent".into() }
pub(crate) fn default_score_threshold() -> f64 { 50.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) poll_seconds: u64,
    pub(crate) input_weight: f64,
    pub(crate) cache_read_weight: f64,
    pub(crate) output_weight: f64,
    pub(crate) alerts: Vec<AlertRule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_seconds: 30,
            input_weight: 15.0,
            cache_read_weight: 80.0,
            output_weight: 5.0,
            alerts: vec![],
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
    pub(crate) fn workload_price(&self, input_weight: f64, cache_read_weight: f64, output_weight: f64) -> f64 {
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
    (input * input_weight + cache_price * cache_read_weight + output * output_weight)
        / total_weight
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
    fn blended_price_is_normalized_and_cache_aware() {
        let price = blended_price(2.0, Some(0.2), 10.0, 15.0, 80.0, 5.0);
        assert!((price - 0.96).abs() < 1e-12);
        let fallback = blended_price(2.0, None, 10.0, 15.0, 80.0, 5.0);
        assert!((fallback - 2.4).abs() < 1e-12);
    }

    #[test]
    fn liquidity_filters_use_live_market_quality() {
        let q = test_quote("model-a", 1.0, true);
        assert!(LiquidityFilter::Trusted.apply(&q, 15.0, 80.0, 5.0).is_some());
        assert!(LiquidityFilter::Healthy3.apply(&q, 15.0, 80.0, 5.0).is_some());
        assert!(LiquidityFilter::Healthy10.apply(&q, 15.0, 80.0, 5.0).is_none());
        let fallback = test_quote("model-a", 1.0, false);
        assert!(LiquidityFilter::Trusted.apply(&fallback, 15.0, 80.0, 5.0).is_none());
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
