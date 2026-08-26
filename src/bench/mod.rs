//! Benchmark engine: cross-source row matching, composite scoring, and
//! per-source view helpers consumed by the UI and fetch layers.

pub(crate) mod matching;
pub(crate) mod scoring;

pub(crate) use matching::*;
pub(crate) use scoring::*;

use std::collections::HashMap;

use crate::types::{
    default_common_scaffold, Benchmark, BenchmarkKind, BenchmarkSource, ComparisonMode,
};

pub(crate) fn benchmark_row_allowed(row: &Benchmark, mode: ComparisonMode, common_scaffold: &str) -> bool {
    match mode {
        ComparisonMode::ModelCapability => row.kind == BenchmarkKind::Model,
        ComparisonMode::BestAvailableAgent => true,
        ComparisonMode::CommonScaffold => row
            .agent
            .as_deref()
            .map(|agent| scaffold_matches(agent, common_scaffold))
            .unwrap_or(false),
    }
}

pub(crate) fn scaffold_matches(agent: &str, requested: &str) -> bool {
    let a = normalize(agent);
    let b = normalize(requested);
    !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a))
}

pub(crate) fn available_scaffolds(sets: &HashMap<BenchmarkSource, Vec<Benchmark>>) -> Vec<String> {
    let mut by_key: HashMap<String, String> = HashMap::new();
    for rows in sets.values() {
        for row in rows {
            let Some(agent) = row.agent.as_deref().map(str::trim).filter(|s| !s.is_empty()) else { continue };
            let key = normalize(agent);
            if !key.is_empty() {
                by_key.entry(key).or_insert_with(|| agent.to_owned());
            }
        }
    }
    let preferred = [
        "mini-SWE-agent", "SWE-agent", "Claude Code", "Codex", "OpenHands",
        "Terminus-2", "Cursor", "Junie", "AMI Agent", "Slingshot",
    ];
    let mut out = Vec::new();
    let mut used = std::collections::HashSet::new();
    for wanted in preferred {
        if let Some((key, value)) = by_key.iter().find(|(_, value)| scaffold_matches(value, wanted)) {
            if used.insert(key.clone()) { out.push(value.clone()); }
        }
    }
    let mut rest = by_key.into_iter()
        .filter(|(key, _)| !used.contains(key))
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    rest.sort_by_key(|v| normalize(v));
    out.extend(rest);
    if out.is_empty() { out.push(default_common_scaffold()); }
    out
}

pub(crate) fn collapse_benchmark_rows(
    rows: &[Benchmark],
    mode: ComparisonMode,
    common_scaffold: &str,
) -> Vec<Benchmark> {
    let mut best: HashMap<String, Benchmark> = HashMap::new();
    for row in rows.iter().filter(|row| benchmark_row_allowed(row, mode, common_scaffold)) {
        let Some(score) = row.agentic_coding else { continue };
        if !score.is_finite() { continue; }
        let key = benchmark_model_key(&row.slug);
        if key.is_empty() { continue; }
        let replace = best
            .get(&key)
            .and_then(|old| old.agentic_coding)
            .map(|old| score > old)
            .unwrap_or(true);
        if replace {
            best.insert(key, row.clone());
        }
    }
    let mut out = best.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| b.agentic_coding.unwrap_or_default().total_cmp(&a.agentic_coding.unwrap_or_default()));
    out
}

pub(crate) fn benchmarks_for_source(
    source: BenchmarkSource,
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
    mode: ComparisonMode,
    common_scaffold: &str,
) -> Vec<Benchmark> {
    if let Some(flavor) = source.composite_flavor() {
        return build_agentic_composite(sets, mode, common_scaffold, flavor);
    }
    let Some(rows) = sets.get(&source) else { return Vec::new(); };
    let effective_mode = if source == BenchmarkSource::ArtificialAnalysisSnapshot {
        ComparisonMode::ModelCapability
    } else {
        mode
    };
    let collapsed = collapse_benchmark_rows(rows, effective_mode, common_scaffold);
    // Some useful leaderboards (notably Revelo Code Index and SWE-bench Live)
    // only publish model+agent rows. In "Model capability" mode, do not make
    // those sources appear completely unmapped: fall back to the best observed
    // harness row per canonical model and keep the upstream agent in metadata.
    if collapsed.is_empty() && mode == ComparisonMode::ModelCapability {
        collapse_benchmark_rows(rows, ComparisonMode::BestAvailableAgent, common_scaffold)
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score_benchmark;
    use crate::testfix::test_quote;
    use crate::types::{BenchmarkKind, BenchmarkSource, ComparisonMode};
    use std::collections::HashMap;

    #[test]
    fn common_scaffold_mode_selects_only_matching_harness() {
        let rows = vec![
            score_benchmark("GPT-5.6 Sol", 70.0, Some("Codex".into()), None, BenchmarkKind::ModelAgent),
            score_benchmark("GPT-5.6 Sol", 60.0, Some("mini-SWE-agent".into()), None, BenchmarkKind::Model),
        ];
        let collapsed = collapse_benchmark_rows(&rows, ComparisonMode::CommonScaffold, "mini-SWE-agent");
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].agent.as_deref(), Some("mini-SWE-agent"));
    }

    #[test]
    fn agent_only_benchmarks_still_map_in_model_capability_mode() {
        let mut sets = HashMap::new();
        sets.insert(BenchmarkSource::ReveloCodeIndex, vec![
            score_benchmark("gpt-5.6-sol", 51.2, Some("codex".into()), None, BenchmarkKind::ModelAgent),
        ]);
        let rows = benchmarks_for_source(
            BenchmarkSource::ReveloCodeIndex,
            &sets,
            ComparisonMode::ModelCapability,
            "mini-SWE-agent",
        );
        assert_eq!(rows.len(), 1);
        let q = test_quote("gpt-5.6-sol", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }

}
