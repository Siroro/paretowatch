//! Bundled Artificial Analysis Intelligence Index snapshot.
//!
//! This data lives apart from `main.rs` so the table can be bumped without
//! touching any fetch/parse code. It is never presented as live data: the UI
//! labels it as a static snapshot and the composite treats it as a broad
//! capability prior that works without an API key.
//!
//! # Keeping the snapshot up to date
//!
//! 1. Open the Artificial Analysis Intelligence Index for [`ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION`]
//!    and read each relevant model page.
//! 2. Store ONE representative score per model family:
//!    - Prefer the best reasoning/thinking variant that Surplus actually lists;
//!      fall back to instruct/non-reasoning numbers only when no reasoning run
//!      exists.
//!    - Deployment wrappers (E2EE/web/private/fast/highspeed) intentionally
//!      inherit the base score because benchmark matching strips those tokens,
//!      so never add separate rows for them.
//!    - When AA only rates a sibling variant, proxy it and mark the row with
//!      `~proxy`; pure AA estimates get `~est`.
//!    - SKUs that are execution modes of an already-rated model (same weights,
//!      for example `reasoning.mode: "pro"`) get one row per purchasable SKU
//!      with the base score, and MUST be listed in
//!      [`INHERITED_EXECUTION_MODE_NOTES`] so the UI discloses the inheritance.
//! 3. Bump [`ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION`] and
//!    [`ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE`] together with the values.
//! 4. Run `cargo test`: the tests below reject duplicate normalized keys,
//!    out-of-range scores, and accidental row loss during bumps.
//!
//! Deliberately unrated (no AA presence as of 2026-08-26): Aion 2.0, Hermes 3
//! 405B, Palmyra Vision 7B, Qwen 2.5 7B, GPT-OSS Safeguard 20B/120B, Grok 4.20
//! Multi-Agent Beta, Venice rebrands, and uncensored finetunes such as GLM 4.7
//! Flash Heretic. Unrated models fall back to the composite's small neutral
//! prior for the missing evidence instead of renormalizing it away, so absent
//! rows cost coverage confidence but never invent capability.

use crate::{Benchmark, BenchmarkKind, score_benchmark};

pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE: &str = "2026-08-26";
pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION: &str = "v4.1.2";

/// `(model name, AA Intelligence score)` pairs, one representative row per
/// family. Names should mirror Surplus display names so exact-key joins hit
/// before fuzzy matching has to.
pub(crate) const SNAPSHOT_ROWS: &[(&str, f64)] = &[
    ("Claude Opus 5", 63.0),
    ("Claude Fable 5", 62.0),
    ("GPT-5.6 Sol", 61.0),
    ("Grok 4.6", 61.0),
    ("Kimi K3", 60.0),
    ("GLM-5.3", 60.0),
    // Released just after the v4.1.2 snapshot was read; AA has since
    // published its Intelligence Index score for it.
    ("GLM-5.3-Flash", 57.0),
    ("Qwen3.8 Max", 58.0),
    ("Qwen3.8 2.4T A95B", 58.0),
    ("GPT-5.6 Terra", 57.0),
    ("Muse Spark 1.2", 57.0),
    ("Gemini 3.7 Flash", 56.0),
    ("Grok 4.5", 56.0),
    ("Claude Sonnet 5", 55.0),
    ("GPT-5.5", 55.0),
    ("Claude Opus 4.8", 57.0),
    ("Claude Opus 4.7", 55.0),
    ("Muse Spark 1.1", 53.0),
    ("DeepSeek V4 Pro 0813", 53.0),
    ("GLM-5.2", 53.0),
    ("GPT-5.4", 53.0),
    ("GPT-5.4 Mini", 41.0),
    ("GPT-5.4 Nano", 40.0),
    ("GPT-5.2", 43.0),
    ("GPT-5 Mini", 26.0),
    ("DeepSeek V4 Flash 0731", 52.0),
    ("GPT-5.6 Luna", 52.0),
    ("Gemini 3.5 Flash", 52.0),
    ("Gemini 3.1 Pro Preview", 48.0),
    ("Gemini 3.1 Flash-Lite", 26.0),
    ("Gemini 3 Flash Preview", 39.0),
    ("Claude Sonnet 4.6", 48.0),
    ("MiniMax-M3", 45.0),
    ("MiniMax-M2.7", 39.0),
    ("MiniMax-M2.5", 34.0),
    ("MiniMax-M2.1", 32.0),
    ("MiniMax-M2", 29.0),
    ("DeepSeek V4 Pro", 45.0),
    ("Kimi K2.6", 45.0),
    ("Claude Opus 4.6", 45.0),
    ("Qwen3.8 27B", 52.0),
    ("Kimi K2.7 Code", 43.0),
    ("Kimi K2 Thinking", 33.0),
    ("Kimi K2", 20.0),
    ("MiMo-V2.5-Pro", 43.0),
    ("Hy3", 42.0),
    ("DeepSeek V4 Flash", 42.0),
    ("GLM-5.1", 41.0),
    ("GLM-5", 41.0),
    ("GLM-4.7", 34.0),
    ("GLM-4.7-Flash", 23.0),
    ("GLM-4.6", 29.0),
    // Older GLM generations were missing from earlier snapshots, which made
    // them look artificially strong in coverage-first composites (they
    // renormalized away the AA weight entirely). Values were read from the AA
    // v4.1.1 model pages on 2026-08-26.
    ("GLM-4.5", 20.0),
    ("GLM-4.5-Air", 17.0),
    ("Grok 4.3", 38.0),
    ("Grok 4.1 Fast", 31.0),
    ("Grok 4", 34.0),
    ("NVIDIA Nemotron 3 Ultra 550B A55B", 38.0),
    ("Claude 4.5 Sonnet", 37.0),
    ("Claude Opus 4.5", 36.0),
    ("Kimi K2.5", 36.0),
    ("Qwen3.5 397B A17B", 34.0),
    ("Qwen3.5 122B A10B", 33.0),
    ("DeepSeek V3.2", 33.0),
    ("DeepSeek V3.1", 21.0),
    ("DeepSeek V3.1 Terminus", 22.0),
    ("DeepSeek R1 0528", 20.0),
    ("DeepSeek R1", 19.0),
    ("Qwen3.6 27B", 38.0),
    ("Qwen3.6 35B A3B", 32.0),
    ("Qwen3.5 35B A3B", 30.0),
    ("Qwen3 Next 80B A3B Instruct", 14.0),
    ("Qwen3 Coder Next", 21.0),
    ("Qwen3 Coder 480B", 18.0),
    ("Qwen3 Coder 480B Turbo", 18.0),
    ("Gemma 4 31B", 30.0),
    ("Gemma 4 26B A4B", 26.0),
    ("Claude 4.5 Haiku", 30.0),
    ("GPT-5.5 Instant", 29.0),
    ("gpt-oss-120b", 24.0),
    ("gpt-oss-20b", 15.0),
    ("Qwen3 235B A22B 2507", 20.0),
    ("Mistral Small 3.2", 11.0),
    ("Mistral Small 3.2 24B Instruct", 11.0),
    ("Mistral Small 4", 20.0),
    ("Mistral Large 3", 16.0),
    ("Llama 4 Scout", 10.0),
    ("Llama 3.3 70B", 9.0),
    //
    // Backfill 2026-08-26: remaining Surplus catalog text models rated from
    // their AA v4.1.1 model pages. `~proxy` rows reuse the closest variant AA
    // does rate; `~est` rows are AA estimates.
    //
    // OpenAI
    ("GPT-4o", 11.0),
    ("GPT-4o Mini", 7.0),
    ("GPT-5 Nano", 20.0),
    ("GPT-5.2 Codex", 41.0),
    ("GPT-5.3 Codex", 46.0),
    ("GPT-5.6 Sol Pro", 61.0), // same weights as GPT-5.6 Sol; see INHERITED_EXECUTION_MODE_NOTES
    ("GPT-5.6 Terra Pro", 57.0), // same weights as GPT-5.6 Terra; see INHERITED_EXECUTION_MODE_NOTES
    ("GPT-5.6 Luna Pro", 52.0),  // same weights as GPT-5.6 Luna; see INHERITED_EXECUTION_MODE_NOTES
    // Google
    ("Gemini 2.5 Pro", 26.0),
    ("Gemini 2.5 Flash", 14.0),
    // xAI
    ("Grok 4.20 Beta", 38.0),
    ("Grok Build 0.1", 41.0),
    ("Grok Code Fast 1", 22.0),
    // Z AI
    ("GLM-4.7-Thinking", 34.0),
    ("GLM-5 Turbo", 39.0),
    ("GLM 5V Turbo", 35.0),
    // Meta
    ("Llama 3.2 3B", 4.0), // ~est
    // Gemma open models
    ("Gemma 3 27B", 7.0),
    ("Gemma 3 12B", 6.0),
    ("Gemma 3 4B", 1.0),
    ("Gemma 4 E2B", 10.0),
    // Mistral family
    ("Magistral Small 2509", 11.0),
    ("Ministral 14B 3.0", 11.0),
    ("Ministral 3 8B", 9.0),
    ("Ministral 3B", 7.0),
    ("Devstral 2 123B", 19.0),
    // NVIDIA
    ("NVIDIA Nemotron 3 Super 120B", 26.0),
    ("NVIDIA Nemotron Cascade 2 30B", 18.0), // ~est
    ("NVIDIA Nemotron 3 Nano 30B A3B", 7.0),
    ("NVIDIA Nemotron Nano 12B v2", 9.0), // ~est: VL Reasoning (non-reasoning is 4)
    ("NVIDIA Nemotron Nano 9B v2", 7.0),
    // Qwen
    ("Qwen 3.7 Max", 47.0),
    ("Qwen 3.7 Plus", 39.0),
    ("Qwen3.5 Plus", 31.0),  // ~proxy: Qwen3.5 Omni Plus
    ("Qwen3.5 Flash", 19.0), // ~proxy: Qwen3.5 Omni Flash
    ("Qwen 3.5 9B", 22.0),
    ("Qwen3 VL 235B A22B", 14.0),
    ("Qwen3 235B A22B Thinking 2507", 20.0),
    ("Qwen3 32B", 11.0),
    ("Qwen3 30B A3B", 9.0), // reasoning variant (instruct is 7)
    ("Qwen3 Coder 30B A3B Instruct", 14.0),
    // Others
    ("Mercury 2", 22.0),
    ("Trinity Large Thinking", 19.0),
];

/// Rows sold by Surplus as separate SKUs that are actually execution modes of
/// the same weights (for example OpenAI serves `gpt-5.6-sol-pro` as the Sol
/// model with `reasoning.mode: "pro"`). Each needs its own snapshot row so
/// Surplus prices join, but no leaderboard has measured the mode itself, so the
/// score is inherited from the base variant. `(execution-mode SKU, base SKU)`
/// pairs; `main.rs` uses this table to resolve the mode's canonical model key
/// to the base variant when joining live leaderboard rows, and the builder
/// appends an inheritance note to the rendered row name.
pub(crate) const INHERITED_EXECUTION_MODES: &[(&str, &str)] = &[
    ("GPT-5.6 Sol Pro", "GPT-5.6 Sol"),
    ("GPT-5.6 Terra Pro", "GPT-5.6 Terra"),
    ("GPT-5.6 Luna Pro", "GPT-5.6 Luna"),
];

pub(crate) fn artificial_analysis_snapshot() -> Vec<Benchmark> {
    SNAPSHOT_ROWS.iter()
        .map(|(model, score)| {
            let mut row = score_benchmark(
                *model,
                *score,
                None,
                None,
                BenchmarkKind::Model,
            );
            let inherited = INHERITED_EXECUTION_MODES.iter()
                .find(|(mode, _)| *mode == *model)
                .map(|(_, base)| *base);
            row.name = match inherited {
                Some(base) => format!(
                    "{} [AA Intelligence {} · snapshot {} · inherited from {} (max effort); pro mode not separately benchmarked]",
                    model,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
                    base,
                ),
                None => format!(
                    "{} [AA Intelligence {} · snapshot {}]",
                    model,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
                ),
            };
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark_model_key;

    #[test]
    fn rows_are_well_formed() {
        for (model, score) in SNAPSHOT_ROWS {
            assert!(!model.trim().is_empty(), "blank row name");
            assert!(
                score.is_finite() && *score >= 0.0 && *score <= 100.0,
                "{model}: implausible score {score}",
            );
        }
    }

    #[test]
    fn rows_have_unique_normalized_keys() {
        let mut seen = std::collections::HashSet::new();
        for (model, _) in SNAPSHOT_ROWS {
            let key = benchmark_model_key(model);
            assert!(!key.is_empty(), "{model}: normalizes to an empty key");
            assert!(
                seen.insert(key.clone()),
                "duplicate normalized key `{key}` ({model}); collapse_benchmark_rows would silently drop one",
            );
        }
    }

    #[test]
    fn snapshot_keeps_growing_with_each_bump() {
        assert!(
            SNAPSHOT_ROWS.len() >= 120,
            "row count dropped to {}; a bump must never remove existing families",
            SNAPSHOT_ROWS.len(),
        );
    }

    #[test]
    fn snapshot_metadata_is_plausible() {
        assert_eq!(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE.len(), 10);
        assert!(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE.starts_with("20"));
        assert!(ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION.starts_with('v'));
    }

    #[test]
    fn builder_tags_every_row_with_snapshot_provenance() {
        let rows = artificial_analysis_snapshot();
        assert_eq!(rows.len(), SNAPSHOT_ROWS.len());
        assert!(rows.iter().all(|row| {
            row.name.contains("AA Intelligence")
                && row.name.contains(ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION)
                && row.name.contains(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE)
        }));
    }

    #[test]
    fn inherited_mode_pairs_reference_real_rows_and_disclose_in_name() {
        let names: std::collections::HashSet<_> =
            SNAPSHOT_ROWS.iter().map(|(model, _)| *model).collect();
        assert!(!INHERITED_EXECUTION_MODES.is_empty());
        for (mode, base) in INHERITED_EXECUTION_MODES {
            assert!(
                names.contains(mode),
                "pair references unknown mode SKU `{mode}`"
            );
            assert!(
                names.contains(base),
                "pair references unknown base variant `{base}`"
            );
            assert_ne!(
                benchmark_model_key(mode),
                benchmark_model_key(base),
                "{mode} must normalize differently from {base}"
            );
        }
        let rows = artificial_analysis_snapshot();
        for (mode, base) in INHERITED_EXECUTION_MODES {
            let row = rows
                .iter()
                .find(|row| benchmark_model_key(&row.slug) == benchmark_model_key(mode))
                .unwrap();
            assert!(
                row.name.contains(base) && row.name.contains("not separately benchmarked"),
                "`{}` name does not disclose its inheritance from {base}",
                row.slug
            );
        }
    }
}
