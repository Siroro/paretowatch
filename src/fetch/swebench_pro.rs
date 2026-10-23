//! SWE-Bench Pro V2 (HARD) — Scale's long-horizon software-engineering
//! benchmark: 1,865 tasks across 41 professional repositories, third-party
//! run with a locked protocol, and unsaturated where SWE-bench Verified
//! topped out. The public split uses strong-copyleft repos (contamination
//! resistant by design) and every result is re-graded on a pristine image.
//!
//! The leaderboard page server-renders its rows (the default HARD tab) with
//! `data-model-name` markers, so a plain HTML fetch needs no API endpoint:
//! `Opus 5 (Claude Code) xhigh` + `98.00`. The parenthetical is the agent
//! harness the model ran in — the row scores the pairing, so rows carry the
//! harness in `agent` and the board joins the harness-specific family.

use anyhow::{Result, anyhow};
use regex::Regex;
use reqwest::blocking::Client;

use super::fetch_text;
use crate::types::{Benchmark, BenchmarkKind};

const SWEBENCH_PRO_URL: &str = "https://labs.scale.com/leaderboard/swe_bench_pro_public_v2";

/// Effort tiers the leaderboard appends after the harness.
const NAMED_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Display names the leaderboard renders differently from the Surplus
/// listing, mapped to the listing name so exact-key joins hit first.
fn alias_display_name(base: &str) -> &str {
    match base {
        // Scale versions Muse Wonder as "1.0"; Surplus lists it unversioned.
        "Muse Wonder 1.0" => "Muse Wonder",
        // Surplus lists "Thinking Machines: Inkling"; the creator prefix is
        // not one benchmark matching strips.
        "Inkling" => "Thinking Machines: Inkling",
        other => other,
    }
}

pub(crate) fn fetch_swebench_pro(client: &Client) -> Result<Vec<Benchmark>> {
    let html = fetch_text(client, SWEBENCH_PRO_URL, "SWE-Bench Pro V2 leaderboard")?;
    let rows = parse_swebench_pro(&html);
    if rows.is_empty() {
        return Err(anyhow!("SWE-Bench Pro V2 page had no scored models"));
    }
    Ok(rows)
}

/// Split a leaderboard row name into `(base model, agent harness, effort)`:
/// `Opus 5 (Claude Code) xhigh` → (`Opus 5`, `Claude Code`, `xhigh`).
pub(crate) fn split_scale_name(raw: &str) -> (String, Option<String>, Option<String>) {
    let mut name = raw.trim().to_owned();
    let mut effort = None;
    if let Some((head, last)) = name.rsplit_once(' ')
        && NAMED_EFFORTS.iter().any(|e| e.eq_ignore_ascii_case(last))
    {
        effort = Some(last.to_lowercase());
        name = head.trim().to_owned();
    }
    let mut agent = None;
    if let Some(open) = name.rfind(" (")
        && name.ends_with(')')
    {
        let base = name[..open].trim().to_owned();
        if !base.is_empty() {
            agent = Some(name[open + 2..name.len() - 1].to_owned());
            name = base;
        }
    }
    (name, agent, effort)
}

pub(crate) fn parse_swebench_pro(html: &str) -> Vec<Benchmark> {
    // Each row server-renders as
    // `<p data-model-name="true" ...>Opus 5 (Claude Code) xhigh</p>` with the
    // resolve-rate span a short, non-greedy hop later in the same row block.
    let row_re = Regex::new(
        r#"(?s)data-model-name="true"[^>]*>([^<]+)</p>.{0,600}?<span class="text-ink">([0-9]+\.?[0-9]*)</span>"#,
    )
    .expect("valid row regex");
    let mut rows = Vec::new();
    for caps in row_re.captures_iter(html) {
        let (base, agent, effort) = split_scale_name(caps.extract::<2>().1[0]);
        let base = alias_display_name(&base).to_owned();
        let Ok(score) = caps.extract::<2>().1[1].parse::<f64>() else {
            continue;
        };
        if base.is_empty() || !score.is_finite() {
            continue;
        }
        let kind = if agent.is_some() {
            BenchmarkKind::ModelAgent
        } else {
            BenchmarkKind::Model
        };
        let mut row = super::score_benchmark(base.clone(), score, agent.clone(), effort, kind);
        row.name = format!("{base} [SWE-Bench Pro V2 HARD resolve rate]");
        if let Some(agent) = &agent {
            row.name = format!("{base} ({agent}) [SWE-Bench Pro V2 HARD resolve rate]");
        }
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::best_benchmark_match;
    use crate::testfix::test_quote;

    fn page(rows: &[(&str, &str)]) -> String {
        let body = rows
            .iter()
            .map(|(name, score)| {
                format!(
                    r#"<div><p data-model-name="true" class="text-xs">{name}</p>\
                    <p><span class="text-ink">{score}</span></p></div>"#,
                )
            })
            .collect::<String>();
        format!("<html><body>{body}</body></html>")
    }

    #[test]
    fn splits_harness_and_effort_from_row_names() {
        assert_eq!(
            split_scale_name("Opus 5 (Claude Code) xhigh"),
            (
                "Opus 5".to_owned(),
                Some("Claude Code".to_owned()),
                Some("xhigh".to_owned())
            )
        );
        // Trailing whitespace seen in the wild must not pin the effort token.
        assert_eq!(
            split_scale_name("GPT-6 Astra (Codex) high "),
            (
                "GPT-6 Astra".to_owned(),
                Some("Codex".to_owned()),
                Some("high".to_owned())
            )
        );
        assert_eq!(
            split_scale_name("Kimi-K3 (mini-swe-agent) max"),
            (
                "Kimi-K3".to_owned(),
                Some("mini-swe-agent".to_owned()),
                Some("max".to_owned())
            )
        );
        assert_eq!(
            split_scale_name("GLM-5.3 (mini-swe-agent)"),
            (
                "GLM-5.3".to_owned(),
                Some("mini-swe-agent".to_owned()),
                None
            )
        );
    }

    #[test]
    fn parses_server_rendered_rows() {
        let html = &page(&[
            ("Opus 5 (Claude Code) xhigh", "98.00"),
            ("Gemini 4 Argon (mini-swe-agent) high", "92.16"),
            ("Inkling (mini-swe-agent) xhigh", "56.90"),
            ("Muse Wonder 1.0 (mini-swe-agent) xhigh", "90.20"),
        ]);
        let rows = parse_swebench_pro(html);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].slug, "Opus 5");
        assert_eq!(rows[0].agentic_coding, Some(98.0));
        assert_eq!(rows[0].agent.as_deref(), Some("Claude Code"));
        assert_eq!(rows[0].reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(rows[0].kind, BenchmarkKind::ModelAgent);
        // Aliased names join their Surplus listings.
        assert_eq!(rows[2].slug, "Muse Wonder");
        assert!(rows.iter().any(|r| r.slug == "Thinking Machines: Inkling"));
    }

    #[test]
    fn rows_join_surplus_quotes() {
        let html = &page(&[("Sonnet 5.5 (mini-swe-agent) xhigh", "70.59")]);
        let rows = parse_swebench_pro(html);
        let q = test_quote("claude-sonnet-5.5", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }
}
