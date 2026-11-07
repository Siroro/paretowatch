//! Cross-source model matching: canonical model keys, creator
//! canonicalization, and the fuzzy join between Surplus quotes and
//! benchmark rows.

use crate::artificial_analysis_snapshot::DISTINCT_BUILD_NON_JOINS;
use crate::theme::{canonical_group, infer_creator};
use crate::types::{Benchmark, Quote};

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
        if keys_differ_only_by_release_tag(&q_name, &b_key_name)
            || keys_differ_only_by_release_tag(&q_model, &b_key_slug)
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

/// True when one canonical key equals the other plus only MMDD-style release
/// tags (`0731`, `2507`, ...). Bare version digits are deliberately excluded so
/// `GPT-5` never fuzzy-joins `GPT-5.2` — and the comparison is COUNT-aware:
/// set semantics collapse `Sonnet 5.5` (`sonnet 5 5`) onto `Sonnet 5`
/// because the doubled `5` vanishes, which made two different models score
/// a perfect match against each other.
pub(crate) fn keys_differ_only_by_release_tag(a: &str, b: &str) -> bool {
    use std::collections::HashMap;
    fn counts(s: &str) -> HashMap<&str, usize> {
        let mut map = HashMap::new();
        for token in s.split_whitespace() {
            *map.entry(token).or_insert(0) += 1;
        }
        map
    }
    let a_counts = counts(a);
    let b_counts = counts(b);
    let (short, long) = if a_counts.len() <= b_counts.len() {
        (&a_counts, &b_counts)
    } else {
        (&b_counts, &a_counts)
    };
    // Every token the short side names must appear at least as often on the
    // long side, and every surplus occurrence must be a release tag.
    short
        .iter()
        .all(|(token, n)| long.get(token).is_some_and(|m| *m >= *n))
        && long.iter().all(|(token, n)| {
            short.get(token).copied().unwrap_or(0) >= *n
                || (token.len() >= 3 && token.chars().all(|c| c.is_ascii_digit()))
        })
}

pub(crate) fn creators_compatible(a: &str, b: &str) -> bool {
    a.is_empty() || b.is_empty() || a == b || a.contains(b) || b.contains(a)
}

pub(crate) fn canonical_creator(s: &str) -> String {
    normalize(canonical_group(s))
}

pub(crate) fn token_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashMap;
    // COUNT-aware (multiset) Jaccard: `sonnet 5` and `sonnet 5 5` (Sonnet 5
    // vs Sonnet 5.5) must score 2/3, not 1/0 — set-deduped tokens made the
    // doubled version number invisible and joined two different models.
    fn counts(s: &str) -> HashMap<&str, usize> {
        let mut map = HashMap::new();
        for token in s.split_whitespace() {
            *map.entry(token).or_insert(0) += 1;
        }
        map
    }
    let at = counts(a);
    let bt = counts(b);
    if at.is_empty() || bt.is_empty() {
        return 0.0;
    }
    let mut inter = 0usize;
    let mut union = 0usize;
    for (token, n) in &at {
        union += n;
        if let Some(m) = bt.get(token) {
            inter += (*n).min(*m);
        }
    }
    for (token, m) in &bt {
        if at.get(token).is_none_or(|n| *n < *m) {
            union += m - at.get(token).copied().unwrap_or(0);
        }
    }
    inter as f64 / union as f64
}

pub(crate) fn model_core(s: &str) -> String {
    let tokens = normalize(s)
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    // A year token opens a two-part date (`2024-11-20`): the all-digit tokens
    // right after it are its month and day. A KEPT non-digit token disarms
    // the counter — without that, an unrelated digit later in the name
    // ("X 2025 chat 5") is eaten as a date part that never existed.
    let mut skip_date_parts = 0usize;
    for token in tokens {
        if skip_date_parts > 0 && token.chars().all(|c| c.is_ascii_digit()) {
            skip_date_parts -= 1;
            continue;
        }
        if is_year_token(&token) {
            skip_date_parts = 2;
            continue;
        }
        if token.len() == 8 && token.starts_with("20") && token.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        // Keep four-digit MMDD release suffixes (for example DeepSeek `0731`).
        // Current benchmark snapshots can score dated releases differently from
        // the base family; fuzzy matching still lets an undated family score act
        // as a fallback when a specific release has no row.
        if is_model_modifier(&token) {
            continue;
        }
        skip_date_parts = 0;
        out.push(token);
    }
    out.join(" ")
}

pub(crate) fn benchmark_model_key(s: &str) -> String {
    let core = model_core(s);
    let tokens = core
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    // Some sources write Qwen generations as `Qwen3.5`, while Surplus display
    // names often use `Qwen 3.5`. Normalization turns those into `qwen3 5` and
    // `qwen 3 5`; split the attached generation number so they join exactly.
    let mut expanded = Vec::with_capacity(tokens.len() + 1);
    for token in tokens {
        if let Some(suffix) = token.strip_prefix("qwen")
            && !suffix.is_empty()
            && suffix.chars().all(|c| c.is_ascii_digit())
        {
            expanded.push("qwen".to_owned());
            expanded.push(suffix.to_owned());
            continue;
        }
        expanded.push(token);
    }
    let mut tokens = expanded;
    if tokens.len() >= 3
        && matches!(
            (tokens[0].as_str(), tokens[1].as_str()),
            ("open", "ai") | ("z", "ai")
        )
    {
        tokens.drain(0..2);
    } else if tokens.len() >= 2
        && matches!(
            tokens[0].as_str(),
            "openai"
                | "anthropic"
                | "google"
                | "xai"
                | "alibaba"
                | "moonshot"
                | "meta"
                | "zhipu"
                | "xiaomi"
                | "nvidia"
        )
    {
        tokens.remove(0);
    }

    // Deployment wrappers exposed by Surplus do not change the underlying model
    // capability score. Let E2EE/web/private variants inherit the same benchmark.
    while tokens
        .first()
        .is_some_and(|token| matches!(token.as_str(), "e2ee" | "web" | "private"))
    {
        tokens.remove(0);
    }

    // Anthropic names are annoyingly inconsistent across leaderboards:
    // `Claude Opus 4.6`, `Claude-4.6-Opus`, and `Opus 4.6` should all join.
    if tokens.first().is_some_and(|token| token == "claude") {
        tokens.remove(0);
        if let Some(pos) = tokens
            .iter()
            .position(|token| matches!(token.as_str(), "opus" | "sonnet" | "haiku" | "fable"))
            && pos != 0
        {
            let family = tokens.remove(pos);
            tokens.insert(0, family);
        }
    }

    // Common leaderboard/catalog aliases. Keep version numbers intact; only
    // remove packaging words that do not identify a distinct model.
    tokens.retain(|token| !matches!(token.as_str(), "model" | "api"));
    tokens.join(" ")
}

pub(crate) fn is_year_token(token: &str) -> bool {
    token.len() == 4
        && token.chars().all(|c| c.is_ascii_digit())
        && token
            .parse::<u32>()
            .map(|year| (2024..=2035).contains(&year))
            .unwrap_or(false)
}

pub(crate) fn is_model_modifier(token: &str) -> bool {
    matches!(
        token,
        "auto"
            | "low"
            | "medium"
            | "high"
            | "xhigh"
            | "effort"
            | "max"
            | "preview"
            | "latest"
            | "basic"
            | "minimal"
            | "standard"
            | "web"
            | "e2ee"
            | "private"
            | "reasoning"
            | "nonreasoning"
            | "fast"
            | "highspeed"
            | "ultraspeed"
            | "prime"
            | "batch"
            | "instruct"
            | "it"
    ) || (token.ends_with('k')
        && token[..token.len().saturating_sub(1)]
            .chars()
            .all(|c| c.is_ascii_digit()))
}

pub(crate) fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::score_benchmark;
    use crate::testfix::test_quote;
    use crate::types::BenchmarkKind;

    #[test]
    fn claude_family_ordering_matches_across_sources() {
        assert_eq!(benchmark_model_key("Claude Opus 4.6"), "opus 4 6");
        assert_eq!(benchmark_model_key("Claude-4.6-Opus"), "opus 4 6");
        assert_eq!(benchmark_model_key("Opus 4.6"), "opus 4 6");
    }

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

        assert!(token_jaccard("sonnet 5", "sonnet 5 5") < 0.80);
        assert!(!keys_differ_only_by_release_tag("sonnet 5", "sonnet 5 5"));
        assert!(!keys_differ_only_by_release_tag("gpt 5", "gpt 5 2"));
        // Dated-release fallbacks must keep joining.
        assert!(keys_differ_only_by_release_tag(
            "deepseek v4 flash",
            "deepseek v4 flash 0731"
        ));
        assert_eq!(token_jaccard("sonnet 5 5", "sonnet 5 5"), 1.0);
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
    fn deployment_wrappers_inherit_base_model_benchmark_key() {
        assert_eq!(
            benchmark_model_key("e2ee-glm-5"),
            benchmark_model_key("GLM-5")
        );
        assert_eq!(
            benchmark_model_key("deepseek-v4-flash:web"),
            benchmark_model_key("DeepSeek V4 Flash")
        );
        // Xiaomi's UltraSpeed serving edition is the same MiMo-V2.6-Pro
        // checkpoint, so it inherits the base row instead of reading as an
        // unrated model.
        assert_eq!(
            benchmark_model_key("mimo-v2.6-pro-ultraspeed"),
            benchmark_model_key("MiMo-V2.6-Pro")
        );
        // Z.ai's GLM 5.3 Prime is the same GLM-5.3 checkpoint served at higher
        // speed ("inheriting its full capabilities" per the listing), so it
        // inherits the base row the same way.
        assert_eq!(
            benchmark_model_key("glm-5.3-prime"),
            benchmark_model_key("GLM-5.3")
        );
        // Anthropic's Batch API SKUs (for example Claude Haiku 5.5 (batch),
        // listed at half the interactive price) are execution modes of the
        // same weights, so the batch token strips like the speed wrappers.
        assert_eq!(
            benchmark_model_key("Claude Haiku 5.5 (batch)"),
            benchmark_model_key("Claude Haiku 5.5")
        );
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

    #[test]
    fn canonicalization_keeps_scored_releases_and_unifies_qwen_spelling() {
        assert_ne!(
            benchmark_model_key("DeepSeek V4 Flash 0731"),
            benchmark_model_key("DeepSeek V4 Flash")
        );
        assert_eq!(
            benchmark_model_key("Qwen3.5 35B A3B"),
            benchmark_model_key("Qwen 3.5 35B A3B Instruct")
        );
        assert_eq!(
            benchmark_model_key("Qwen3 Coder 480B Turbo"),
            benchmark_model_key("Qwen 3 Coder 480B Turbo")
        );
    }

    #[test]
    fn date_skip_disarms_when_a_kept_word_token_intervenes() {
        // A year token opens a two-part date, but a kept word after it must
        // disarm the counter: "X 2025 chat 5" lost its "5" to the still-armed
        // skip and keyed as "x chat".
        assert_eq!(model_core("X 2025 chat 5"), "x chat 5");
        assert_eq!(model_core("Model 2026 lite 7"), "model lite 7");
        // The guarded real cases keep behaving: a year token's own date parts
        // are still eaten, and MMDD-style release tags are not year tokens.
        assert_eq!(model_core("GPT-4 2024-11-20"), "gpt 4");
        assert_eq!(
            model_core("DeepSeek V4 Flash 0731"),
            "deepseek v4 flash 0731"
        );
        assert_eq!(model_core("Qwen3 235B A22B 2507"), "qwen3 235b a22b 2507");
    }
}
