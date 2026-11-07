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

use super::aa_names::{NAMED_EFFORTS, alias_display_name};
use super::http::fetch_text;
use crate::types::{Benchmark, BenchmarkKind, score_benchmark};

const SWEBENCH_PRO_URL: &str = "https://labs.scale.com/leaderboard/swe_bench_pro_public_v2";

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
    // resolve-rate span a short hop later in the same row block. Parsing is
    // row-local — the segment between one name marker and the next belongs
    // to that row — so a row whose score span is missing is dropped alone
    // instead of pairing the NEXT row's score and shifting every later
    // name/score pairing.
    let name_re =
        Regex::new(r#"data-model-name="true"[^>]*>([^<]+)</p>"#).expect("valid name regex");
    let score_re = Regex::new(r#"<span class="text-ink">([0-9]+\.?[0-9]*)</span>"#)
        .expect("valid score regex");
    let anchors: Vec<(usize, usize, String)> = name_re
        .captures_iter(html)
        .map(|caps| {
            let whole = caps.get(0).expect("whole match");
            (
                whole.start(),
                whole.end(),
                caps.extract::<1>().1[0].to_owned(),
            )
        })
        .collect();
    let mut rows = Vec::new();
    for (idx, (_, name_end, raw_name)) in anchors.iter().enumerate() {
        let segment_end = anchors
            .get(idx + 1)
            .map_or(html.len(), |(next_start, _, _)| *next_start);
        // The score span must still sit within a short hop of the name (the
        // old row regex's `.{0,600}?`), inside this row's own segment.
        let window = &html[*name_end..(*name_end + 600).min(segment_end)];
        let Some(score_caps) = score_re.captures(window) else {
            continue;
        };
        let (base, agent, effort) = split_scale_name(raw_name);
        let base = alias_display_name(&base).to_owned();
        let Ok(score) = score_caps.extract::<1>().1[0].parse::<f64>() else {
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
        let mut row = score_benchmark(base.clone(), score, agent.clone(), effort, kind);
        row.name = format!("{base} [SWE-Bench Pro V2 HARD resolve rate]");
        if let Some(agent) = &agent {
            row.name = format!("{base} ({agent}) [SWE-Bench Pro V2 HARD resolve rate]");
        }
        rows.push(row);
    }
    super::parse::sort_rows_by_score_desc(&mut rows);
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
    fn row_missing_score_is_dropped_without_cascading() {
        // The middle row publishes no resolve-rate span. Row-local parsing
        // must drop only that row; a whole-page name-then-next-score regex
        // would pair its name with the NEXT row's score and shift every
        // later pairing.
        let html = concat!(
            r#"<div><p data-model-name="true" class="text-xs">Opus 5 (Claude Code) xhigh</p><p><span class="text-ink">98.00</span></p></div>"#,
            r#"<div><p data-model-name="true" class="text-xs">Gemini 4 Argon (mini-swe-agent) high</p><p>no score published</p></div>"#,
            r#"<div><p data-model-name="true" class="text-xs">Inkling (mini-swe-agent) xhigh</p><p><span class="text-ink">56.90</span></p></div>"#,
        );
        let rows = parse_swebench_pro(html);
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .any(|r| r.slug == "Opus 5" && r.agentic_coding == Some(98.0))
        );
        assert!(
            rows.iter()
                .any(|r| r.slug == "Thinking Machines: Inkling" && r.agentic_coding == Some(56.9))
        );
        assert!(!rows.iter().any(|r| r.slug.contains("Gemini")));
    }

    #[test]
    fn rows_join_surplus_quotes() {
        let html = &page(&[("Sonnet 5.5 (mini-swe-agent) xhigh", "70.59")]);
        let rows = parse_swebench_pro(html);
        let q = test_quote("claude-sonnet-5.5", 1.0, true);
        assert!(best_benchmark_match(&q, &rows).is_some());
    }
}
