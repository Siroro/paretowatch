//! Live AA Intelligence Index, read from the public leaderboard page —
//! no API key. The page's RSC payload embeds every CURRENT model with its
//! `intelligenceIndex` (and an `intelligenceIndexIsEstimated` flag), so one
//! request refreshes the whole frontier. Legacy and hidden models are not in
//! that payload (their scores live only on per-model pages, ~4.5 MB each),
//! so this is an OVERLAY on the bundled snapshot rather than a replacement:
//!
//! - live non-estimated values replace the bundled score for the family
//!   (exact canonical key first, then the 0.80 fuzzy bar) and the row name
//!   re-tags from "snapshot <date>" to "live <date>";
//! - same-weights SKU aliases (`GPT-6.1 Sol Pro`, ...) follow their base
//!   variant's live value, keeping their inheritance disclosure;
//! - families the payload adds (new models AA now rates) get fresh rows;
//! - everything else — legacy families, ~proxy rows, AA estimates — keeps
//!   the bundled value, so a fetch failure degrades to today's behavior:
//!   the plain bundled snapshot.
//!
//! Variant policy mirrors the snapshot's: prefer the best reasoning effort
//! the page lists (max > xhigh > high > medium > low), fall back to the bare
//! entry, and only then to an explicit non-reasoning run.

use anyhow::Result;
use regex::Regex;
use reqwest::blocking::Client;

use super::aa_evals::{NAMED_EFFORTS, alias_display_name, split_aa_display_name};
use super::fetch_text;
use crate::artificial_analysis_snapshot::{SAME_WEIGHTS_SKU_ALIASES, artificial_analysis_snapshot};
use crate::bench::matching::{benchmark_model_key, token_jaccard};
use crate::types::{Benchmark, BenchmarkKind};

const AA_LEADERBOARD_URL: &str = "https://artificialanalysis.ai/leaderboards/models";

/// One live leaderboard entry, after name splitting.
#[derive(Debug, Clone)]
pub(crate) struct LiveEntry {
    /// Canonical key of the base model name.
    key: String,
    /// Display name for the family (Surplus-facing, post-alias).
    name: String,
    score: f64,
    /// Reasoning effort of the variant this entry read, if any.
    effort: Option<String>,
    /// The entry is an explicit non-reasoning run (lowest preference).
    non_reasoning: bool,
}

impl LiveEntry {
    fn preference(&self) -> usize {
        variant_preference(self.effort.as_deref(), self.non_reasoning)
    }
}

pub(crate) fn fetch_artificial_analysis_index(client: &Client) -> Result<Vec<Benchmark>> {
    let (index, _speed) = fetch_aa_index_and_speed(client);
    index
}

/// One leaderboard download, two products: the Intelligence Index overlay
/// (never fails — the bundled snapshot is the offline fallback) and the
/// standardized-speed table (fails outright when the payload is unusable,
/// so the UI can say so instead of silently showing nothing).
pub(crate) fn fetch_aa_index_and_speed(
    client: &Client,
) -> (
    Result<Vec<Benchmark>>,
    Result<crate::types::AaSpeedSnapshot, String>,
) {
    let bundled = artificial_analysis_snapshot();
    let html = match fetch_text(
        client,
        AA_LEADERBOARD_URL,
        "AA Intelligence Index leaderboard",
    ) {
        Ok(html) => html,
        Err(err) => return (Ok(bundled), Err(format!("{err:#}"))),
    };
    let version = parse_index_version(&html);
    let entries = parse_live_entries(&html);
    if entries.is_empty() {
        return (Ok(bundled), Err("payload carried no model entries".into()));
    }
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let rows = overlay_live_rows(bundled, &entries, version.as_deref(), &date);
    let speed = super::aa_speed::parse_aa_speed_snapshot(&html);
    (Ok(rows), speed)
}

/// The payload names the board ("Artificial Analysis Intelligence Index
/// v4.3.2") in its registry block; the live read discloses whatever
/// revision it actually measured.
fn parse_index_version(html: &str) -> Option<String> {
    Regex::new(r"Artificial Analysis Intelligence Index (v[0-9][0-9.]*)")
        .ok()?
        .captures(html)
        .map(|c| c.extract::<1>().1[0].to_owned())
}

/// Extract one live family entry per canonical key: attributed by nearest
/// preceding `shortName`, AA estimates skipped, variants folded by the
/// snapshot's preference order.
pub(crate) fn parse_live_entries(html: &str) -> Vec<LiveEntry> {
    let name_re = Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex");
    let score_re = Regex::new(
        r#"\\?"intelligenceIndex\\?":(null|[0-9]+\.?[0-9]*),\\?"intelligenceIndexIsEstimated\\?":(true|false)"#,
    )
    .expect("score regex");
    let names: Vec<(usize, String)> = name_re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).expect("whole match").start(),
                c.extract::<1>().1[0].to_owned(),
            )
        })
        .collect();

    let mut best: std::collections::HashMap<String, LiveEntry> = std::collections::HashMap::new();
    for caps in score_re.captures_iter(html) {
        let raw = caps.extract::<2>().1;
        if raw[0] == "null" || raw[1] == "true" {
            continue;
        }
        let Ok(score) = raw[0].parse::<f64>() else {
            continue;
        };
        if !score.is_finite() || !(0.0..=100.0).contains(&score) {
            continue;
        }
        let whole = caps.get(0).expect("whole match");
        let Some((_, short)) = names.iter().rev().find(|(pos, _)| *pos < whole.start()) else {
            continue;
        };
        let short = short.replace("\\u0026", "&");
        let Some(entry) = classify_short_name(&short, score) else {
            continue;
        };
        let current = best.get(&entry.key);
        if current.is_none_or(|old| entry.preference() > old.preference()) {
            best.insert(entry.key.clone(), entry);
        }
    }
    let mut out: Vec<LiveEntry> = best.into_values().collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
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

/// Split a leaderboard `shortName` into a family entry, folding the
/// non-reasoning marker into the variant class.
fn classify_short_name(short: &str, score: f64) -> Option<LiveEntry> {
    let (key, name, effort, non_reasoning) = classify_aa_name(short)?;
    Some(LiveEntry {
        key,
        name,
        score,
        effort,
        non_reasoning,
    })
}

/// Merge live entries onto the bundled snapshot rows (see the module docs
/// for the overlay rules).
pub(crate) fn overlay_live_rows(
    bundled: Vec<Benchmark>,
    live: &[LiveEntry],
    version: Option<&str>,
    date: &str,
) -> Vec<Benchmark> {
    let version = version
        .unwrap_or(crate::artificial_analysis_snapshot::ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION);
    let mut rows = bundled;
    let keys: Vec<String> = rows.iter().map(|r| benchmark_model_key(&r.slug)).collect();

    // Base key -> live score applied to that row, so SKU aliases can follow.
    let mut applied: Vec<(String, f64)> = Vec::new();
    for entry in live {
        // Exact canonical key first, then the 0.80 fuzzy bar (the leaderboard
        // drops parameter suffixes AA's own snapshot carries, like
        // "Nemotron 3.5 Lightning" vs "... 30B"); earliest row wins ties.
        let mut target: Option<usize> = None;
        let mut best_score = 0.0_f64;
        for (idx, key) in keys.iter().enumerate() {
            if key.is_empty() {
                continue;
            }
            let similarity = if *key == entry.key {
                1.0
            } else {
                token_jaccard(key, &entry.key)
            };
            if similarity >= 0.80 && similarity > best_score {
                best_score = similarity;
                target = Some(idx);
            }
        }
        match target {
            Some(idx) => {
                let slug = rows[idx].slug.clone();
                rows[idx].overall = Some(entry.score);
                rows[idx].coding = Some(entry.score);
                rows[idx].agentic_coding = Some(entry.score);
                rows[idx].name = format!("{slug} [AA Intelligence {version} · live {date}]");
                applied.push((benchmark_model_key(&slug), entry.score));
            }
            None => {
                let mut row = super::score_benchmark(
                    entry.name.clone(),
                    entry.score,
                    None,
                    None,
                    BenchmarkKind::Model,
                );
                row.name = format!("{} [AA Intelligence {version} · live {date}]", entry.name);
                rows.push(row);
            }
        }
    }

    // Same-weights SKU aliases inherit their base variant's live value so the
    // family keeps rating identically everywhere.
    for row in &mut rows {
        let key = benchmark_model_key(&row.slug);
        let Some((_, base, what)) = SAME_WEIGHTS_SKU_ALIASES
            .iter()
            .find(|(sku, _, _)| benchmark_model_key(sku) == key)
        else {
            continue;
        };
        let base_key = benchmark_model_key(base);
        if let Some((_, score)) = applied.iter().find(|(k, _)| *k == base_key) {
            row.overall = Some(*score);
            row.coding = Some(*score);
            row.agentic_coding = Some(*score);
            row.name = format!(
                "{} [AA Intelligence {version} · live {date} · inherited from {} — {}; same weights, not separately benchmarked]",
                row.slug, base, what
            );
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::benchmark_model_key;

    fn entry(name: &str, score: f64) -> LiveEntry {
        let (base, effort) = split_aa_display_name(name);
        let base = alias_display_name(&base).to_owned();
        LiveEntry {
            key: benchmark_model_key(&base),
            name: base,
            score,
            effort,
            non_reasoning: false,
        }
    }

    #[test]
    fn parses_leaderboard_payload_with_estimates_and_nulls() {
        let html = concat!(
            r#"{"slug":"claude-opus-5-5","shortName":"Claude Opus 5.5 (max with fallback)","intelligenceIndex":57.62,"intelligenceIndexIsEstimated":false},"#,
            r#"{"slug":"claude-opus-5-5-xhigh","shortName":"Claude Opus 5.5 (xhigh with fallback)","intelligenceIndex":55.99,"intelligenceIndexIsEstimated":false},"#,
            r#"{"slug":"jt-35b-flash","shortName":"JT-35B-Flash","intelligenceIndex":18.66,"intelligenceIndexIsEstimated":true},"#,
            r#"{"slug":"unrated","shortName":"Not Rated Model","intelligenceIndex":null,"intelligenceIndexIsEstimated":false},"#,
            r#"{"slug":"m2","shortName":"MiniMax-M2 (non-reasoning)","intelligenceIndex":18.4,"intelligenceIndexIsEstimated":false}"#,
        );
        let entries = parse_live_entries(html);
        // The estimate is skipped, the null skipped, and the two Opus effort
        // variants fold to the max-effort read; the non-reasoning entry
        // still lands (no reasoning twin in this fixture).
        assert_eq!(entries.len(), 2);
        let opus = entries
            .iter()
            .find(|e| e.name == "Claude Opus 5.5")
            .unwrap();
        assert!((opus.score - 57.62).abs() < 1e-9);
        assert!(!entries.iter().any(|e| e.name == "JT-35B-Flash"));
        assert!(
            entries
                .iter()
                .any(|e| e.non_reasoning && e.name == "MiniMax-M2")
        );
    }

    #[test]
    fn overlay_updates_matched_rows_and_adds_new_ones() {
        let bundled = artificial_analysis_snapshot();
        let live = vec![
            entry("Claude Opus 5.5 (max with fallback)", 57.6),
            entry("GPT-6.1 Sol (max)", 53.9),
            entry("Step 5 Preview", 43.7),
        ];
        let rows = overlay_live_rows(bundled, &live, Some("v4.3.2"), "2026-10-23");
        let by_slug = |s: &str| {
            rows.iter()
                .find(|r| r.slug == s)
                .unwrap_or_else(|| panic!("{s} missing"))
        };
        // Exact-key match: value moves, name re-tags to live.
        let opus = by_slug("Claude Opus 5.5");
        assert!((opus.agentic_coding.unwrap() - 57.6).abs() < 1e-9);
        assert!(opus.name.contains("live 2026-10-23"));
        assert!(!opus.name.contains("snapshot"));
        // The Sol Pro alias follows its base's live value.
        let sol_pro = by_slug("GPT-6.1 Sol Pro");
        assert!((sol_pro.agentic_coding.unwrap() - 53.9).abs() < 1e-9);
        assert!(sol_pro.name.contains("inherited from GPT-6.1 Sol"));
        assert!(sol_pro.name.contains("live 2026-10-23"));
        // A family with no live row keeps its bundled snapshot read — and a
        // family the current index has not re-measured stays unrated rather
        // than exposing a previous index's scale (GPT-5.2 is one of the
        // snapshot's `None` rows since v5.0).
        let legacy = by_slug("GPT-5.2");
        assert!(legacy.name.contains("snapshot"));
        assert_eq!(legacy.agentic_coding, None);
        // New current models join with a live tag.
        let step = rows
            .iter()
            .find(|r| benchmark_model_key(&r.slug) == benchmark_model_key("Step 5 Preview"))
            .expect("new row added");
        assert!((step.agentic_coding.unwrap() - 43.7).abs() < 1e-9);
        assert!(step.name.contains("live 2026-10-23"));
    }

    #[test]
    fn overlay_fuzzy_matches_size_token_differences() {
        // The leaderboard names the family without the parameter suffix AA's
        // own snapshot uses; the 0.80 fuzzy bar must bridge it so the
        // bundled (Surplus-facing) row gets the fresh value instead of a
        // duplicate being added.
        let bundled = artificial_analysis_snapshot();
        let live = vec![entry("Nemotron 3.5 Lightning", 12.8)];
        let rows = overlay_live_rows(bundled, &live, Some("v4.3.2"), "2026-10-23");
        let matches = rows
            .iter()
            .filter(|r| benchmark_model_key(&r.slug).contains("lightning"))
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "fuzzy overlay must update the bundled row, not add a second"
        );
        assert!(matches[0].slug.contains("30B"));
        assert!((matches[0].agentic_coding.unwrap() - 12.8).abs() < 1e-9);
    }

    #[test]
    fn version_anchor_extracts_from_payload_registry() {
        let html = r#"\"name\":\"Artificial Analysis Intelligence Index v4.3.2\",\"keywords\":[]}"#;
        assert_eq!(parse_index_version(html).as_deref(), Some("v4.3.2"));
    }
}
