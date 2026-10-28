//! Artificial Analysis's own agentic evaluation boards, read from their
//! public HTML pages (no API key — the same pages the site renders for
//! anonymous visitors):
//!
//! - **AA-Briefcase** — Elo ratings from blind pairwise comparisons on long,
//!   multi-week professional-work tasks. The task set is private, so the
//!   Elo is contamination-resistant by construction, and it is one of the
//!   flagship components of the Intelligence Index this app already trusts
//!   as its bundled prior.
//! - **AutomationBench-AA** — long-horizon office automation (finance, HR,
//!   ...). The headline number is the *partial score*: how much of each
//!   task's objective tree the model completed with no violations.
//! - **AA-3D** (added 0.31.0) — Elo ratings for interactive 3D experience
//!   building: each model works as an agent in AA's offline Stirrup sandbox,
//!   turning a written brief into a single three.js web page, and blind
//!   pairwise votes from AA's recruited panel produce a Bradley-Terry Elo
//!   (GPT-6 Luna anchored at 1000).
//!
//! The Briefcase page renders a plain server-side table. The
//! AutomationBench page embeds its per-model data as escaped JSON inside
//! the RSC payload; each model object carries a `shortName` (display name
//! with effort suffix) well before its `automationBenchPartialScore`, so
//! scores are attributed by nearest-preceding-name position — the same
//! value the site headlines (Gemini 4 Argon's 77.5 partial score matches
//! the number AA's changelog quotes for it). AA-3D uses that same payload
//! style: its board objects end with `\"elo\":<integer>` right after the
//! model's shortName and creator logo. The integer-only, object-end form
//! matters — the page also embeds decimal `elo` fields for OTHER boards
//! (AA-Briefcase breakdowns, per-domain win rates), which the pattern
//! must not capture.

use anyhow::{Result, anyhow};
use regex::Regex;
use reqwest::blocking::Client;

use super::{collapse_whitespace, fetch_text, score_benchmark, strip_html_tags};
use crate::bench::matching::benchmark_model_key;
use crate::types::{Benchmark, BenchmarkKind};

const AA_BRIEFCASE_URL: &str = "https://artificialanalysis.ai/evaluations/aa-briefcase";
const AUTOMATIONBENCH_URL: &str = "https://artificialanalysis.ai/evaluations/automationbench-aa";
const AA_3D_URL: &str = "https://artificialanalysis.ai/evaluations/aa-3d";

/// Effort tiers AA names in its variant suffixes. Anything else inside the
/// parentheses (`Default Fallback`, run notes) is configuration, not effort.
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

fn build_row(raw_name: &str, score: f64, url_note: &str) -> Option<Benchmark> {
    let (base, effort) = split_aa_display_name(raw_name);
    let base = alias_display_name(&base).to_owned();
    if base.is_empty() || !score.is_finite() {
        return None;
    }
    let mut row = score_benchmark(base.clone(), score, None, effort, BenchmarkKind::Model);
    row.name = format!("{base} [AA {url_note}]");
    Some(row)
}

/// Keep one row per canonical key — AA lists several effort variants of the
/// same model, and the purchasable SKU should read the best variant.
fn dedupe_keep_best(rows: Vec<Benchmark>) -> Vec<Benchmark> {
    let mut best: std::collections::HashMap<String, Benchmark> = std::collections::HashMap::new();
    for row in rows {
        let key = benchmark_model_key(&row.slug);
        if key.is_empty() {
            continue;
        }
        let replace = match best.get(&key).and_then(|old| old.agentic_coding) {
            Some(old) => row.agentic_coding.is_some_and(|new| new > old),
            None => true,
        };
        if replace {
            best.insert(key, row);
        }
    }
    let mut out: Vec<Benchmark> = best.into_values().collect();
    out.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    out
}

pub(crate) fn fetch_aa_briefcase(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(client, AA_BRIEFCASE_URL, "AA-Briefcase leaderboard")?;
    let rows = parse_aa_briefcase(&html);
    if rows.is_empty() {
        return Err(anyhow!("AA-Briefcase page had no Elo rows"));
    }
    Ok(rows)
}

/// The page renders one server-side table: rank | org | model | Elo | CI |
/// date. Model names carry their effort suffix in parentheses.
pub(crate) fn parse_aa_briefcase(html: &str) -> Vec<Benchmark> {
    let tr = Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").expect("valid tr regex");
    let td = Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("valid td regex");
    let mut rows = Vec::new();
    for cells in tr
        .captures_iter(html)
        .map(|m| m.extract::<1>().1[0].to_string())
        .map(|row_html| {
            td.captures_iter(&row_html)
                .map(|c| collapse_whitespace(&strip_html_tags(c.extract::<1>().1[0])))
                .collect::<Vec<_>>()
        })
    {
        // Name is the third cell, Elo the fourth; anything shorter is the
        // header or a section row.
        let (Some(name), Some(elo)) = (cells.get(2), cells.get(3)) else {
            continue;
        };
        let Ok(elo) = elo.trim().parse::<f64>() else {
            continue;
        };
        if !(500.0..=3000.0).contains(&elo) {
            continue;
        }
        if let Some(row) = build_row(name, elo, "Briefcase Elo") {
            rows.push(row);
        }
    }
    dedupe_keep_best(rows)
}

pub(crate) fn fetch_automationbench_aa(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(
        client,
        AUTOMATIONBENCH_URL,
        "AutomationBench-AA leaderboard",
    )?;
    let rows = parse_automationbench_aa(&html);
    if rows.is_empty() {
        return Err(anyhow!("AutomationBench-AA page had no scored models"));
    }
    Ok(rows)
}

/// The scores live in the escaped-JSON RSC payload as
/// `\"shortName\":\"Model (effort)\"` ... `\"automationBenchPartialScore\":0.695`.
/// Attribute each score to the nearest preceding shortName and read the
/// partial score as a percentage.
pub(crate) fn parse_automationbench_aa(html: &str) -> Vec<Benchmark> {
    let name_re = Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex");
    let score_re = Regex::new(r#"\\?"automationBenchPartialScore\\?":([0-9]+\.?[0-9]*)"#)
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
    let mut rows = Vec::new();
    for caps in score_re.captures_iter(html) {
        let whole = caps.get(0).expect("whole match");
        let Ok(frac) = caps.extract::<1>().1[0].parse::<f64>() else {
            continue;
        };
        // Nearest preceding shortName owns this score.
        let Some((_, name)) = names.iter().rev().find(|(pos, _)| *pos < whole.start()) else {
            continue;
        };
        let name = name.replace("\\u0026", "&");
        if let Some(row) = build_row(
            &name,
            (frac * 100.0).clamp(0.0, 100.0),
            "AutomationBench partial score",
        ) {
            rows.push(row);
        }
    }
    dedupe_keep_best(rows)
}

pub(crate) fn fetch_aa_3d(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(client, AA_3D_URL, "AA-3D leaderboard")?;
    let rows = parse_aa_3d(&html);
    if rows.is_empty() {
        return Err(anyhow!("AA-3D page had no Elo rows"));
    }
    Ok(rows)
}

/// AA-3D Elo from the evaluation page's escaped-JSON RSC payload. The
/// board's model objects end with `\"elo\":<integer>}` right after the
/// model's shortName — the same integer the leaderboard table renders
/// (verified: 23 objects for 23 ranked models, GPT-6 Luna anchored at
/// 1000). The integer-only object-end form is the discriminator: the page
/// also embeds DECIMAL `elo` fields with `lower95ci` followers for other
/// boards (AA-Briefcase breakdowns, per-domain win rates), which must not
/// be captured — nearest-preceding-name attribution plus this shape pins
/// each Elo to the model object that carries it.
pub(crate) fn parse_aa_3d(html: &str) -> Vec<Benchmark> {
    let name_re = Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex");
    let elo_re = Regex::new(r#"\\?"elo\\?":([0-9]+)\}"#).expect("elo regex");
    let names: Vec<(usize, String)> = name_re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).expect("whole match").start(),
                c.extract::<1>().1[0].to_owned(),
            )
        })
        .collect();
    let mut rows = Vec::new();
    for caps in elo_re.captures_iter(html) {
        let whole = caps.get(0).expect("whole match");
        let Ok(elo) = caps.extract::<1>().1[0].parse::<f64>() else {
            continue;
        };
        if !(500.0..=3000.0).contains(&elo) {
            continue;
        }
        let Some((_, name)) = names.iter().rev().find(|(pos, _)| *pos < whole.start()) else {
            continue;
        };
        let name = name.replace("\\u0026", "&");
        if let Some(row) = build_row(&name, elo, "3D Elo") {
            rows.push(row);
        }
    }
    dedupe_keep_best(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::best_benchmark_match;
    use crate::testfix::test_quote;

    #[test]
    fn splits_effort_suffixes_from_display_names() {
        assert_eq!(
            split_aa_display_name("Claude Sonnet 5.5 (Max, Default Fallback)"),
            ("Claude Sonnet 5.5".to_owned(), Some("max".to_owned()))
        );
        assert_eq!(
            split_aa_display_name("Gemini 4 Argon (high)"),
            ("Gemini 4 Argon".to_owned(), Some("high".to_owned()))
        );
        // AutomationBench shortNames spell the fallback without a comma.
        assert_eq!(
            split_aa_display_name("Claude Opus 5.5 (max with fallback)"),
            ("Claude Opus 5.5".to_owned(), Some("max".to_owned()))
        );
        assert_eq!(
            split_aa_display_name("MiMo-V2.6-Pro"),
            ("MiMo-V2.6-Pro".to_owned(), None)
        );
        // A parenthetical that is not an effort tier stays part of the name.
        assert_eq!(
            split_aa_display_name("K2 Horizon (375B A23B)"),
            ("K2 Horizon (375B A23B)".to_owned(), None)
        );
    }

    #[test]
    fn parses_briefcase_elo_table() {
        let html = r#"
        <table><thead><tr><th>Rank</th><th>Org</th><th>Model</th><th>Elo</th><th>95% CI</th><th>Date</th></tr></thead>
        <tbody>
          <tr><td>1</td><td><div><span>Anthropic</span></div></td><td>Claude Sonnet 5.5 (Max, Default Fallback)</td><td>1816</td><td>-11 / +12</td><td>Sep 2026</td></tr>
          <tr><td>2</td><td><div><span>Anthropic</span></div></td><td>Claude Opus 5.5 (Max, Default Fallback)</td><td>1801</td><td>-9 / +10</td><td>Sep 2026</td></tr>
          <tr><td>3</td><td><div><span>xAI</span></div></td><td>Grok 4.6 (xhigh)</td><td>1762</td><td>-14 / +15</td><td>Sep 2026</td></tr>
        </tbody></table>
        "#;
        let rows = parse_aa_briefcase(html);
        assert_eq!(rows.len(), 3);
        // Sorted by score descending; effort tiers split out of the name.
        assert_eq!(rows[0].name, "Claude Sonnet 5.5 [AA Briefcase Elo]");
        assert_eq!(rows[0].agentic_coding, Some(1816.0));
        assert_eq!(rows[0].reasoning_effort.as_deref(), Some("max"));
        assert_eq!(rows[2].reasoning_effort.as_deref(), Some("xhigh"));
        // Elo rows join Surplus quotes by exact key.
        let q = test_quote("claude-sonnet-5.5", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }

    #[test]
    fn briefcase_dedupes_effort_variants_keeping_the_best() {
        let html = r#"
        <table><tbody>
          <tr><td>1</td><td></td><td>GPT-6 Astra (max)</td><td>1700</td><td></td><td></td></tr>
          <tr><td>9</td><td></td><td>GPT-6 Astra (xhigh)</td><td>1650</td><td></td><td></td></tr>
        </tbody></table>
        "#;
        let rows = parse_aa_briefcase(html);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agentic_coding, Some(1700.0));
    }

    #[test]
    fn parses_automationbench_escaped_payload_by_position() {
        // Mirrors the RSC payload: escaped JSON with shortName well before
        // each model's automationBenchPartialScore, plus unrelated
        // creator/name pairs that must NOT capture the attribution.
        let html = concat!(
            r#"{"creator":{"slug":"google","name":"Google"},"models":[{"slug":"gemini-4-argon","name":"Gemini 4 Argon (High)","shortName":"Gemini 4 Argon (high)","suffix":"High","#,
            r#""intelligenceIndex":53.02,"automationBenchPartialScore":0.77498,"automationBenchBreakdown":{"strictScore":0.41}},"#,
            r#"{"creator":{"slug":"anthropic","name":"Anthropic"},"slug":"claude-opus-5-5","shortName":"Claude Opus 5.5 (max with fallback)","#,
            r#","automationBenchPartialScore":0.69538}]}"#,
        );
        let rows = parse_automationbench_aa(html);
        assert_eq!(rows.len(), 2);
        // Partial score reads as a percentage; Argon's 77.5 matches the
        // number AA's changelog headlines for it.
        let argon = rows.iter().find(|r| r.slug == "Gemini 4 Argon").unwrap();
        assert!((argon.agentic_coding.unwrap() - 77.5).abs() < 0.05);
        assert_eq!(argon.reasoning_effort.as_deref(), Some("high"));
        let opus = rows.iter().find(|r| r.slug == "Claude Opus 5.5").unwrap();
        assert!((opus.agentic_coding.unwrap() - 69.5).abs() < 0.05);
        assert_eq!(opus.reasoning_effort.as_deref(), Some("max"));
    }

    #[test]
    fn automationbench_rows_join_surplus_quotes() {
        let html = concat!(
            r#"{"slug":"mimo-v2-6-pro","shortName":"MiMo-V2.6-Pro","automationBenchPartialScore":0.586},"#,
            r#"{"slug":"inkling","shortName":"Inkling (xhigh)","automationBenchPartialScore":0.05}"#,
        );
        let rows = parse_automationbench_aa(html);
        let mimo = test_quote("mimo-v2.6-pro", 1.0, true);
        assert!(best_benchmark_match(&mimo, &rows).is_some());
        // The aliased Inkling row joins the listing through the display name
        // the way Surplus publishes it (creator prefix + model).
        let inkling = crate::types::Quote {
            display_name: "Thinking Machines: Inkling".into(),
            creator: "Thinking Machines".into(),
            ..test_quote("inkling", 1.0, true)
        };
        let matched = best_benchmark_match(&inkling, &rows).expect("inkling joins");
        assert!(matched.slug.starts_with("Thinking Machines: Inkling"));
    }

    #[test]
    fn display_name_alias_applies_to_both_boards() {
        let html = r#"<table><tbody><tr><td>14</td><td></td><td>Inkling (xhigh)</td><td>1494</td><td></td><td></td></tr></tbody></table>"#;
        let rows = parse_aa_briefcase(html);
        assert_eq!(rows[0].slug, "Thinking Machines: Inkling");
    }

    #[test]
    fn parses_aa_3d_elo_and_ignores_other_boards_elo_fields() {
        // Mirrors the AA-3D page payload: board objects end with an integer
        // `elo` right after the model's shortName, while the page ALSO embeds
        // decimal `elo` fields for other boards (AA-Briefcase breakdowns, the
        // AA-3D chart's CI block). Only the integer board form may produce
        // rows.
        let html = concat!(
            r#"{"slug":"claude-sonnet-5-5","name":"Claude Sonnet 5.5 (Max, Default Fallback)","shortName":"Claude Sonnet 5.5 (max with fallback)","#,
            r#""briefcaseElo":1811.82,"briefcaseBreakdown":{"overall":{"elo":1811.82,"lower95ci":1800.1,"upper95ci":1823.5}},"#,
            r#"{"slug":"gpt-6-luna","shortName":"GPT-6 Luna (max)","creatorLogo":"/img/logos/openai.svg","elo":1000},"#,
            r#"{"slug":"nemotron-3-ultra-550b-a55b","shortName":"Nemotron 3 Ultra","creatorLogo":"/img/logos/nvidia.svg","elo":573},"#,
            r##"{"chartBlock":[{"shortName":"Claude Sonnet 5.5 (max with fallback)","creatorColor":"#cc785c","elo":1198.8306,"lower95ci":1166.3291,"upper95ci":1231.3321}]}"##,
            r#""creatorLogo":"/img/logos/anthropic.svg","elo":1199}"#,
        );
        let rows = parse_aa_3d(html);
        assert_eq!(rows.len(), 3, "decimal elo fields must not add rows");
        let sonnet = rows
            .iter()
            .find(|r| r.slug == "Claude Sonnet 5.5")
            .expect("effort suffix splits out of the name");
        assert_eq!(sonnet.agentic_coding, Some(1199.0));
        assert_eq!(sonnet.reasoning_effort.as_deref(), Some("max"));
        // GPT-6 Luna is the board's fixed 1000 anchor.
        let luna = rows.iter().find(|r| r.slug == "GPT-6 Luna").unwrap();
        assert_eq!(luna.agentic_coding, Some(1000.0));
        // The board's short family name aliases onto the Surplus listing
        // name, so the row joins Nemotron quotes by exact key.
        let nemotron = rows
            .iter()
            .find(|r| r.slug == "NVIDIA Nemotron 3 Ultra 550B A55B")
            .expect("short board name aliases onto the listing name");
        assert_eq!(nemotron.agentic_coding, Some(573.0));
        let quote = test_quote("nvidia-nemotron-3-ultra-550b-a55b", 1.0, true);
        assert!(best_benchmark_match(&quote, &rows).is_some());
        // Sorted best-first: the leader tops the board.
        assert_eq!(rows[0].slug, "Claude Sonnet 5.5");
    }
}
