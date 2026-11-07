//! Artificial Analysis board naming conventions: effort tiers, board-short
//! name aliases, display-name splitting, and leaderboard `shortName`
//! classification. Shared by every AA board fetcher (aa_evals, aa_index,
//! aa_speed), the SWE-Bench Pro effort parsing, and the bundled snapshot's
//! name-disclosure pass — one table so the snapshot rows, the live overlay,
//! and the board parsers cannot drift apart.

use crate::model_id::benchmark_model_key;

/// Effort tiers AA names in its variant suffixes. Anything else inside the
/// parentheses (`Default Fallback`, run notes) is configuration, not effort.
/// Order is load-bearing: [`variant_preference`] indexes it low→max and
/// [`crate::fetch::aa_speed`] inverts it for TTFT preference.
pub(crate) const NAMED_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The AA boards rate some models under short display names that normalize
/// differently from the Surplus listing. Map them to the listing name so
/// exact-key joins hit first.
pub(crate) fn alias_display_name(base: &str) -> &str {
    match base {
        // Surplus lists "Thinking Machines: Inkling"; the creator prefix is
        // not one benchmark matching strips.
        "Inkling" => "Thinking Machines: Inkling",
        // The v5.0 (2026-10-28) boards drop parameter counts and creator
        // prefixes from these names; the snapshot rows (and Surplus display
        // names) carry the full forms, so alias them rather than rely on the
        // 0.80 fuzzy bar (or miss the join entirely — these fall well under
        // it).
        "Nemotron 3 Ultra" => "NVIDIA Nemotron 3 Ultra 550B A55B",
        "Muse Glimmer" => "Muse Glimmer 30B",
        "Beam 1.0" => "Reflection: Beam 1.0 501B A23B",
        "Ling 3.1 Flash 1019" => "inclusionAI: Ling 3.1 Flash 1019",
        // Scale versions Muse Wonder as "1.0"; Surplus lists it unversioned
        // (SWE-Bench Pro's leaderboard renders this spelling).
        "Muse Wonder 1.0" => "Muse Wonder",
        other => other,
    }
}

/// Split an AA display name into its base model and effort tier:
/// `Claude Sonnet 5.5 (Max, Default Fallback)` → (`Claude Sonnet 5.5`, max),
/// `Claude Opus 5.5 (max with fallback)` → (`Claude Opus 5.5`, max). A
/// parenthetical that does not LEAD with an effort tier is part of the model
/// name (`K2 Horizon (375B A23B)`) and stays.
pub(crate) fn split_aa_display_name(raw: &str) -> (String, Option<String>) {
    let raw = raw.trim();
    let Some(open) = raw.rfind(" (") else {
        return (raw.to_owned(), None);
    };
    let base = raw[..open].trim();
    if base.is_empty() {
        return (raw.to_owned(), None);
    }
    let inner = raw[open + 2..].trim_end_matches(')');
    let Some(first) = inner.split([',', ' ']).find(|token| !token.is_empty()) else {
        return (raw.to_owned(), None);
    };
    match NAMED_EFFORTS.iter().find(|e| e.eq_ignore_ascii_case(first)) {
        Some(effort) => (base.to_owned(), Some((*effort).to_owned())),
        None => (raw.to_owned(), None),
    }
}

/// Split a leaderboard `shortName` into its family identity: canonical key,
/// base display name (post-alias), reasoning-effort variant, and the
/// non-reasoning marker. Shared by the Intelligence Index reads and the
/// standardized-speed reads so both fold variants identically.
pub(crate) fn classify_aa_name(short: &str) -> Option<(String, String, Option<String>, bool)> {
    let trimmed = short.trim();
    let mut non_reasoning = false;
    let mut probe = trimmed;
    if let Some(open) = trimmed.rfind(" (")
        && trimmed.ends_with(')')
        && trimmed[open + 2..]
            .trim_end_matches(')')
            .to_ascii_lowercase()
            .starts_with("non-reasoning")
    {
        non_reasoning = true;
        probe = trimmed[..open].trim();
    }
    let (base, effort) = split_aa_display_name(probe);
    if base.is_empty() {
        return None;
    }
    let base = alias_display_name(&base).to_owned();
    Some((benchmark_model_key(&base), base, effort, non_reasoning))
}

/// Variant preference order shared by every AA read: bare > named efforts
/// (max first) > explicit non-reasoning.
pub(crate) fn variant_preference(effort: Option<&str>, non_reasoning: bool) -> usize {
    if non_reasoning {
        return 0;
    }
    let rank = |e: &str| NAMED_EFFORTS.iter().position(|t| *t == e).unwrap_or(0) + 1;
    1 + effort.map_or(0, rank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_split_only_on_leading_effort_tiers() {
        assert_eq!(
            split_aa_display_name("Claude Sonnet 5.5 (Max, Default Fallback)"),
            ("Claude Sonnet 5.5".into(), Some("max".into()))
        );
        assert_eq!(
            split_aa_display_name("Gemini 4 Argon (high)"),
            ("Gemini 4 Argon".into(), Some("high".into()))
        );
        assert_eq!(
            split_aa_display_name("Claude Opus 5.5 (max with fallback)"),
            ("Claude Opus 5.5".into(), Some("max".into()))
        );
        assert_eq!(
            split_aa_display_name("MiMo-V2.6-Pro"),
            ("MiMo-V2.6-Pro".into(), None)
        );
        assert_eq!(
            split_aa_display_name("K2 Horizon (375B A23B)"),
            ("K2 Horizon (375B A23B)".into(), None)
        );
    }
}
