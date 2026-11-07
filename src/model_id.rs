//! Model identity: canonical model keys, creator canonicalization and
//! inference, and the token-similarity math the cross-source joins build on.
//! A leaf module — everything that names, normalizes, or groups a model
//! routes through here, and it depends on nothing but std.

/// One canonical display label per lab. The catalog spells the same lab
/// inconsistently across model rows (`xAI` vs `SpaceXAI`, `Z.ai` vs
/// `Zhipu AI`, `Alibaba` vs `Qwen` vs `Alibaba Cloud`), which used to split a
/// single family into several chart colours and legend groups. Everything
/// that groups, labels, or colours a creator routes through here. Unknown
/// labs pass through unchanged so each stays its own group.
pub(crate) fn canonical_group(creator: &str) -> &str {
    match normalize(creator).as_str() {
        "xai" | "x ai" | "spacexai" => "xAI",
        "z ai" | "zai" | "zhipu" | "zhipu ai" => "Z.AI",
        "alibaba" | "alibaba cloud" | "qwen" => "Alibaba",
        "openai" | "open ai" => "OpenAI",
        "google" | "deepmind" | "google deepmind" => "Google",
        "moonshot" | "moonshot ai" | "kimi" => "Moonshot",
        "deepseek" | "deepseek ai" => "DeepSeek",
        "minimax" | "minimax ai" => "MiniMax",
        "mistral" | "mistral ai" => "Mistral AI",
        "arcee" | "arcee ai" => "Arcee AI",
        "meta" | "meta ai" => "Meta",
        "venice" | "venice ai" | "api venice ai" => "Venice AI",
        "anthropic" => "Anthropic",
        "nvidia" => "NVIDIA",
        "xiaomi" | "mimo" | "mimo ai" => "Xiaomi",
        "tencent" => "Tencent",
        "microsoft" => "Microsoft",
        "cohere" => "Cohere",
        "ai21" => "AI21",
        _ => creator,
    }
}

pub(crate) fn infer_creator(model: &str) -> String {
    let n = normalize(model);
    if n.starts_with("claude ")
        || n.starts_with("opus ")
        || n.starts_with("sonnet ")
        || n.starts_with("haiku ")
        || n.starts_with("fable ")
    {
        "Anthropic".into()
    } else if n.starts_with("gpt ")
        || n.starts_with("o1 ")
        || n.starts_with("o3 ")
        || n.starts_with("o4 ")
    {
        "OpenAI".into()
    } else if n.starts_with("gemini ") {
        "Google".into()
    } else if n.starts_with("grok ") {
        "xAI".into()
    } else if n.starts_with("deepseek ") {
        "DeepSeek".into()
    } else if n.starts_with("qwen") {
        "Alibaba".into()
    } else if n.starts_with("kimi ") {
        "Moonshot".into()
    } else if n.starts_with("glm ") {
        "Z.AI".into()
    } else if n.starts_with("minimax ") {
        "MiniMax".into()
    } else if n.starts_with("mimo ") || n.starts_with("xiaomi ") {
        "Xiaomi".into()
    } else if n.starts_with("nemotron ") || n.starts_with("nvidia ") {
        "NVIDIA".into()
    } else if n.starts_with("mistral ") {
        "Mistral AI".into()
    } else if n.starts_with("arcee ") || n.starts_with("trinity ") {
        "Arcee AI".into()
    } else {
        String::new()
    }
}

/// The chart legend label for a creator: the canonical group, or "Unknown"
/// for the empty creator that unattributed rows carry.
pub(crate) fn group_label(creator: &str) -> &str {
    if creator.is_empty() {
        "Unknown"
    } else {
        canonical_group(creator)
    }
}

pub(crate) fn creators_compatible(a: &str, b: &str) -> bool {
    a.is_empty() || b.is_empty() || a == b || a.contains(b) || b.contains(a)
}

pub(crate) fn canonical_creator(s: &str) -> String {
    normalize(canonical_group(s))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_family_ordering_matches_across_sources() {
        assert_eq!(benchmark_model_key("Claude Opus 4.6"), "opus 4 6");
        assert_eq!(benchmark_model_key("Claude-4.6-Opus"), "opus 4 6");
        assert_eq!(benchmark_model_key("Opus 4.6"), "opus 4 6");
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

    #[test]
    fn jaccard_and_release_tag_math_is_count_aware() {
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
    fn catalog_creator_spelling_variants_share_one_group() {
        // Real strings observed in the live Surplus catalog: the same lab is
        // spelled differently across model rows.
        for variants in [
            ["xAI", "SpaceXAI", "X.AI"],
            ["Z.ai", "Zhipu AI", "Zhipu"],
            ["Alibaba", "Qwen", "Alibaba Cloud"],
            ["Moonshot", "Moonshot AI", "Kimi"],
            ["OpenAI", "Open AI", "openai"],
            ["Venice AI", "Venice", "api.venice.ai"],
            ["Xiaomi", "MiMo", "MiMo AI"],
        ] {
            let canonical = canonical_group(variants[0]);
            for variant in variants {
                assert_eq!(canonical_group(variant), canonical, "{variant}");
                assert_eq!(group_label(variant), canonical, "{variant}");
            }
        }
        // Unknown labs stay their own group (no cross-lab bleed).
        assert_eq!(canonical_group("Kuaishou"), "Kuaishou");
        assert_ne!(canonical_group("Kuaishou"), canonical_group("ByteDance"));
        assert_eq!(canonical_group(""), "");
        assert_eq!(group_label(""), "Unknown");
    }

    #[test]
    fn mimo_models_infer_as_xiaomi() {
        // Surplus prices rows carry bare IDs like `xiaomi-mimo-v2-5` with no
        // provider field, and some boards label the creator `MiMo`: both must
        // land in the Xiaomi group.
        assert_eq!(infer_creator("xiaomi-mimo-v2-5"), "Xiaomi");
        assert_eq!(infer_creator("MiMo-V2.5"), "Xiaomi");
        assert_eq!(infer_creator("MiMo-V2.5-Pro"), "Xiaomi");
        assert_eq!(canonical_group("MiMo"), "Xiaomi");
    }
}
