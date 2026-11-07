//! Cross-source model matching: the fuzzy join between Surplus quotes and
//! benchmark rows, and the curated distinct-build guard that fences it.
//! Name normalization and creator identity live in [`crate::model_id`].

use crate::model_id::{
    benchmark_model_key, canonical_creator, creators_compatible, infer_creator, token_jaccard,
};
use crate::types::{Benchmark, Quote};

/// Canonical-key pairs that are DIFFERENT BUILDS and must never join — not
/// exactly, and not through the 0.80 fuzzy bar in [`best_benchmark_match`]
/// (or the live overlay's matching loop in
/// [`crate::fetch::aa_index::overlay_live_rows`]).
///
/// The shape that forces this table: two family names whose canonical keys
/// differ by one non-numeric word token sit exactly at the fuzzy bar
/// (`qwen 3 8 flash` vs `qwen 3 8 flash next` shares 4 of 5 tokens = 0.80),
/// and the count-aware matching hardening only defends version-number
/// collapses (`Sonnet 5` vs `Sonnet 5.5`). Without the guard, the undated
/// Surplus SKUs silently inherit the Aug-26 preview's live board evidence —
/// AA-Briefcase rates `qwen3-8-flash-next` (Elo ~1575) while AA's own
/// Intelligence Index leaves it at null, so the composite manufactures a
/// ~74th-percentile standing for a family the index deliberately does not
/// measure, and the undated listings plot at the preview's rank (#15 of 162
/// as found 2026-11-02). Per the 2026-10-23 row removal ("a different build
/// is not [the same model]"), the undated SKUs stay unrated — the GLM-4.5
/// precedent — until AA rates them under their own name.
///
/// The 2026-11-02 audit sweep (the live join audit's explainability rule now
/// rejects surplus word tokens, where the old version-sequence rule blessed
/// anything reading `[..., 3, 8, ...]` on both sides) surfaced the same
/// disease across the catalogue: derivative fine-tunes riding their base
/// checkpoint's rows, family variants (VL/Coder) riding the base model's
/// rows, and tier/experimental siblings (Flash Lite ↔ Flash, Vision Exp)
/// merging. Uncensored/heretic listings are handled generally in the matcher
/// (a flagged quote never fuzzy-joins — its only honest benchmark is an
/// exact row of its own, the curated `Abliterated Large V2` pattern); the
/// rest are curated here pair by pair. The live join audit fails on any new
/// word-token collision, so each future one must be either curated here or
/// proven same-weights in [`crate::artificial_analysis_snapshot::SAME_WEIGHTS_SKU_ALIASES`].
pub(crate) const DISTINCT_BUILD_NON_JOINS: &[(&str, &str)] = &[
    ("Qwen3.8 Flash", "Qwen3.8-Flash-Next"),
    ("Qwen3.8 Omni Flash", "Qwen3.8-Flash-Next"),
    // Family variants are different models, not spellings of the base.
    ("Qwen3 Coder 30B A3B Instruct", "Qwen3 30B A3B"),
    ("Qwen3 VL 235B A22B", "Qwen3 235B A22B"),
    ("Qwen3 VL 30B A3B", "Qwen3 30B A3B"),
    // Tier and experimental siblings rate separately.
    ("Gemini 3.5 Flash Lite", "Gemini 3.5 Flash"),
    ("DeepSeek V4 Flash Vision Exp", "DeepSeek V4 Flash Vision"),
    // A legal-domain finetune of FlashX is not FlashX.
    ("GLM 5.3 FlashX Legal", "GLM-5.3-FlashX"),
];

pub(crate) fn best_benchmark_match<'a>(
    quote: &Quote,
    benchmarks: &'a [Benchmark],
) -> Option<&'a Benchmark> {
    let q_model = benchmark_model_key(&quote.model);
    let q_name = benchmark_model_key(&quote.display_name);
    let inferred = infer_creator(&quote.model);
    let q_creator = canonical_creator(if quote.creator.is_empty() {
        &inferred
    } else {
        &quote.creator
    });
    // Distinct-build pairs (both orientations) are checked once per join.
    let non_joins = distinct_build_key_pairs();

    // An exact canonical model key is stronger evidence than provider metadata.
    // Several public leaderboards omit/mislabel the creator while still publishing
    // an unambiguous model slug, so creator gating here caused false non-matches.
    if let Some(exact) = benchmarks.iter().find(|b| {
        let slug = benchmark_model_key(&b.slug);
        let name = benchmark_model_key(&b.name);
        slug == q_model || name == q_name || slug == q_name || name == q_model
    }) {
        return Some(exact);
    }

    // An uncensored listing is a modified-weights derivative of whatever it
    // names (abliteration, heretic finetunes, …), so its only honest
    // benchmark is an exact row of its own — the exact branch above, the
    // curated `Abliterated Large V2` ~proxy pattern. Fuzzy inheritance
    // handed base-checkpoint scores to finetunes (GLM-5.3 Flash Uncensored
    // riding GLM-5.3-Flash on seven boards, found 2026-11-02).
    if quote.uncensored {
        return None;
    }

    let mut best: Option<(&Benchmark, f64)> = None;
    for b in benchmarks {
        let b_creator = canonical_creator(&b.creator);
        if !creators_compatible(&q_creator, &b_creator) {
            continue;
        }
        let b_key_slug = benchmark_model_key(&b.slug);
        let b_key_name = benchmark_model_key(&b.name);
        // A curated distinct-build pair never joins, however similar the
        // names read: the fuzzy bar below is exactly at 0.80 for the shape
        // this table exists for (the undated Qwen3.8 Flash SKUs vs the
        // Flash-Next preview).
        if is_distinct_build_join(&q_model, &q_name, &b_key_slug, &b_key_name, &non_joins) {
            continue;
        }
        let mut score =
            token_jaccard(&q_name, &b_key_name).max(token_jaccard(&q_model, &b_key_slug));
        // Dated releases keep their release tag in the canonical key (see
        // `model_core`), which leaves an undated family name one token short of
        // the fuzzy bar. When the only difference is such a tag, treat the pair
        // as a strong fallback match instead.
        if crate::model_id::keys_differ_only_by_release_tag(&q_name, &b_key_name)
            || crate::model_id::keys_differ_only_by_release_tag(&q_model, &b_key_slug)
        {
            score = score.max(0.95);
        }
        if score >= 0.80 && best.map(|(_, current)| score > current).unwrap_or(true) {
            best = Some((b, score));
        }
    }
    best.map(|(b, _)| b)
}

/// Canonical-key pairs (both orientations flattened) whose families are
/// different builds — [`DISTINCT_BUILD_NON_JOINS`]. A quote on one side must
/// never inherit the other side's benchmark rows, on any board.
pub(crate) fn distinct_build_key_pairs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (a, b) in DISTINCT_BUILD_NON_JOINS {
        let key_a = benchmark_model_key(a);
        let key_b = benchmark_model_key(b);
        if !key_a.is_empty() && !key_b.is_empty() && key_a != key_b {
            out.push((key_a.clone(), key_b.clone()));
            out.push((key_b, key_a));
        }
    }
    out
}

/// True when the quote keys and the row keys pair up as distinct builds: any
/// of the four key combinations the fuzzy matcher could ride on landing in
/// the non-join table blocks the row. (`distinct_build_key_pairs` flattens
/// both orientations, so one direction is checked here.)
fn is_distinct_build_join(
    q_model: &str,
    q_name: &str,
    b_slug: &str,
    b_name: &str,
    non_joins: &[(String, String)],
) -> bool {
    non_joins
        .iter()
        .any(|(x, y)| (x == q_model || x == q_name) && (y == b_slug || y == b_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;
    use crate::types::{BenchmarkKind, score_benchmark};

    #[test]
    fn version_doubled_names_never_join_their_base_version() {
        // `Sonnet 5.5` keys to `sonnet 5 5`; set-deduped token maths collapsed
        // the doubled `5` and scored the two different models a perfect 1.0
        // against each other — so the Sonnet 5 listing displayed Sonnet 5.5's
        // benchmark scores on every board.
        let q = test_quote("claude-sonnet-5", 1.0, true);
        let row = score_benchmark(
            "Claude Sonnet 5.5",
            1199.0,
            None,
            None,
            BenchmarkKind::Model,
        );
        assert!(best_benchmark_match(&q, &[row]).is_none());

        let q = test_quote("claude-opus-5", 1.0, true);
        let row = score_benchmark("Claude Opus 5.5", 58.0, None, None, BenchmarkKind::Model);
        assert!(best_benchmark_match(&q, &[row]).is_none());
    }

    #[test]
    fn distinct_build_skus_never_inherit_a_differently_built_rows_score() {
        // The undated Qwen3.8 Flash SKU shares 4 of 5 canonical tokens with
        // the Aug-26 Flash-Next preview (`qwen 3 8 flash` vs
        // `qwen 3 8 flash next`) — exactly the 0.80 fuzzy bar. They are
        // different builds (AA deleted the undated page; only the preview is
        // rated), so the join must be refused on every side.
        let flash_next = score_benchmark(
            "Qwen3.8-Flash-Next",
            1575.0,
            None,
            None,
            BenchmarkKind::Model,
        );
        for surplus_id in ["qwen3.8-flash", "qwen3.8-omni-flash"] {
            let q = test_quote(surplus_id, 1.0, true);
            assert!(
                best_benchmark_match(&q, std::slice::from_ref(&flash_next)).is_none(),
                "{surplus_id} must not inherit the Flash-Next preview's rows"
            );
        }
        // Symmetric: a Flash-Next quote must not ride an undated-family row.
        let q = test_quote("qwen3-8-flash-next", 1.0, true);
        let undated = score_benchmark("Qwen3.8 Flash", 20.0, None, None, BenchmarkKind::Model);
        assert!(best_benchmark_match(&q, &[undated]).is_none());

        // The guard is narrow: everything it does not name still joins.
        // A dated refresh of the undated family keeps its fallback join…
        let q = test_quote("qwen3.8-flash", 1.0, true);
        let dated = score_benchmark("Qwen3.8 Flash 1019", 29.0, None, None, BenchmarkKind::Model);
        assert!(best_benchmark_match(&q, &[dated]).is_some());
        // …and so does the Flash-Next preview against its own family rows.
        let q = test_quote("qwen3-8-flash-next", 1.0, true);
        assert!(best_benchmark_match(&q, &[flash_next]).is_some());
    }

    #[test]
    fn uncensored_listings_never_fuzzy_join_a_base_checkpoints_score() {
        // An uncensored/abliterated listing is a modified-weights derivative
        // of whatever it names. Its only honest benchmark is an exact row of
        // its own; fuzzy inheritance handed base scores to finetunes
        // (GLM-5.3 Flash Uncensored riding GLM-5.3-Flash on seven boards).
        let mut q = test_quote("glm-5.3-flash-uncensored", 1.0, true);
        q.uncensored = true;
        let base = score_benchmark("GLM-5.3-Flash", 41.9, None, None, BenchmarkKind::Model);
        assert!(best_benchmark_match(&q, &[base]).is_none());
        // An exact row of the derivative itself still joins (the curated
        // ~proxy pattern, `Abliterated Large V2`).
        let own = score_benchmark(
            "GLM-5.3-Flash-Uncensored",
            41.9,
            None,
            None,
            BenchmarkKind::Model,
        );
        assert!(best_benchmark_match(&q, &[own]).is_some());
    }

    #[test]
    fn family_variant_listings_do_not_inherit_the_base_models_rows() {
        // Tier and family siblings the audit surfaced riding base rows at
        // exactly the 0.80 bar: Vision-language and Coder variants are
        // different models, and Flash-Lite is a different tier from Flash.
        for (variant, base) in [
            ("Qwen3 VL 235B A22B", "Qwen3 235B A22B"),
            ("Qwen3 Coder 30B A3B Instruct", "Qwen3 30B A3B"),
            ("Gemini 3.5 Flash Lite", "Gemini 3.5 Flash"),
        ] {
            let q = test_quote(variant, 1.0, true);
            let row = score_benchmark(base, 50.0, None, None, BenchmarkKind::Model);
            assert!(
                best_benchmark_match(&q, &[row]).is_none(),
                "{variant} must not inherit {base}'s rows"
            );
        }
    }

    #[test]
    fn livebench_effort_suffix_matches_base_model() {
        let q = test_quote("gpt-5.5", 1.0, true);
        let b = vec![Benchmark {
            slug: "gpt-5.5-xhigh".into(),
            name: "gpt-5.5-xhigh".into(),
            creator: "OpenAI".into(),
            overall: Some(90.0),
            coding: Some(82.0),
            agentic_coding: Some(70.0),
            agent: None,
            reasoning_effort: None,
            kind: BenchmarkKind::Model,
            tokens_per_task: None,
            token_profile: None,
        }];
        assert!(best_benchmark_match(&q, &b).is_some());
    }

    #[test]
    fn benchmark_aliases_match_real_cross_source_names() {
        for (surplus, benchmark) in [
            ("claude-opus-5", "Opus 5"),
            ("claude-fable-5", "Fable 5"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("glm-5.2", "GLM-5.2"),
            ("deepseek-v4-flash", "deepseek-v4-flash-0731"),
            ("claude-opus-4.6", "Claude-4.6-Opus"),
        ] {
            let q = test_quote(surplus, 1.0, true);
            let b = vec![score_benchmark(
                benchmark,
                50.0,
                None,
                None,
                BenchmarkKind::Model,
            )];
            assert!(
                best_benchmark_match(&q, &b).is_some(),
                "{surplus} should match {benchmark}"
            );
        }
    }
}
