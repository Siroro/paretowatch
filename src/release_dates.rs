//! Bundled model release dates.
//!
//! The live Surplus catalog carries an `added_at` timestamp (the marketplace
//! listing date), but no true release date — and the AA leaderboard payload
//! publishes none either. Historical dates therefore live here, the same way
//! the pre-v5 AA reads do: a curated, disclosed table that the UI prefers
//! over the marketplace listing date, with the listing date serving as the
//! fallback for every model this table does not name.
//!
//! # Entry policy
//!
//! - Exact documented dates first: launches and listings stated in the
//!   [`crate::artificial_analysis_snapshot`] notes (e.g. "Claude Sonnet 5.5 —
//!   released 2026-09-28").
//! - Dated refresh builds use their build tag's date (`1017` → 2026-10-17);
//!   the tag is part of the model's identity, so these never inherit the
//!   undated family row's date.
//! - Legacy (pre-2026) public models use their well-known public release
//!   dates; ±1 day of precision is fine and noted where it applies.
//! - Anything not known to within a few days is LEFT OUT: the marketplace
//!   `added_at` listing date is the honest fallback, and a wrong date here
//!   would be indistinguishable from a real one in the UI.
//! - Same-weights SKUs in [`crate::artificial_analysis_snapshot::SAME_WEIGHTS_SKU_ALIASES`]
//!   inherit their base variant's date through [`release_date`] and need no
//!   row of their own.

use crate::artificial_analysis_snapshot::SAME_WEIGHTS_SKU_ALIASES;
use crate::bench::matching::benchmark_model_key;

/// `(model name, release date)` pairs, grouped by provenance and roughly
/// newest-first within each group (lookups join on the normalized key, so
/// the order is not load-bearing). Names follow the snapshot's display-name
/// conventions so the same normalized key joins both the snapshot rows and
/// the Surplus catalog listings.
pub(crate) const RELEASE_DATES: &[(&str, &str)] = &[
    // Documented launches and listings (snapshot notes).
    ("Mistral Large 4", "2026-10-29"), // GA build; the deprecated Preview read (2026-10-06) is not the release
    ("Gemini 4 Carbon", "2026-10-28"), // launched hours after the 2026-10-28 morning rebaseline
    ("GPT-6.1 Terra", "2026-10-28"),   // entered with the v5.0 rebaseline bump
    ("Kimi K3.1 Pro", "2026-10-24"), // launch bump; OpenRouter-first, ahead of its Surplus listing
    ("DeepSeek V4.1 Flash Uncensored", "2026-10-21"), // Surplus listed 2026-10-21
    ("Muse Wonder", "2026-10-14"),   // listed 2026-10-14
    ("Abliterated Large V2", "2026-10-13"), // listed 2026-10-13
    ("Claude Haiku 5.5", "2026-10-07"), // listed 2026-10-07
    ("Claude Sonnet 5.5", "2026-09-28"), // released 2026-09-28 (snapshot notes)
    ("Claude Opus 5.5", "2026-09-22"), // entered as leader with the 2026-09-22 bump
    ("Gemini 4 Argon", "2026-09-30"), // launch read 2026-09-30; general Surplus listing 2026-10-14
    ("GPT-6.1 Sol", "2026-09-30"),   // added for the 2026-09-29..30 listings
    // Dated refresh builds: the build tag is the release date.
    ("DeepSeek V4.1 Flash 1017", "2026-10-17"),
    ("inclusionAI: Ling 3.1 Flash 1019", "2026-10-19"),
    ("DeepSeek V4.1 Flash", "2026-09-10"), // the 0910 build the undated row reads
    ("DeepSeek V4 Pro 0813", "2026-08-13"),
    ("DeepSeek V4 Flash 0731", "2026-07-31"),
    ("Qwen3.8 Max (0902)", "2026-09-02"),
    // Long-running Surplus listings with a known start.
    ("Thinking Machines: Inkling", "2026-07-24"), // listed since 2026-07-24 (auto-proposed SKU)
    // Legacy public models: well-known release dates (public announcements).
    ("Grok 4.1 Fast", "2025-11-17"),
    ("Kimi K2 Thinking", "2025-11-06"),
    ("DeepSeek V3.2", "2025-09-29"), // V3.2-Exp public release
    ("GLM-4.6", "2025-09-30"),       // public release (±1 day)
    ("DeepSeek V3.1 Terminus", "2025-09-22"),
    ("Qwen3 Next 80B A3B Instruct", "2025-09-11"),
    ("Kimi K2", "2025-07-11"),
    ("Qwen3 Coder 480B", "2025-07-23"), // public release (±1 day)
    ("GLM-4.5-Air", "2025-07-28"),      // GLM-4.5 family launch
    ("Qwen3 235B A22B 2507", "2025-07-24"), // 2507 build
    ("Qwen3 235B A22B Thinking 2507", "2025-07-24"),
    ("Magistral Small 2509", "2025-09-24"), // 2509 build (±few days)
    ("Qwen3 Coder 30B A3B Instruct", "2025-08-13"), // public release (±1 day)
    ("GPT-5 Nano", "2025-08-07"),
    ("gpt-oss-120b", "2025-08-05"),
    ("gpt-oss-20b", "2025-08-05"),
    ("Grok Code Fast 1", "2025-08-26"),
    ("DeepSeek V3.1", "2025-08-19"),
    ("Gemini 2.5 Flash", "2025-06-17"), // GA; the 2025-03/04 previews predate it
    ("Mistral Small 3.2", "2025-06-20"),
    ("Qwen3 32B", "2025-04-29"), // Qwen3 launch (2025-04-28/29)
    ("Qwen3 30B A3B", "2025-04-29"),
    ("Llama 4 Scout", "2025-04-05"),
    ("Gemini 2.5 Pro", "2025-03-25"), // preview launch; GA 2025-06-17
    ("Gemma 3 27B", "2025-03-12"),    // Gemma 3 launch
    ("Gemma 3 12B", "2025-03-12"),
    ("Gemma 3 4B", "2025-03-12"),
    ("DeepSeek R1 0528", "2025-05-28"), // dated refresh
    ("DeepSeek R1", "2025-01-20"),
    ("Llama 3.3 70B", "2024-12-06"),
    ("GPT-4o Mini", "2024-07-18"),
    ("GPT-4o", "2024-05-13"),
];

/// Alias walk depth cap. The alias table is a single hop today and its bases
/// name real rows, but that is data discipline, not a structural guarantee —
/// the cap turns a future two-step alias cycle into `None` instead of a
/// stack overflow.
const MAX_ALIAS_HOPS: u32 = 8;

/// Release timestamp for a model display name or slug, in unix seconds.
/// Exact normalized-key hits first, then the same-weights SKU aliases (a SKU
/// inherits its base variant's date; the walk is depth-capped so a future
/// alias cycle bottoms out as `None`). `None` = unknown: the UI falls back
/// to the marketplace listing date.
pub(crate) fn release_date(model: &str) -> Option<i64> {
    release_date_at(model, MAX_ALIAS_HOPS)
}

fn release_date_at(model: &str, hops: u32) -> Option<i64> {
    let key = benchmark_model_key(model);
    if key.is_empty() {
        return None;
    }
    if let Some((_, date)) = RELEASE_DATES
        .iter()
        .find(|(name, _)| benchmark_model_key(name) == key)
    {
        return parse_date(date);
    }
    if hops == 0 {
        return None;
    }
    SAME_WEIGHTS_SKU_ALIASES
        .iter()
        .find(|(sku, _, _)| benchmark_model_key(sku) == key)
        .and_then(|(_, base, _)| release_date_at(base, hops - 1))
}

fn parse_date(date: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artificial_analysis_snapshot::{ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE, SNAPSHOT_ROWS};

    // "Future" relative to the snapshot's own date, the newest provenance the
    // app carries: every documented date must predate it (a row that
    // postdates the snapshot means the snapshot itself needs bumping first).
    const DATE_CUTOFF: &str = ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE;

    #[test]
    fn keys_are_unique_and_parseable() {
        let mut seen = std::collections::HashSet::new();
        for (model, date) in RELEASE_DATES {
            let key = benchmark_model_key(model);
            assert!(!key.is_empty(), "{model}: normalizes to an empty key");
            assert!(
                seen.insert(key.clone()),
                "duplicate normalized key `{key}` ({model})"
            );
            assert!(
                chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
                "{model}: bad date {date}"
            );
        }
    }

    #[test]
    fn no_date_is_in_the_future() {
        for (model, date) in RELEASE_DATES {
            assert!(
                date.as_bytes() <= DATE_CUTOFF.as_bytes(),
                "{model}: {date} postdates the snapshot provenance ({DATE_CUTOFF})"
            );
        }
    }

    #[test]
    fn dated_refreshes_do_not_inherit_their_family_row() {
        // A dated build is a different checkpoint from the undated family
        // row; its release date must be its own entry, never inherited.
        for (refresh, family) in [
            ("DeepSeek V4.1 Flash 1017", "DeepSeek V4.1 Flash"),
            ("DeepSeek R1 0528", "DeepSeek R1"),
            ("Qwen3.8 Max (0902)", "Qwen3.8 Max"),
        ] {
            assert_ne!(
                benchmark_model_key(refresh),
                benchmark_model_key(family),
                "{refresh} must key separately from {family}"
            );
        }
        assert_eq!(
            release_date("DeepSeek V4.1 Flash 1017").unwrap(),
            parse_date("2026-10-17").unwrap()
        );
    }

    #[test]
    fn same_weights_skus_inherit_their_base_release() {
        for (sku, base, _) in SAME_WEIGHTS_SKU_ALIASES {
            // A same-weights SKU with its own dated row is a different BUILD
            // (the R1 0528 refresh) — scoring inherits the base, but its
            // release date is genuinely its own and the exact row wins.
            let has_own_row = RELEASE_DATES
                .iter()
                .any(|(name, _)| benchmark_model_key(name) == benchmark_model_key(sku));
            if has_own_row {
                continue;
            }
            let base_date = release_date(base);
            if base_date.is_some() {
                assert_eq!(
                    release_date(sku),
                    base_date,
                    "{sku} must inherit {base}'s release date"
                );
            }
        }
        assert_eq!(
            release_date("Mistral Small 3.2 24B Instruct"),
            release_date("Mistral Small 3.2"),
        );
        // The dated refresh keeps its own, later release date even though it
        // scores identically to the base page.
        assert!(release_date("DeepSeek R1 0528").unwrap() > release_date("DeepSeek R1").unwrap(),);
    }

    #[test]
    fn alias_walk_is_depth_capped() {
        // The cap exists so a future two-step alias cycle returns None
        // instead of overflowing the stack; with no hops left, even a
        // well-formed alias must refuse to walk.
        assert_eq!(release_date_at("Mistral Small 3.2 24B Instruct", 0), None);
        // With hops available the same SKU inherits its base's date as ever.
        assert_eq!(
            release_date_at("Mistral Small 3.2 24B Instruct", MAX_ALIAS_HOPS),
            release_date("Mistral Small 3.2"),
        );
    }

    #[test]
    fn lookup_joins_surplus_display_spellings() {
        // The catalog spells names with creator prefixes and differently
        // spaced generations; the lookup must survive both.
        assert_eq!(
            release_date("OpenAI GPT OSS 120B"),
            release_date("gpt-oss-120b"),
        );
        assert_eq!(
            release_date("openai-gpt-oss-120b"),
            release_date("gpt-oss-120b")
        );
        assert_eq!(
            release_date("Meta: Muse Spark 1.3 Contributor"),
            release_date("Muse Spark 1.3"),
        );
    }

    #[test]
    fn unknown_models_resolve_to_none() {
        assert_eq!(release_date("Totally Made Up Model"), None);
        assert_eq!(release_date(""), None);
    }

    #[test]
    fn every_named_snapshot_row_keeps_its_date() {
        // Rows this table names must keep resolving across snapshot bumps; a
        // renamed row that drops its entry silently falls back to the
        // listing date, so the test makes that visible instead.
        let named: std::collections::HashSet<String> = RELEASE_DATES
            .iter()
            .map(|(model, _)| benchmark_model_key(model))
            .collect();
        let snapshot: std::collections::HashSet<String> = SNAPSHOT_ROWS
            .iter()
            .map(|(model, _)| benchmark_model_key(model))
            .collect();
        assert!(
            named.is_subset(&snapshot),
            "release-date rows that name no snapshot row: {:?}",
            named.difference(&snapshot).collect::<Vec<_>>()
        );
    }
}
