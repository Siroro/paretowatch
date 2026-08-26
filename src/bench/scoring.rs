//! Composite scoring engine: percentile calibration, evidence weighting,
//! priors, and the capability/deployment composites, plus the cross-source
//! consensus view shown under quotes.

use std::collections::HashMap;

use crate::artificial_analysis_snapshot::INHERITED_EXECUTION_MODES;
use crate::bench::matching::{benchmark_model_key, best_benchmark_match};
use crate::types::{
    Benchmark, BenchmarkKind, BenchmarkSource, ComparisonMode, CompositeFlavor, Quote,
    HARNESS_DEMOTION_FACTOR,
};
use super::{benchmarks_for_source, collapse_benchmark_rows};

/// Percentile ranks are only comparable across sources when they are computed
/// against comparable model populations. Each leaderboard covers a different
/// slice of the ecosystem (AA spans 100+ models including many sub-frontier
/// ones; agentic boards cover a few dozen frontier models), so a raw
/// within-source percentile is not interchangeable between boards.
pub(crate) const MIN_ANCHORED_POPULATION: usize = 10;

/// Fixed calibration landmarks mapping raw AA Intelligence scores to a stable
/// 0..100 capability percentile. Empirical within-snapshot ranks throw away
/// magnitude: because the snapshot is bottom-heavy with legacy models, scores
/// 42 and 53 both land in the top quartile by rank even though they differ by
/// 11 points of measured intelligence. Interpolating over these hand-picked
/// anchors keeps AA comparable with the other boards while preserving how far
/// apart two models actually are. Bump together with snapshot bumps if the
/// score scale shifts.
pub(crate) const AA_CALIBRATION_POINTS: &[(f64, f64)] = &[
    (63.0, 99.0),
    (55.0, 90.0),
    (50.0, 80.0),
    (45.0, 68.0),
    (40.0, 55.0),
    (35.0, 42.0),
    (30.0, 30.0),
    (25.0, 22.0),
    (20.0, 15.0),
    (15.0, 10.0),
    (10.0, 6.0),
    (5.0, 3.0),
    (0.0, 0.5),
];

pub(crate) fn calibrated_aa_percentile(score: f64) -> f64 {
    let points = AA_CALIBRATION_POINTS;
    if score >= points[0].0 {
        return points[0].1;
    }
    for window in points.windows(2) {
        let (hi_score, hi_pct) = window[0];
        let (lo_score, lo_pct) = window[1];
        if score >= lo_score {
            let t = (score - lo_score) / (hi_score - lo_score);
            return lo_pct + t * (hi_pct - lo_pct);
        }
    }
    points[points.len() - 1].1
}

/// Total base weight of the agentic (non-AA) sources that can contribute
/// evidence on top of the AA prior: rebench .20 + TB3 .15 + DeepSWE .15 +
/// LiveBench .10 + SWE-bench Live .075 + Revelo .05 + Verified .025.
pub(crate) const AGENTIC_EVIDENCE_TOTAL_WEIGHT: f64 = 0.75;

/// Weight of the AA prior inside the posterior. Small on purpose: among the
/// frontier models this chart actually compares, AA is a weak discriminator
/// (1 point of Intelligence Index apart), so the boards must decide. Models
/// AA has not measured fall back to the same small neutral prior.
pub(crate) const PRIOR_WEIGHT: f64 = 0.10;

/// Missing-board skepticism. A loaded board that did not evaluate a model is
/// evidence absence, not evidence of mediocrity — but treating it as pure
/// absence lets a hot new model top the two boards it appears on and outrank
/// models with broad, strong coverage. Each missing board therefore adds a
/// pseudo-observation AT the model's prior percentile with this fraction of
/// the board's usual weight. Full-coverage models are unaffected; a model
/// missing half the evidence universe gets pulled meaningfully toward its
/// prior. Boards with no rows at all (feed not loaded) add nothing: that is
/// missing evidence for everyone, not a per-model signal.
pub(crate) const MISSING_EVIDENCE_FRACTION: f64 = 0.25;

/// Confidence discount for models no board has evaluated yet. Their score is
/// pure AA prior; without this discount a newcomer's unverified standing sits
/// ABOVE models whose boards measurably drag them (a brand-new model with AA
/// 58 outranked GPT-5.6 Sol, who tops DeepSWE, because Sol's real boards cost
/// him while the newcomer's absence cost nothing). Ordering among newcomers
/// is preserved; only the confidence in their standing is reduced.
pub(crate) const ZERO_EVIDENCE_CONFIDENCE: f64 = 0.75;

/// Coverage at which evidence confers full confidence — the same threshold
/// `evidence_tier` uses for "strong evidence".
pub(crate) const STRONG_EVIDENCE_COVERAGE: f64 = 0.60;

/// Evidence confidence ramps linearly from `ZERO_EVIDENCE_CONFIDENCE` at no
/// board coverage to 1.0 at strong coverage, and the final score shrinks
/// toward the neutral midpoint by it. The zero-board case is exactly the
/// historical discount; partial coverage now gets a proportional version —
/// a two-board model whose every percentile sits at its prior was carrying
/// full confidence, which let thin evidence outrank broad measured coverage
/// (GLM-5.3 above GPT-5.6 Sol on 2-of-7 boards).
pub(crate) fn evidence_confidence(coverage: f64) -> f64 {
    ZERO_EVIDENCE_CONFIDENCE
        + (1.0 - ZERO_EVIDENCE_CONFIDENCE) * (coverage / STRONG_EVIDENCE_COVERAGE).min(1.0)
}

/// Percentile precision scales with population: a #1-of-1 leaderboard row is
/// nearly uninformative while a 60-model board is a solid measurement. Rank
/// variance shrinks like 1/n, so weights are scaled by n/(n+k): tiny boards
/// fade fast, real boards (n ≥ 20) keep ≥ 90% of their hand-tuned weight.
pub(crate) const POPULATION_SMOOTHING: f64 = 2.0;

pub(crate) fn population_factor(population: usize) -> f64 {
    if population == 0 {
        return 0.0;
    }
    let n = population as f64;
    n / (n + POPULATION_SMOOTHING)
}

/// Huber-style robustness constant: a source whose percentile sits more than
/// 2.5 weighted standard deviations from the consensus starts losing weight,
/// smoothly and symmetrically (high OR low). Replaces the old hard
/// "drop the single lowest" trim, which biased every 4+-source model upward.
pub(crate) const HUBER_K: f64 = 2.5;

pub(crate) fn huber_weight(residual_abs: f64, scale: f64) -> f64 {
    if scale <= 1e-9 {
        return 1.0;
    }
    (HUBER_K * scale / residual_abs).min(1.0)
}

/// A robustness pass only *discloses* a source as downweighted once it lost
/// enough weight to matter visually in the method string.
pub(crate) const DOWNWEIGHT_DISCLOSURE_THRESHOLD: f64 = 0.95;

pub(crate) fn evidence_tier(coverage: f64) -> &'static str {
    if coverage >= 0.60 {
        "strong evidence"
    } else if coverage >= 0.35 {
        "moderate evidence"
    } else {
        "sparse evidence"
    }
}

/// Canonical model keys of execution-mode SKUs (for example `gpt 5 6 sol pro`)
/// mapped to the base variant whose leaderboard evidence they inherit. Modes
/// inherit base rows; the base never inherits mode-specific rows.
pub(crate) fn execution_mode_aliases() -> HashMap<String, String> {
    INHERITED_EXECUTION_MODES
        .iter()
        .filter_map(|(mode, base)| {
            let mode_key = benchmark_model_key(mode);
            let base_key = benchmark_model_key(base);
            (!mode_key.is_empty() && !base_key.is_empty() && mode_key != base_key)
                .then(|| (mode_key, base_key))
        })
        .collect()
}

/// Tie-aware 0..100 percentile per model key within one source's population,
/// optionally restricted to the AA-covered anchor cohort so percentiles from
/// differently-scoped leaderboards land on a shared scale. Also returns the
/// population the ranking used: percentile precision scales with it, so the
/// composite downweights boards whose anchored cohort is tiny.
pub(crate) fn tie_aware_percentiles(
    scores: impl Iterator<Item = (String, f64)>,
    anchor_keys: Option<&std::collections::HashSet<String>>,
) -> (HashMap<String, f64>, usize) {
    let mut ranked: Vec<(String, f64)> = scores.collect();
    if let Some(anchor) = anchor_keys {
        let anchored = ranked
            .iter()
            .filter(|(key, _)| anchor.contains(key))
            .cloned()
            .collect::<Vec<_>>();
        if anchored.len() >= MIN_ANCHORED_POPULATION {
            ranked = anchored;
        }
    }
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    let n = ranked.len();
    let mut map = HashMap::new();
    for (index, (key, score)) in ranked.iter().enumerate() {
        let first = ranked.iter().position(|(_, s)| (*s - *score).abs() <= 1e-9).unwrap_or(index);
        let last = ranked.iter().rposition(|(_, s)| (*s - *score).abs() <= 1e-9).unwrap_or(index);
        let pct = if n <= 1 { 100.0 } else { ((first + last) as f64 / 2.0) / (n as f64 - 1.0) * 100.0 };
        map.insert(key.clone(), pct);
    }
    (map, n)
}

/// AA percentiles on the composite's anchored scale: ranked within the union
/// of models the agentic boards actually evaluate (the elite cohort this
/// chart compares), NOT the broad snapshot population. Models no board has
/// evaluated yet are INSERTION-RANKED into that same cohort — where would
/// this AA score land among the evaluated set — so a brand-new model enters
/// at its elite-cohort standing, never at its broad-population percentile.
/// Returns the ranked population too; None when the cohort is too small to
/// rank against (callers fall back to the fixed calibration curve, which is
/// a different scale and must not be mixed with board percentiles).
pub(crate) fn anchored_aa_percentiles(
    aa_scores: impl Iterator<Item = (String, f64)>,
    board_union: &std::collections::HashSet<String>,
) -> Option<(HashMap<String, f64>, usize)> {
    let all: Vec<(String, f64)> = aa_scores.collect();
    let population: Vec<(String, f64)> = all
        .iter()
        .filter(|(k, _)| board_union.contains(k))
        .cloned()
        .collect();
    if population.len() < MIN_ANCHORED_POPULATION {
        return None;
    }
    let (mut map, n) = tie_aware_percentiles(population.iter().cloned(), None);
    let mut sorted: Vec<f64> = population.iter().map(|(_, s)| *s).collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let total = n + 1;
    for (key, score) in all {
        if map.contains_key(&key) { continue; }
        let less = sorted.iter().filter(|s| **s < score - 1e-9).count();
        let equal = sorted.iter().filter(|s| (**s - score).abs() <= 1e-9).count();
        // Tie-aware midpoint of the newcomer's slot within cohort + itself.
        let first = less + 1;
        let last = less + equal + 1;
        let pct = ((first + last) as f64 / 2.0) / (total as f64 - 1.0) * 100.0;
        map.insert(key, pct);
    }
    Some((map, n))
}

pub(crate) fn aa_anchor_keys(
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
) -> Option<std::collections::HashSet<String>> {
    let rows = collapse_benchmark_rows(
        sets.get(&BenchmarkSource::ArtificialAnalysisSnapshot)?,
        ComparisonMode::ModelCapability,
        "",
    );
    let keys: std::collections::HashSet<String> = rows
        .iter()
        .filter(|row| row.agentic_coding.is_some_and(|s| s.is_finite()))
        .map(|row| benchmark_model_key(&row.slug))
        .filter(|key| !key.is_empty())
        .collect();
    (keys.len() >= MIN_ANCHORED_POPULATION).then_some(keys)
}

/// Re-express one board's within-field percentiles on the anchored elite
/// scale using the field's own quality. Anchoring membership (ranking only
/// against AA-covered models) is not enough: a #5 of 13 in an all-frontier
/// field still reads "68th percentile" while a #8 of 47 in a mixed field
/// reads "84th", so elite fields are underpriced and mixed fields overpriced.
/// The board's AA-covered members carry anchored AA standings; their
/// distribution is the empirical worth of each position in this field:
/// calibrated(m) = AA-standings at the m-th board percentile position.
/// Monotone in board standing, so the board's ordering is preserved; when
/// the board's ordering agrees with AA, calibrated equals each member's own
/// AA standing (the board adds nothing, honestly). Skipped when too few
/// members have anchored standings to characterize the field.
/// Minimum AA-covered members needed to characterize a board's field quality
/// for calibration. Lower than `MIN_ANCHORED_POPULATION` (which guards
/// ranking precision): SWE-rebench — the elite agentic standard — carries
/// only 5 AA-covered members of 13, and skipping its calibration priced
/// #5-of-13-frontier-coders as a generic 68th percentile while boards with
/// broader AA overlap calibrated normally.
pub(crate) const MIN_CALIBRATION_FIELD: usize = 5;

pub(crate) fn calibrate_percentiles_to_field_quality(
    percentiles: &HashMap<String, f64>,
    aa_percentiles: Option<&HashMap<String, f64>>,
) -> HashMap<String, f64> {
    let Some(aa) = aa_percentiles else {
        return percentiles.clone();
    };
    let mut field_q: Vec<f64> = percentiles
        .keys()
        .filter_map(|key| aa.get(key).copied())
        .collect();
    if field_q.len() < MIN_CALIBRATION_FIELD {
        return percentiles.clone();
    }
    field_q.sort_by(|a, b| a.total_cmp(b));
    percentiles
        .iter()
        .map(|(key, p)| (key.clone(), field_quantile(&field_q, *p)))
        .collect()
}

/// Linear interpolation inside the sorted standings of a board's field:
/// the anchored percentile at cumulative position `pct` (0..100).
fn field_quantile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.len() <= 1 {
        return sorted.first().copied().unwrap_or(50.0);
    }
    let t = (pct.clamp(0.0, 100.0) / 100.0) * (sorted.len() - 1) as f64;
    let lo = t.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    sorted[lo] + (t - lo as f64) * (sorted[hi] - sorted[lo])
}

pub(crate) fn composite_weight_factor(source: BenchmarkSource, flavor: CompositeFlavor) -> f64 {
    match flavor {
        CompositeFlavor::Capability => {
            if source.is_harness_specific() {
                HARNESS_DEMOTION_FACTOR
            } else {
                1.0
            }
        }
        CompositeFlavor::Deployment => {
            if source.is_model_level() {
                HARNESS_DEMOTION_FACTOR
            } else {
                1.0
            }
        }
    }
}

/// Sources whose models define the AA prior's anchored cohort. SWE-bench
/// Verified is excluded: it is the explicitly-legacy board (180 rows of
/// mostly superseded models), and letting it contribute would drag 100+
/// legacy AA models into the "elite cohort", inflating every prior and
/// every insertion rank computed against it.
pub(crate) fn prior_cohort_source(source: BenchmarkSource) -> bool {
    !matches!(
        source,
        BenchmarkSource::ArtificialAnalysisSnapshot | BenchmarkSource::SWEBenchVerified
    )
}

pub(crate) fn build_agentic_composite(
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
    mode: ComparisonMode,
    common_scaffold: &str,
    flavor: CompositeFlavor,
) -> Vec<Benchmark> {
    // Coverage-first: keep current, useful text models on screen even when only
    // one leaderboard has evaluated them yet. The composite is a robust
    // precision-weighted posterior:
    //
    //   score = (prior_w·prior_AA + Σ wᵢ·pctᵢ) / (prior_w + Σ wᵢ)
    //
    // - The PRIOR is AA's percentile ranked within the union of models the
    // agentic boards actually evaluate (the same elite cohort the boards are
    // anchored against), with a small weight: broad intelligence barely
    // discriminates between frontier models, so boards decide. Models AA has
    // not measured fall back to the same small neutral prior.
    // - Each agentic source's precision wᵢ combines its hand-tuned base weight
    //   (construct relevance + flavor trust) with a population factor
    //   n/(n+2): percentiles from tiny boards carry almost no information, so
    //   a #1-of-1 row cannot outrank real measurements.
    // - Two Huber passes downweight sources that sit far from the consensus,
    //   symmetrically (high or low), replacing the old always-drop-the-lowest
    //   trim that biased 4+-source models upward.
    let weights = [
        (BenchmarkSource::ArtificialAnalysisSnapshot, 0.25_f64),
        (BenchmarkSource::SWERebench, 0.20_f64),
        (BenchmarkSource::TerminalBench3, 0.15_f64),
        (BenchmarkSource::DeepSWE11, 0.15_f64),
        (BenchmarkSource::LiveBench, 0.10_f64),
        (BenchmarkSource::SWEBenchLive, 0.075_f64),
        (BenchmarkSource::ReveloCodeIndex, 0.05_f64),
        (BenchmarkSource::SWEBenchVerified, 0.025_f64),
    ];
    let aliases = execution_mode_aliases();

    let mut source_scores: HashMap<BenchmarkSource, HashMap<String, (Benchmark, f64)>> = HashMap::new();
    for (source, _) in weights {
        let source_mode = if source == BenchmarkSource::ArtificialAnalysisSnapshot {
            ComparisonMode::ModelCapability
        } else {
            mode
        };
        // Use the shared per-source loader (with the agent-only-board fallback)
        // so the composite sees exactly the same rows the consensus grid and
        // the single-source tabs see. Calling collapse_benchmark_rows directly
        // here silently dropped agent-only boards (Revelo Code Index) from the
        // composite while the consensus grid kept them.
        let collapsed = benchmarks_for_source(source, sets, source_mode, common_scaffold);
        let mut by_model: HashMap<String, (Benchmark, f64)> = HashMap::new();
        for row in collapsed {
            let Some(score) = row.agentic_coding else { continue };
            let key = benchmark_model_key(&row.slug);
            if !key.is_empty() && score.is_finite() {
                by_model.insert(key, (row, score));
            }
        }
        // Execution-mode SKUs (e.g. `...-sol-pro`) inherit the base variant's
        // rows so same-weights SKUs rate identically; native rows for the mode
        // itself always win over inherited ones.
        for (mode_key, base_key) in &aliases {
            if let Some(entry) = by_model.get(base_key).cloned() {
                by_model.entry(mode_key.clone()).or_insert(entry);
            }
        }
        source_scores.insert(source, by_model);
    }

    // Anchor every source to the AA-covered cohort when there is enough overlap:
    // rank each model only against models that AA also evaluated. Without this,
    // a model missing from one source could renormalize away its weakest board
    // and outrank strictly stronger models that have full coverage.
    let anchor = aa_anchor_keys(sets);

    // The AA prior must live on the SAME scale as the board percentiles.
    // The fixed calibration curve ranks against the broad snapshot population
    // (bottom-heavy with legacy models: an Intelligence score of 61 maps to
    // the ~97th percentile), while the boards are anchored to the elite
    // AA-covered cohort. Mixing those scales let the oversized prior compress
    // the differences that should discriminate frontier models from each
    // other. Instead: rank AA empirically WITHIN the union of models the
    // agentic boards actually evaluate — the cohort this chart compares — and
    // keep the calibration curve only as a fallback when that cohort is too
    // small to rank against. Computed before the board pass: each board's
    // percentiles are then calibrated through its own field's AA standings,
    // so a mid-rank in an elite field finally prices above a high rank in a
    // mixed field (no circularity — standings come from AA scores only).
    let mut board_union: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (source, rows) in &source_scores {
        if prior_cohort_source(*source) {
            board_union.extend(rows.keys().cloned());
        }
    }
    let aa_prior_percentiles: Option<HashMap<String, f64>> = source_scores
        .get(&BenchmarkSource::ArtificialAnalysisSnapshot)
        .and_then(|aa_rows| {
            anchored_aa_percentiles(
                aa_rows.iter().map(|(k, (_, s))| (k.clone(), *s)),
                &board_union,
            )
        })
        .map(|(map, _)| map);

    let mut percentiles: HashMap<BenchmarkSource, HashMap<String, f64>> = HashMap::new();
    let mut populations: HashMap<BenchmarkSource, usize> = HashMap::new();
    for (source, _) in weights {
        if source == BenchmarkSource::ArtificialAnalysisSnapshot { continue; }
        let Some(rows) = source_scores.get(&source) else { continue };
        let (map, population) = tie_aware_percentiles(rows.iter().map(|(k, (_, s))| (k.clone(), *s)), anchor.as_ref());
        let map = calibrate_percentiles_to_field_quality(&map, aa_prior_percentiles.as_ref());
        percentiles.insert(source, map);
        populations.insert(source, population);
    }

    let mut all_keys = std::collections::HashSet::new();
    for rows in source_scores.values() {
        all_keys.extend(rows.keys().cloned());
    }

    let mut out = Vec::new();
    for key in all_keys {
        // (source, effective weight, base weight, percentile, raw score)
        let mut contribs: Vec<(BenchmarkSource, f64, f64, f64, f64)> = Vec::new();
        let mut parts: Vec<String> = Vec::new();
        let mut representative: Option<Benchmark> = None;
        let mut task_profile: Option<(f64, String)> = None;

        for (source, base_weight) in weights {
            if source == BenchmarkSource::ArtificialAnalysisSnapshot { continue; }
            let Some((row, raw_score)) = source_scores.get(&source).and_then(|rows| rows.get(&key)) else { continue };
            let Some(percentile) = percentiles.get(&source).and_then(|rows| rows.get(&key)).copied() else { continue };
            // Precision weighting: the hand-tuned base weight encodes construct
            // relevance and flavor trust; the population factor encodes how
            // sharply this board's percentile scale can actually distinguish
            // models (a 1-row board cannot distinguish anything).
            let population = populations.get(&source).copied().unwrap_or(0);
            let weight = base_weight
                * composite_weight_factor(source, flavor)
                * population_factor(population);
            contribs.push((source, weight, base_weight, percentile, *raw_score));
            if representative.is_none() { representative = Some(row.clone()); }
            if task_profile.is_none() {
                if let Some(tokens) = row.tokens_per_task.filter(|tokens| tokens.is_finite() && *tokens > 0.0) {
                    task_profile = Some((
                        tokens,
                        row.token_profile.clone().unwrap_or_else(|| source.short_label().to_owned()),
                    ));
                }
            }
        }

        // Prior selection — the AA percentile is a small informative PRIOR, not
        // a contribution, always on the anchored elite scale: models the boards
        // have evaluated are ranked within that cohort, models they have NOT
        // evaluated are insertion-ranked into it (their expected standing among
        // the evaluated set). The broad calibration curve enters only in the
        // pure-AA world (no board cohort exists yet): there it is the only
        // scale in play, so it stays self-consistent.
        let anchored_prior = aa_prior_percentiles.as_ref().and_then(|rows| rows.get(&key).copied());
        let pure_aa_prior = if aa_prior_percentiles.is_none() && contribs.is_empty() {
            source_scores
                .get(&BenchmarkSource::ArtificialAnalysisSnapshot)
                .and_then(|rows| rows.get(&key))
                .map(|(_, score)| calibrated_aa_percentile(*score))
        } else {
            None
        };
        let (prior_pct, prior_label) = match anchored_prior.or(pure_aa_prior) {
            Some(pct) => (pct, format!("AA {:.0}th", pct)),
            None => (50.0, "neutral 50th".to_owned()),
        };
        let mut prior_weight = PRIOR_WEIGHT;
        let mut missing_boards = 0usize;
        for (source, base_weight) in weights {
            if source == BenchmarkSource::ArtificialAnalysisSnapshot { continue; }
            if contribs.iter().any(|(s, ..)| *s == source) { continue; }
            // Only loaded boards count: a feed with no rows is missing for
            // every model equally and adds no per-model information.
            let population = populations.get(&source).copied().unwrap_or(0);
            if population == 0 { continue; }
            prior_weight += base_weight
                * composite_weight_factor(source, flavor)
                * population_factor(population)
                * MISSING_EVIDENCE_FRACTION;
            missing_boards += 1;
        }

        if anchored_prior.is_none() && pure_aa_prior.is_none() && contribs.is_empty() { continue; }

        // Robust precision-weighted posterior:
        //   score = (prior_w·prior + Σ wᵢ·pctᵢ) / (prior_w + Σ wᵢ)
        // with two Huber passes that smoothly downweight sources disagreeing
        // with the consensus by more than HUBER_K weighted standard deviations
        // — symmetric (a wildly HIGH board loses weight too), unlike the old
        // hard trim that always dropped the lowest percentile.
        let posterior = |w: &[f64]| -> (f64, f64) {
            let mut sum = prior_weight * prior_pct;
            let mut denom = prior_weight;
            for (i, (_, _, _, pct, _)) in contribs.iter().enumerate() {
                sum += w[i] * pct;
                denom += w[i];
            }
            (sum / denom, denom)
        };
        let mut robust_w: Vec<f64> = contribs.iter().map(|(_, w, _, _, _)| *w).collect();
        for _ in 0..2 {
            let (mu, denom) = posterior(&robust_w);
            let mut var = prior_weight * (prior_pct - mu).powi(2);
            for (i, (_, _, _, pct, _)) in contribs.iter().enumerate() {
                var += robust_w[i] * (pct - mu).powi(2);
            }
            let scale = (var / denom).sqrt();
            for i in 0..robust_w.len() {
                let residual = (contribs[i].3 - mu).abs();
                robust_w[i] = contribs[i].1 * huber_weight(residual, scale);
            }
        }
        let (adjusted, _) = posterior(&robust_w);
        // Tier reflects how much real information the agentic universe reported
        // on this model: coverage counts population-scaled weight, so a single
        // 1-row board is nearly no coverage at all (the prior does not count).
        let covered_base: f64 = contribs.iter().map(|(source, _, base, _, _)| {
            base * population_factor(populations.get(source).copied().unwrap_or(0))
        }).sum();
        let coverage = (covered_base / AGENTIC_EVIDENCE_TOTAL_WEIGHT).min(1.0);
        // Confidence scales with coverage: zero-board models keep the
        // historical ZERO_EVIDENCE_CONFIDENCE discount; partial coverage gets
        // a proportional one. Without the ramp, a thin two-board model whose
        // every percentile sits near its prior carried full confidence and
        // outranked broad measured coverage.
        let score = 50.0 + (adjusted - 50.0) * evidence_confidence(coverage);
        let measured = if robust_w.iter().any(|w| *w > 0.0) {
            let mut sum = 0.0;
            let mut denom = 0.0;
            for (i, (_, _, _, pct, _)) in contribs.iter().enumerate() {
                sum += robust_w[i] * pct;
                denom += robust_w[i];
            }
            sum / denom
        } else {
            prior_pct
        };

        for (index, (source, _, _, _, raw_score)) in contribs.iter().enumerate() {
            let downweighted = robust_w[index] < contribs[index].1 * DOWNWEIGHT_DISCLOSURE_THRESHOLD;
            parts.push(if downweighted {
                format!("{} {:.1} (downweighted)", source.short_label(), raw_score)
            } else {
                format!("{} {:.1}", source.short_label(), raw_score)
            });
        }

        let mut representative = representative.unwrap_or(Benchmark {
            slug: key.clone(),
            name: key.clone(),
            creator: String::new(),
            overall: Some(score),
            coding: Some(score),
            agentic_coding: Some(score),
            agent: None,
            reasoning_effort: None,
            kind: BenchmarkKind::Model,
            tokens_per_task: None,
            token_profile: None,
        });
        representative.slug = key;
        let pending = if missing_boards > 0 {
            format!(" · {missing_boards} board{} pending", if missing_boards == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        representative.name = format!(
            "{} [measured {:.1} · adjusted {:.1} · {} · {} sources · prior {}{} · {}]",
            clean_benchmark_display_name(&representative.name),
            measured,
            score,
            evidence_tier(coverage),
            contribs.len(),
            prior_label,
            pending,
            parts.join(" · "),
        );
        representative.overall = Some(score);
        representative.coding = Some(score);
        representative.agentic_coding = Some(score);
        if let Some((tokens, profile)) = task_profile {
            representative.tokens_per_task = Some(tokens);
            representative.token_profile = Some(format!("Composite uses {profile}"));
        }
        out.push(representative);
    }
    out.sort_by(|a, b| b.agentic_coding.unwrap_or_default().total_cmp(&a.agentic_coding.unwrap_or_default()));
    out
}

pub(crate) fn clean_benchmark_display_name(name: &str) -> String {
    name.split(" [").next().unwrap_or(name).trim().to_owned()
}

#[derive(Debug, Clone)]
pub(crate) struct ConsensusEntry {
    pub(crate) source: BenchmarkSource,
    pub(crate) rank: usize,
    pub(crate) total: usize,
    pub(crate) percentile: f64,
    pub(crate) score: f64,
    pub(crate) benchmark_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BenchmarkConsensus {
    pub(crate) entries: Vec<ConsensusEntry>,
}

pub(crate) fn format_percentile(pct: f64) -> String {
    let v = pct.round() as i64;
    let suffix = match v % 10 {
        1 if v % 100 != 11 => "st",
        2 if v % 100 != 12 => "nd",
        3 if v % 100 != 13 => "rd",
        _ => "th",
    };
    format!("{v}{suffix}")
}

pub(crate) fn benchmark_consensus_for_quote(
    quote: &Quote,
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
    mode: ComparisonMode,
    common_scaffold: &str,
) -> Option<BenchmarkConsensus> {
    // Raw ranks are meaningless across leaderboards of very different sizes:
    // #12/12 and #17/87 are wildly different results despite similar numbers.
    // Express every source as a tie-aware percentile over a comparable elite
    // cohort: boards rank within the AA-covered anchor, and AA ranks within
    // the union of models the boards actually evaluate — the same anchored
    // scale the composite prior uses, so the grid's AA percentile agrees with
    // what the composite actually weighed.
    let anchor = aa_anchor_keys(sets);

    let mut rows_by_source: HashMap<BenchmarkSource, Vec<Benchmark>> = HashMap::new();
    let mut keyed_by_source: HashMap<BenchmarkSource, Vec<(String, f64)>> = HashMap::new();
    for source in BenchmarkSource::consensus_sources() {
        let rows = benchmarks_for_source(source, sets, mode, common_scaffold);
        if rows.is_empty() { continue; }
        let keyed: Vec<(String, f64)> = rows
            .iter()
            .filter_map(|row| {
                row.agentic_coding
                    .filter(|s| s.is_finite())
                    .map(|s| (benchmark_model_key(&row.slug), s))
            })
            .filter(|(key, _)| !key.is_empty())
            .collect();
        rows_by_source.insert(source, rows);
        keyed_by_source.insert(source, keyed);
    }

    // AA on the anchored elite scale; the broad calibration curve is only the
    // fallback when the board cohort is too small to rank against.
    let mut board_union: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (source, keyed) in &keyed_by_source {
        if prior_cohort_source(*source) {
            board_union.extend(keyed.iter().map(|(k, _)| k.clone()));
        }
    }
    let aa_ranked: Option<(HashMap<String, f64>, usize)> = keyed_by_source
        .get(&BenchmarkSource::ArtificialAnalysisSnapshot)
        .and_then(|keyed| anchored_aa_percentiles(keyed.iter().cloned(), &board_union));

    let mut entries = Vec::new();
    for source in BenchmarkSource::consensus_sources() {
        let Some(rows) = rows_by_source.get(&source) else { continue };
        let Some(keyed) = keyed_by_source.get(&source) else { continue };
        let Some(matched) = best_benchmark_match(quote, rows) else { continue };
        let Some(score) = matched.agentic_coding else { continue };
        let matched_key = benchmark_model_key(&matched.slug);
        let (percentiles, raw_percentile, population) = if source == BenchmarkSource::ArtificialAnalysisSnapshot {
            match &aa_ranked {
                Some((map, n)) => (map.clone(), None, *n),
                // Anchored cohort too small: fall back to the calibration
                // curve, whose percentile carries no rank meaning, so show
                // the raw snapshot population for honest totals.
                None => {
                    let Some(keyed) = keyed_by_source.get(&source) else { continue };
                    (
                        keyed
                            .iter()
                            .map(|(k, s)| (k.clone(), calibrated_aa_percentile(*s)))
                            .collect(),
                        None,
                        keyed.len(),
                    )
                }
            }
        } else {
            let (raw, population) = tie_aware_percentiles(keyed.iter().cloned(), anchor.as_ref());
            // Same field-quality calibration the composite weighs, so the
            // grid's board percentiles agree with what actually moved the
            // score. Rank stays the raw position in the field.
            let calibrated = calibrate_percentiles_to_field_quality(&raw, aa_ranked.as_ref().map(|(m, _)| m));
            (calibrated, raw.get(&matched_key).copied(), population)
        };
        let Some(percentile) = percentiles.get(&matched_key).copied() else { continue };
        // Rank/total must describe the SAME population the percentile was
        // computed over (the anchored cohort when anchoring applied), not the
        // raw board size — "#3/130" next to an anchored percentile is two
        // different claims glued together. Derive the rank from the tie-aware
        // midpoint percentile: pct = (first+last)/2 / (n-1). Board entries
        // carry a field-calibrated percentile (no longer positional), so the
        // raw positional percentile is used for the rank instead.
        let total = population.max(1);
        let positional = raw_percentile.unwrap_or(percentile);
        let rank = (((100.0 - positional) / 100.0) * (total as f64 - 1.0)).round() as usize + 1;
        // Distinguish rows that differ only by effort (e.g. Sol [max] vs
        // [medium]) — otherwise the grid shows two identical-looking matches.
        let benchmark_name = match matched.reasoning_effort.as_deref() {
            Some(effort) if !effort.is_empty() && effort != "default" => {
                format!("{} [{effort}]", matched.name)
            }
            _ => matched.name.clone(),
        };
        entries.push(ConsensusEntry {
            source,
            rank,
            total,
            percentile,
            score,
            benchmark_name,
        });
    }
    if entries.is_empty() { return None; }
    Some(BenchmarkConsensus {
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::benchmark_model_key;
    use crate::score_benchmark;
    use crate::testfix::test_quote;
    use crate::types::{BenchmarkKind, BenchmarkSource, ComparisonMode};
    use std::collections::HashMap;

    #[test]
    fn composite_prefers_fresh_agentic_sources_and_discloses_method() {
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(BenchmarkSource::SWERebench, vec![mk("GPT-5.6 Sol", 62.3), mk("GLM-5.3", 50.0)]);
        sets.insert(BenchmarkSource::TerminalBench3, vec![mk("GPT-5.6 Sol", 34.6), mk("GLM-5.3", 28.3)]);
        sets.insert(BenchmarkSource::DeepSWE11, vec![mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9)]);
        sets.insert(BenchmarkSource::LiveBench, vec![mk("GPT-5.6 Sol", 70.0), mk("GLM-5.3", 80.0)]);
        sets.insert(BenchmarkSource::SWEBenchLive, vec![mk("GPT-5.6 Sol", 55.0), mk("GLM-5.3", 45.0)]);
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "mini-SWE-agent", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let glm = composite.iter().find(|b| benchmark_model_key(&b.slug) == "glm 5 3").unwrap();
        assert!(sol.agentic_coding.unwrap() > glm.agentic_coding.unwrap());
        // The method must stay disclosed in the row name.
        assert!(sol.name.contains("measured"), "missing measured disclosure: {}", sol.name);
        assert!(sol.name.contains("adjusted"), "missing adjusted disclosure: {}", sol.name);
        assert!(sol.name.contains("prior"), "missing prior disclosure: {}", sol.name);
    }

    #[test]
    fn sparse_evidence_stays_near_neutral_and_broad_coverage_approaches_measured() {
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        // Lone wolf: one strong-looking result on a tiny board plus a middling
        // AA score. No anchored AA cohort exists (one board model), so the
        // calibrated broad-scale AA percentile may NOT enter the posterior —
        // the neutral prior is the only scale-safe anchor.
        sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, vec![mk("Lucky", 42.0)]);
        sets.insert(BenchmarkSource::ReveloCodeIndex, vec![mk("Lucky", 19.6)]);
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let lucky = composite.iter().find(|b| benchmark_model_key(&b.slug) == "lucky").unwrap();
        let lucky_score = lucky.agentic_coding.unwrap();
        assert!(lucky.name.contains("sparse evidence"), "{}", lucky.name);
        // The 1-row Revelo board's 100th percentile carries almost no precision
        // (population factor), so the score must stay near the neutral prior
        // instead of riding the tiny board.
        assert!(lucky_score > 50.0, "score {lucky_score} should sit above the neutral prior but close");
        assert!(lucky_score < 55.0, "score {lucky_score} rode the tiny board: {}", lucky.name);
        assert!(lucky.name.contains("prior neutral 50th"), "{}", lucky.name);
    
        // Broad agentic coverage dominates the neutral prior: the adjusted
        // score must lie between the prior and the measured mean, and land
        // reasonably close to the measurement (fixture boards are tiny, so the
        // population damping keeps the prior's share larger than in production).
        let mut full = HashMap::new();
        full.insert(BenchmarkSource::ArtificialAnalysisSnapshot, vec![mk("Steady", 53.0)]);
        full.insert(BenchmarkSource::SWERebench, vec![mk("Steady", 50.0), mk("r2", 55.0), mk("r3", 48.0)]);
        full.insert(BenchmarkSource::TerminalBench3, vec![mk("Steady", 30.0), mk("t2", 28.0), mk("t3", 22.0)]);
        full.insert(BenchmarkSource::DeepSWE11, vec![mk("Steady", 55.0), mk("d2", 44.0), mk("d3", 40.0)]);
        full.insert(BenchmarkSource::LiveBench, vec![mk("Steady", 56.2), mk("l2", 52.0), mk("l3", 47.0)]);
        full.insert(BenchmarkSource::SWEBenchLive, vec![mk("Steady", 55.0), mk("s2", 45.0), mk("s3", 40.0)]);
        let composite = build_agentic_composite(&full, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let steady = composite.iter().find(|b| benchmark_model_key(&b.slug) == "steady").unwrap();
        // Fixture boards have only 3-4 rows each, so population-scaled coverage
        // caps at "moderate" here; real boards (30+ rows) reach strong.
        assert!(steady.name.contains("moderate evidence"), "{}", steady.name);
        let steady_score = steady.agentic_coding.unwrap();
        let measured = steady.name.split("measured ").nth(1)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap();
        assert!(50.0 - 1e-9 <= steady_score && steady_score <= measured + 1e-9, "adjusted {steady_score} outside [50, {measured}]");
        assert!((steady_score - measured).abs() < 20.0, "adjusted {steady_score} strayed from measured {measured}");
    }

    #[test]
    fn lucky_sparse_board_never_beats_dominant_broader_evidence() {
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        // HY3-shaped: weaker AA, elite-looking on one small board, nothing else.
        sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, vec![mk("Lucky", 42.0), mk("Dominant", 53.0)]);
        sets.insert(
            BenchmarkSource::ReveloCodeIndex,
            vec![mk("Lucky", 19.6), mk("Dominant", 30.8), mk("f1", 12.0), mk("f2", 10.0), mk("f3", 8.0), mk("f4", 6.0)],
        );
        // GLM-5.2-shaped: stronger everywhere they overlap, broader coverage,
        // including one weak harness result that robustness must downweight.
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![mk("Dominant", 62.9), mk("f1", 60.0), mk("f2", 58.0), mk("f3", 55.0), mk("f4", 52.0)],
        );
        sets.insert(BenchmarkSource::TerminalBench3, vec![mk("Dominant", 4.6), mk("f1", 30.0), mk("f2", 26.0)]);
        sets.insert(BenchmarkSource::DeepSWE11, vec![mk("Dominant", 43.8), mk("f1", 42.0), mk("f2", 38.0)]);
        sets.insert(BenchmarkSource::LiveBench, vec![mk("Dominant", 51.8), mk("f1", 50.0), mk("f2", 45.0)]);
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let lucky_row = composite.iter().find(|b| benchmark_model_key(&b.slug) == "lucky").unwrap();
        let dominant_row = composite.iter().find(|b| benchmark_model_key(&b.slug) == "dominant").unwrap();
        let lucky = lucky_row.agentic_coding.unwrap();
        let dominant = dominant_row.agentic_coding.unwrap();
        assert!(
            dominant > lucky,
            "dominant broad evidence ({dominant}) must outrank lucky sparse evidence ({lucky})\nLucky: {}\nDominant: {}",
            lucky_row.name, dominant_row.name,
        );
    }

    #[test]
    fn execution_mode_sku_inherits_base_variant_rows() {
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("GPT-5.6 Sol", 61.0), mk("GPT-5.6 Sol Pro", 61.0)],
        );
        // Only the BASE variant has live harness evidence.
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![mk("gpt-5.6-sol", 62.3), mk("f1", 55.0), mk("f2", 48.0)],
        );
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let base = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let pro = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol pro").unwrap();
        // Same weights, same inherited evidence -> identical rating.
        assert!(
            (base.agentic_coding.unwrap() - pro.agentic_coding.unwrap()).abs() < 1e-9,
            "pro ({}) must rate identically to base ({})",
            pro.agentic_coding.unwrap(),
            base.agentic_coding.unwrap()
        );
        assert!(pro.name.contains("SWE-rebench 62.3"), "pro did not inherit the base row: {}", pro.name);
    }

    #[test]
    fn capability_composite_demotes_harness_rows_but_deployment_does_not() {
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        // Model-level boards love Alpha, harness boards hate it; Beta mirrors.
        // Alpha tops LiveBench, is mid on Terminal-Bench, and bottoms DeepSWE —
        // enough signal for capability weighting to lift him past neutral while
        // deployment weighting (LiveBench demoted, harness full) sinks him.
        sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, vec![mk("Alpha", 90.0), mk("Beta", 40.0)]);
        sets.insert(BenchmarkSource::LiveBench, vec![mk("Alpha", 88.0), mk("Beta", 42.0)]);
        sets.insert(BenchmarkSource::TerminalBench3, vec![mk("Alpha", 28.0), mk("Beta", 80.0), mk("f1", 30.0), mk("f2", 15.0)]);
        sets.insert(BenchmarkSource::DeepSWE11, vec![mk("Alpha", 10.0), mk("Beta", 78.0), mk("f1", 60.0)]);
        let cap = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let dep = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Deployment);
        let cap_a = cap.iter().find(|b| benchmark_model_key(&b.slug) == "alpha").unwrap().agentic_coding.unwrap();
        let dep_a = dep.iter().find(|b| benchmark_model_key(&b.slug) == "alpha").unwrap().agentic_coding.unwrap();
        // Capability follows the model-level board; deployment follows harness boards.
        assert!(cap_a > 50.0, "capability should follow model-level boards, got {cap_a}");
        assert!(dep_a < 50.0, "deployment should follow harness boards, got {dep_a}");
        assert!(cap_a > dep_a, "flavors must diverge: cap {cap_a} vs dep {dep_a}");
    }

    #[test]
    fn broad_top_coverage_outranks_selective_evidence_and_higher_aa() {
        // Regression: GPT-5.6 Sol (AA 61, top of SWE-rebench/TB3/DeepSWE/
        // SWE-bench Live) must outrank GLM-5.3 (AA 60, strong only on
        // LiveBench, mid elsewhere). Mirrors the real cohort shape: boards
        // rank against an elite AA-covered set of ~10 current frontier models.
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0), mk("Grok 4.6", 61.0),
                 mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0), mk("Qwen3.8 Max", 58.0),
                 mk("Claude Sonnet 5", 55.0), mk("GPT-5.5", 55.0), mk("DeepSeek V4 Pro 0813", 53.0),
                 mk("GLM-5.2", 53.0), mk("MiniMax-M3", 45.0)],
        );
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0), mk("Claude Opus 5", 58.0),
                 mk("GLM-5.3", 56.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("Kimi K3", 52.0),
                 mk("Claude Sonnet 5", 50.0), mk("GPT-5.5", 48.0), mk("GLM-5.2", 45.0)],
        );
        sets.insert(
            BenchmarkSource::TerminalBench3,
            vec![mk("GPT-5.6 Sol", 34.6), mk("Claude Opus 5", 33.0), mk("Grok 4.6", 30.0),
                 mk("Kimi K3", 25.0), mk("Claude Sonnet 5", 22.0), mk("Qwen3.8 Max", 20.0), mk("GPT-5.5", 18.0)],
        );
        sets.insert(
            BenchmarkSource::DeepSWE11,
            vec![mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9), mk("Claude Opus 5", 65.0),
                 mk("Grok 4.6", 63.0), mk("Kimi K3", 60.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("GLM-5.2", 50.0)],
        );
        sets.insert(
            BenchmarkSource::LiveBench,
            vec![mk("GLM-5.3", 80.0), mk("Claude Opus 5", 75.0), mk("Kimi K3", 72.0),
                 mk("GPT-5.6 Sol", 70.0), mk("Grok 4.6", 68.0), mk("Qwen3.8 Max", 65.0),
                 mk("Claude Sonnet 5", 60.0), mk("GPT-5.5", 58.0)],
        );
        sets.insert(
            BenchmarkSource::SWEBenchLive,
            vec![mk("GPT-5.6 Sol", 55.0), mk("Claude Opus 5", 50.0), mk("GLM-5.3", 45.0),
                 mk("Grok 4.6", 40.0), mk("DeepSeek V4 Pro 0813", 38.0), mk("GLM-5.2", 30.0)],
        );
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let glm = composite.iter().find(|b| benchmark_model_key(&b.slug) == "glm 5 3").unwrap();
        let sol_score = sol.agentic_coding.unwrap();
        let glm_score = glm.agentic_coding.unwrap();
        assert!(
            sol_score > glm_score,
            "GPT-5.6 Sol ({sol_score}) must outrank GLM-5.3 ({glm_score})\nSol: {}\nGLM: {}",
            sol.name, glm.name,
        );
        // The AA prior must be on the anchored elite scale (well under the
        // ~97th percentile the broad calibration curve would give an AA score
        // of 61), and Sol's higher AA intelligence must keep a higher prior.
        let prior_of = |row: &Benchmark| {
            row.name.split("prior AA ").nth(1)
                .and_then(|rest| rest.split('t').next())
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(f64::NAN)
        };
        let sol_prior = prior_of(sol);
        let glm_prior = prior_of(glm);
        assert!(sol_prior < 90.0, "Sol prior {sol_prior} looks like the broad calibration curve, not the anchored scale");
        assert!(sol_prior > glm_prior, "Sol prior {sol_prior} must exceed GLM prior {glm_prior}");
        assert!(sol.name.contains("strong evidence"), "{}", sol.name);
    }

    #[test]
    fn hot_two_board_model_cannot_outrank_broad_top_coverage() {
        // Regression: a new model topping the couple of boards it appears on
        // (Qwen3.8-2.4T-shaped: AA 58, #1 SWE-rebench, #1 DeepSWE, nothing
        // else) must NOT outrank a model with top-tier evidence across five
        // boards (Sol-shaped). Missing boards add pseudo-evidence at the
        // model's prior, so the narrow leader is pulled back toward its AA
        // percentile while the broad leader keeps its measured standing.
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0), mk("Grok 4.6", 61.0),
                 mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0), mk("Qwen3.8 2.4T A95B", 58.0),
                 mk("Claude Sonnet 5", 55.0), mk("GPT-5.5", 55.0), mk("DeepSeek V4 Pro 0813", 53.0),
                 mk("GLM-5.2", 53.0), mk("MiniMax-M3", 45.0)],
        );
        let rebench = vec![mk("Qwen3.8 2.4T A95B", 63.0), mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0),
                           mk("Claude Opus 5", 58.0), mk("GLM-5.3", 56.0), mk("DeepSeek V4 Pro 0813", 55.0),
                           mk("Kimi K3", 52.0), mk("Claude Sonnet 5", 50.0), mk("GPT-5.5", 48.0)];
        sets.insert(BenchmarkSource::SWERebench, rebench);
        sets.insert(
            BenchmarkSource::TerminalBench3,
            vec![mk("GPT-5.6 Sol", 34.6), mk("Claude Opus 5", 33.0), mk("Grok 4.6", 30.0),
                 mk("Kimi K3", 25.0), mk("Claude Sonnet 5", 22.0), mk("Qwen3.8 Max", 20.0), mk("GPT-5.5", 18.0)],
        );
        let deepswe = vec![mk("Qwen3.8 2.4T A95B", 73.0), mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9),
                           mk("Claude Opus 5", 65.0), mk("Grok 4.6", 63.0), mk("Kimi K3", 60.0),
                           mk("DeepSeek V4 Pro 0813", 55.0), mk("GLM-5.2", 50.0)];
        sets.insert(BenchmarkSource::DeepSWE11, deepswe);
        sets.insert(
            BenchmarkSource::LiveBench,
            vec![mk("GLM-5.3", 80.0), mk("Claude Opus 5", 75.0), mk("Kimi K3", 72.0),
                 mk("GPT-5.6 Sol", 70.0), mk("Grok 4.6", 68.0), mk("Qwen3.8 Max", 65.0),
                 mk("Claude Sonnet 5", 60.0), mk("GPT-5.5", 58.0)],
        );
        sets.insert(
            BenchmarkSource::SWEBenchLive,
            vec![mk("GPT-5.6 Sol", 55.0), mk("Claude Opus 5", 50.0), mk("GLM-5.3", 45.0),
                 mk("Grok 4.6", 40.0), mk("DeepSeek V4 Pro 0813", 38.0), mk("GLM-5.2", 30.0)],
        );
        sets.insert(
            BenchmarkSource::ReveloCodeIndex,
            vec![mk("GPT-5.6 Sol", 51.2), mk("Claude Opus 5", 48.0), mk("Grok 4.6", 40.0),
                 mk("GLM-5.3", 35.0), mk("Kimi K3", 30.0)],
        );
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let qwen = composite.iter().find(|b| benchmark_model_key(&b.slug) == "qwen 3 8 2 4t a95b").unwrap();
        let sol_score = sol.agentic_coding.unwrap();
        let qwen_score = qwen.agentic_coding.unwrap();
        assert!(
            sol_score > qwen_score,
            "broad top coverage ({sol_score}) must outrank narrow board-topping ({qwen_score})\nSol: {}\nQwen: {}",
            sol.name, qwen.name,
        );
        assert!(qwen.name.contains("boards pending"), "missing boards must be disclosed: {}", qwen.name);
    }

    #[test]
    fn zero_board_aa_model_enters_at_elite_cohort_standing_not_broad_percentile() {
        // Regression: a brand-new model with AA coverage but no board rows
        // used to inherit the broad calibration percentile (AA 58 → 93rd vs
        // 130 snapshot models incl. legacy) as its whole score, outranking
        // models with real top-tier evidence. It must instead enter at its
        // insertion rank among the board-evaluated elite cohort.
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0), mk("Grok 4.6", 61.0),
                 mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0), mk("Qwen3.8 2.4T A95B", 58.0),
                 mk("Claude Sonnet 5", 55.0), mk("GPT-5.5", 55.0), mk("DeepSeek V4 Pro 0813", 53.0),
                 mk("GLM-5.2", 53.0), mk("MiniMax-M3", 45.0), mk("Brand New Model", 58.0)],
        );
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0), mk("Claude Opus 5", 58.0),
                 mk("GLM-5.3", 56.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("Kimi K3", 52.0),
                 mk("Claude Sonnet 5", 50.0), mk("GPT-5.5", 48.0), mk("GLM-5.2", 45.0), mk("MiniMax-M3", 30.0)],
        );
        sets.insert(
            BenchmarkSource::DeepSWE11,
            vec![mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9), mk("Claude Opus 5", 65.0),
                 mk("Grok 4.6", 63.0), mk("Kimi K3", 60.0), mk("DeepSeek V4 Pro 0813", 55.0),
                 mk("GLM-5.2", 50.0), mk("Claude Sonnet 5", 48.0), mk("GPT-5.5", 45.0), mk("MiniMax-M3", 40.0)],
        );
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let newcomer = composite.iter().find(|b| benchmark_model_key(&b.slug) == "brand new").unwrap();
        let sol_score = sol.agentic_coding.unwrap();
        let new_score = newcomer.agentic_coding.unwrap();
        // Insertion rank of AA 58 among the 10-board cohort (between the 60s
        // and the 55s) is mid-pack — nowhere near the calibrated 93rd.
        assert!(new_score < 75.0, "newcomer {new_score} should enter mid-pack, not near the calibrated 93rd: {}", newcomer.name);
        assert!(
            sol_score > new_score,
            "Sol ({sol_score}) must outrank an unproven same-AA newcomer ({new_score})\nSol: {}\nNew: {}",
            sol.name, newcomer.name,
        );
        assert!(newcomer.name.contains("prior AA"), "{}", newcomer.name);
    }

    #[test]
    fn consensus_aa_percentile_uses_the_anchored_elite_scale() {
        // Same elite-cohort shape as the Sol/GLM composite regression: boards
        // cover ~10 current frontier models, so AA must be ranked within that
        // cohort (Kimi K3's AA 60 lands mid-pack, ~61st) — not on the broad
        // calibration curve (95.6th against 130 models incl. legacy ones).
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0), mk("Grok 4.6", 61.0),
                 mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0), mk("Qwen3.8 Max", 58.0),
                 mk("Claude Sonnet 5", 55.0), mk("GPT-5.5", 55.0), mk("DeepSeek V4 Pro 0813", 53.0),
                 mk("GLM-5.2", 53.0), mk("MiniMax-M3", 45.0)],
        );
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0), mk("Claude Opus 5", 58.0),
                 mk("GLM-5.3", 56.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("Kimi K3", 52.0),
                 mk("Claude Sonnet 5", 50.0), mk("GPT-5.5", 48.0), mk("GLM-5.2", 45.0)],
        );
        sets.insert(
            BenchmarkSource::DeepSWE11,
            vec![mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9), mk("Claude Opus 5", 65.0),
                 mk("Grok 4.6", 63.0), mk("Kimi K3", 60.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("GLM-5.2", 50.0)],
        );
        sets.insert(
            BenchmarkSource::LiveBench,
            vec![mk("GLM-5.3", 80.0), mk("Claude Opus 5", 75.0), mk("Kimi K3", 72.0),
                 mk("GPT-5.6 Sol", 70.0), mk("Grok 4.6", 68.0), mk("Qwen3.8 Max", 65.0),
                 mk("Claude Sonnet 5", 60.0), mk("GPT-5.5", 58.0)],
        );
        let quote = test_quote("kimi-k3", 1.0, true);
        let consensus = benchmark_consensus_for_quote(&quote, &sets, ComparisonMode::ModelCapability, "").unwrap();
        let aa = consensus.entries.iter().find(|e| e.source == BenchmarkSource::ArtificialAnalysisSnapshot).unwrap();
        assert!(
            aa.percentile < 90.0,
            "AA percentile {:.1} looks like the broad calibration scale, not the anchored elite cohort", aa.percentile,
        );
        assert!(aa.percentile > 50.0, "Kimi K3 should sit mid-pack among the elite cohort, got {:.1}", aa.percentile);
        // Boards still report their own anchored percentiles.
        assert!(consensus.entries.iter().any(|e| e.source == BenchmarkSource::DeepSWE11));
        assert!(consensus.entries.iter().any(|e| e.source == BenchmarkSource::SWERebench));
    }

    #[test]
    fn huber_weights_downweight_outliers_smoothly_and_symmetrically() {
        // Inside the consensus band: full weight.
        assert!((huber_weight(5.0, 10.0) - 1.0).abs() < 1e-12);
        assert!((huber_weight(10.0, 10.0) - 1.0).abs() < 1e-12);
        // Outside 2.5·scale: weight decays proportionally to the excess,
        // identically for high and low outliers.
        assert!((huber_weight(50.0, 10.0) - 0.5).abs() < 1e-12);
        assert!((huber_weight(125.0, 10.0) - 0.2).abs() < 1e-12);
        // Degenerate scale (all percentiles equal): never downweight.
        assert!((huber_weight(1e6, 0.0) - 1.0).abs() < 1e-12);
        // Population precision: tiny boards fade, real boards keep their weight.
        assert!(population_factor(0) < 1e-9);
        assert!(population_factor(1) < 0.6);
        assert!(population_factor(3) < 0.78);
        assert!(population_factor(20) > 0.9);
        assert!(population_factor(60) > 0.95);
    }

    #[test]
    fn catastrophic_harness_outlier_is_downweighted_and_disclosed() {
        // Same elite-cohort shape as the Sol/GLM regression so the anchored-AA
        // prior path runs; one harness catastrophically disagrees with four
        // boards that agree Sol is top-tier.
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0), mk("Grok 4.6", 61.0),
                 mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0), mk("Qwen3.8 Max", 58.0),
                 mk("Claude Sonnet 5", 55.0), mk("GPT-5.5", 55.0), mk("DeepSeek V4 Pro 0813", 53.0),
                 mk("GLM-5.2", 53.0)],
        );
        let rebench = vec![mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0), mk("Claude Opus 5", 58.0),
                           mk("GLM-5.3", 56.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("Kimi K3", 52.0),
                           mk("Claude Sonnet 5", 50.0), mk("GPT-5.5", 48.0), mk("GLM-5.2", 45.0)];
        sets.insert(BenchmarkSource::SWERebench, rebench.clone());
        sets.insert(
            BenchmarkSource::TerminalBench3,
            vec![mk("GPT-5.6 Sol", 1.0), mk("Claude Opus 5", 33.0), mk("Grok 4.6", 30.0),
                 mk("Kimi K3", 25.0), mk("Claude Sonnet 5", 22.0), mk("Qwen3.8 Max", 20.0), mk("GPT-5.5", 18.0)],
        );
        sets.insert(
            BenchmarkSource::DeepSWE11,
            vec![mk("GPT-5.6 Sol", 72.7), mk("GLM-5.3", 66.9), mk("Claude Opus 5", 65.0),
                 mk("Grok 4.6", 63.0), mk("Kimi K3", 60.0), mk("DeepSeek V4 Pro 0813", 55.0), mk("GLM-5.2", 50.0)],
        );
        sets.insert(
            BenchmarkSource::LiveBench,
            vec![mk("GLM-5.3", 80.0), mk("Claude Opus 5", 75.0), mk("Kimi K3", 72.0),
                 mk("GPT-5.6 Sol", 70.0), mk("Grok 4.6", 68.0), mk("Qwen3.8 Max", 65.0),
                 mk("Claude Sonnet 5", 60.0), mk("GPT-5.5", 58.0)],
        );
        let _ = rebench;
        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        assert!(sol.name.contains("(downweighted)"), "expected TB3 downweight disclosure: {}", sol.name);
        // The score must stay near the agreeing cluster, not be wrecked by the
        // outlier (LiveBench legitimately ranks Sol mid-pack, which caps the
        // cluster mean in the low-to-mid 70s).
        assert!(sol.agentic_coding.unwrap() > 70.0, "{}", sol.name);
    }

    #[test]
    fn format_percentile_uses_ordinals() {
        assert_eq!(format_percentile(0.0), "0th");
        assert_eq!(format_percentile(1.2), "1st");
        assert_eq!(format_percentile(2.4), "2nd");
        assert_eq!(format_percentile(3.9), "4th");
        assert_eq!(format_percentile(11.0), "11th");
        assert_eq!(format_percentile(21.0), "21st");
        assert_eq!(format_percentile(57.3), "57th");
        assert_eq!(format_percentile(100.0), "100th");
    }

    #[test]
    fn coverage_first_composite_accepts_a_single_credible_source() {
        let mut sets = HashMap::new();
        sets.insert(
            BenchmarkSource::ArtificialAnalysisSnapshot,
            vec![score_benchmark("DeepSeek V4 Flash 0731", 52.0, None, None, BenchmarkKind::Model)],
        );
        let composite = build_agentic_composite(&sets, ComparisonMode::BestAvailableAgent, "mini-SWE-agent", CompositeFlavor::Capability);
        assert_eq!(composite.len(), 1);
        assert_eq!(benchmark_model_key(&composite[0].slug), "deepseek v4 flash 0731");
    }

    #[test]
    fn field_quality_calibration_prices_elite_fields_above_mixed_fields() {
        // Unit behavior of the position-worth mapping.
        let mk_map = |pairs: &[(&str, f64)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        // Elite field: ten members whose AA standings run 85..99.
        let elite_aa: HashMap<String, f64> = (0..10)
            .map(|i| (format!("e{i}"), 85.0 + i as f64 * (14.0 / 9.0)))
            .collect();
        let elite_p = mk_map(&[
            ("e0", 100.0), ("e1", 90.0), ("e2", 80.0), ("e3", 70.0), ("e4", 60.0),
            ("e5", 50.0), ("e6", 40.0), ("e7", 30.0), ("e8", 20.0), ("e9", 10.0),
        ]);
        // Mixed field: ten members spanning 10..99.
        let mixed_aa: HashMap<String, f64> = (0..10)
            .map(|i| (format!("m{i}"), 10.0 + i as f64 * (89.0 / 9.0)))
            .collect();
        let mixed_p = mk_map(&[
            ("m0", 100.0), ("m1", 90.0), ("m2", 80.0), ("m3", 70.0), ("m4", 60.0),
            ("m5", 50.0), ("m6", 40.0), ("m7", 30.0), ("m8", 20.0), ("m9", 10.0),
        ]);
        let elite = calibrate_percentiles_to_field_quality(&elite_p, Some(&elite_aa));
        let mixed = calibrate_percentiles_to_field_quality(&mixed_p, Some(&mixed_aa));
        // A 60th-percentile standing in the elite field is worth ~93rd on the
        // elite scale; a 60th in a mixed field is worth ~60th.
        assert!(elite["e4"] > 90.0, "elite field 60th percentile calibrated to {}", elite["e4"]);
        assert!((mixed["m4"] - 60.0).abs() < 5.0, "mixed field 60th percentile calibrated to {}", mixed["m4"]);
        // Monotone in standing.
        assert!(elite["e0"] > elite["e4"] && elite["e4"] > elite["e9"]);
        // Too few anchored standings: identity.
        let tiny = mk_map(&[("a", 80.0), ("b", 20.0)]);
        let tiny_aa: HashMap<String, f64> = [("a".to_string(), 90.0), ("b".to_string(), 10.0)].into();
        assert_eq!(calibrate_percentiles_to_field_quality(&tiny, Some(&tiny_aa))["a"], 80.0);
        assert_eq!(calibrate_percentiles_to_field_quality(&tiny, None)["b"], 20.0);
    }

    #[test]
    fn thin_two_board_high_prior_model_lands_below_shared_board_superior() {
        // Regression (live 2026-08-26): GLM-5.3 sat ABOVE GPT-5.6 Sol at 86.2
        // vs 77.7 despite losing the AA prior (60 vs 61), losing the one
        // shared agentic board head-to-head (DeepSWE #5/26 vs #2/26), and
        // appearing on only 2 of 7 boards. GLM's single win (LiveBench, a
        // mixed-strength field) plus full confidence in a near-prior
        // posterior carried it. Field-quality calibration + the coverage
        // confidence ramp must restore the ordering.
        //
        // Fixture mirrors production structure: a wide 50-model AA cohort
        // (63.5 down to 14), SWE-rebench drawn from the TOP of that cohort,
        // LiveBench spanning all of it, DeepSWE the upper-middle, Code Index
        // upper-middle without GLM.
        let mk = |name: &str, score: f64| score_benchmark(name, score, None, None, BenchmarkKind::Model);
        let mut sets = HashMap::new();

        // AA world: 6 heroes + 44 fillers stepping down to AA 14.
        let mut aa_rows = vec![
            mk("Claude Fable 5", 63.5), mk("Claude Opus 5", 63.0), mk("GPT-5.6 Sol", 61.0),
            mk("Grok 4.6", 60.5), mk("GLM-5.3", 60.0), mk("Kimi K3", 60.0),
        ];
        for i in 0..44 {
            aa_rows.push(mk(&format!("aa{i}"), 57.0 - i as f64));
        }
        sets.insert(BenchmarkSource::ArtificialAnalysisSnapshot, aa_rows);

        // SWE-rebench: 13-model ELITE field (top of the AA world). Sol #5.
        // GLM absent (production shape).
        sets.insert(
            BenchmarkSource::SWERebench,
            vec![
                mk("Claude Fable 5", 70.0), mk("Claude Opus 5", 68.0), mk("Kimi K3", 64.0),
                mk("aa0", 63.5), mk("GPT-5.6 Sol", 62.3), mk("Grok 4.6", 61.0),
                mk("aa1", 60.0), mk("aa2", 58.0), mk("aa3", 56.0), mk("aa4", 54.0),
                mk("aa5", 52.0), mk("aa6", 50.0), mk("aa7", 48.0),
            ],
        );

        // DeepSWE: 26-model upper-middle field; the shared board.
        // Sol #2, GLM #5, K3 #4.
        let mut deepswe = vec![
            mk("Claude Fable 5", 76.0), mk("GPT-5.6 Sol", 72.7), mk("Claude Opus 5", 71.0),
            mk("Kimi K3", 69.5), mk("GLM-5.3", 69.0), mk("Grok 4.6", 68.0),
        ];
        for i in 0..20 {
            deepswe.push(mk(&format!("aa{}", 10 + i), 66.0 - i as f64));
        }
        sets.insert(BenchmarkSource::DeepSWE11, deepswe);

        // LiveBench: 47-model field spanning the whole cohort. GLM's one win
        // (#8); Sol #15; K3 #5.
        let mut livebench = vec![
            mk("Claude Fable 5", 70.0), mk("Claude Opus 5", 68.0), mk("Kimi K3", 62.2),
            mk("aa0", 62.0), mk("aa1", 61.5), mk("Grok 4.6", 61.2), mk("aa2", 61.0),
            mk("GLM-5.3", 60.9), mk("aa3", 60.5), mk("aa4", 60.0), mk("aa5", 59.5),
            mk("aa6", 59.0), mk("aa7", 58.5), mk("aa8", 58.2), mk("GPT-5.6 Sol", 56.2),
        ];
        for i in 0..32 {
            livebench.push(mk(&format!("aa{}", 9 + i), 55.0 - i as f64));
        }
        sets.insert(BenchmarkSource::LiveBench, livebench);

        // Code Index: 17-model upper-middle field, Sol #2, K3 #5. GLM absent.
        let mut code_index = vec![
            mk("Claude Fable 5", 55.0), mk("GPT-5.6 Sol", 51.2), mk("aa5", 50.0),
            mk("aa6", 49.5), mk("Kimi K3", 39.6),
        ];
        for i in 0..12 {
            code_index.push(mk(&format!("aa{}", 7 + i), 48.0 - i as f64));
        }
        sets.insert(BenchmarkSource::ReveloCodeIndex, code_index);

        // Loaded boards neither hero appears on: they add pending-board
        // pseudo-evidence for BOTH models (at their own priors).
        sets.insert(
            BenchmarkSource::TerminalBench3,
            (0..9).map(|i| mk(&format!("aa{}", 10 + i), 40.0 - i as f64))
                .chain(std::iter::once(mk("Grok 4.6", 35.0)))
                .collect(),
        );
        sets.insert(
            BenchmarkSource::SWEBenchLive,
            (0..10).map(|i| mk(&format!("aa{}", 5 + i), 45.0 - i as f64)).collect(),
        );
        sets.insert(
            BenchmarkSource::SWEBenchVerified,
            (0..15).map(|i| mk(&format!("aa{}", 20 + i), 40.0 - i as f64)).collect(),
        );

        let composite = build_agentic_composite(&sets, ComparisonMode::ModelCapability, "", CompositeFlavor::Capability);
        let sol = composite.iter().find(|b| benchmark_model_key(&b.slug) == "gpt 5 6 sol").unwrap();
        let glm = composite.iter().find(|b| benchmark_model_key(&b.slug) == "glm 5 3").unwrap();
        let sol_score = sol.agentic_coding.unwrap();
        let glm_score = glm.agentic_coding.unwrap();
        assert!(
            sol_score > glm_score,
            "GPT-5.6 Sol ({sol_score}) must outrank a 2-board GLM-5.3 ({glm_score}) that loses AA, loses DeepSWE head-to-head, and wins only LiveBench\nSol: {}\nGLM: {}",
            sol.name, glm.name,
        );
        // The thin-evidence model must carry the sparse tier and pending
        // boards disclosure.
        assert!(glm.name.contains("sparse evidence"), "{}", glm.name);
        assert!(glm.name.contains("boards pending"), "{}", glm.name);
        assert!(sol.name.contains("strong evidence") || sol.name.contains("moderate evidence"), "{}", sol.name);
    }

    #[test]
    fn evidence_confidence_ramps_from_zero_coverage_to_strong() {
        assert!((evidence_confidence(0.0) - ZERO_EVIDENCE_CONFIDENCE).abs() < 1e-12);
        assert!((evidence_confidence(STRONG_EVIDENCE_COVERAGE) - 1.0).abs() < 1e-12);
        assert!((evidence_confidence(1.0) - 1.0).abs() < 1e-12);
        let quarter = evidence_confidence(0.15);
        assert!(quarter > ZERO_EVIDENCE_CONFIDENCE && quarter < evidence_confidence(0.45));
    }
}
