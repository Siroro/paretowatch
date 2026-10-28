//! Artificial Analysis standardized speed, read from the same leaderboard
//! payload as the Intelligence Index (see [`fetch_aa_index_and_speed`]):
//! per-model median tokens/second with percentiles, median time-to-first
//! token, and end-to-end response time, all measured under AA's own uniform
//! protocol. Unlike the live-market feed's speed fields — coarse histogram
//! buckets pooled across every seller — these are true full-precision
//! medians, so they lead the speed columns wherever a model matches.
//!
//! Parsing mirrors `parse_live_entries`: records are flat JSON objects in an
//! RSC-escaped string, so each field is matched independently and attributed
//! to its record by position (the nearest preceding `shortName` / anchor
//! field), then variants fold into families by the shared classifier — with
//! a speed-specific variant preference (see [`speed_variant_rank`]).

use chrono::Utc;
use regex::Regex;

use super::aa_evals::NAMED_EFFORTS;
use super::aa_index::classify_aa_name;
use crate::bench::matching::{
    benchmark_model_key, canonical_creator, creators_compatible, keys_differ_only_by_release_tag,
    token_jaccard,
};
use crate::theme::infer_creator;
use crate::types::{AaSpeedEntry, AaSpeedSnapshot, Quote};

/// A `shortName`/`modelCreatorName`/speed field may sit this far into the
/// escaped record; anything farther back belongs to another record (or the
/// i18n label tables, which carry field-shaped keys with no names around).
const ATTRIBUTION_WINDOW: usize = 2_500;

fn escaped_number(key: &str) -> Regex {
    Regex::new(&format!(
        r#"{key}\\?":(null|-?[0-9]+\.?[0-9]*(?:[eE][+-]?[0-9]+)?)"#
    ))
    .expect("valid field regex")
}

fn escaped_string(key: &str) -> Regex {
    Regex::new(&format!(r#"\\?"{key}\\?":\\?"(.{{1,120}}?)\\?""#)).expect("valid field regex")
}

fn parse_number(raw: &str) -> Option<f64> {
    if raw == "null" {
        return None;
    }
    let value: f64 = raw.parse().ok()?;
    value.is_finite().then_some(value)
}

/// Every record carries the median-throughput key (null or not) before the
/// rest of the speed block, so its occurrences anchor the block.
fn speed_anchors(html: &str) -> Vec<(usize, Option<f64>)> {
    let re = escaped_number("medianOutputTokensPerSecond");
    re.captures_iter(html)
        .filter_map(|caps| {
            let whole = caps.get(0).expect("whole match");
            let raw = caps.extract::<1>().1[0];
            // Position the anchor on the key itself so it precedes the value
            // and every later field of the same record.
            let key_start = whole.as_str().find("medianOutputTokensPerSecond")?;
            Some((whole.start() + key_start, parse_number(raw)))
        })
        .collect()
}

/// Attribute each occurrence of one field to the nearest preceding anchor.
fn values_by_anchor(html: &str, key: &str, anchors: &[(usize, Option<f64>)]) -> Vec<Option<f64>> {
    let mut out: Vec<Option<f64>> = vec![None; anchors.len()];
    let re = escaped_number(key);
    let mut anchor_idx = 0usize;
    for caps in re.captures_iter(html) {
        let whole = caps.get(0).expect("whole match");
        while anchor_idx + 1 < anchors.len() && anchors[anchor_idx + 1].0 <= whole.start() {
            anchor_idx += 1;
        }
        if anchors.is_empty() || anchors[anchor_idx].0 > whole.start() {
            continue;
        }
        if whole.start() - anchors[anchor_idx].0 > ATTRIBUTION_WINDOW {
            continue;
        }
        out[anchor_idx] = parse_number(caps.extract::<1>().1[0]);
    }
    out
}

/// Attribute each `shortName`/`modelCreatorName` occurrence to the nearest
/// *following* anchor: names sit at the top of the record, before the speed
/// block the anchor opens.
fn string_for_anchor(
    html: &str,
    key: &str,
    anchors: &[(usize, Option<f64>)],
) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None; anchors.len()];
    let re = escaped_string(key);
    let mut anchor_idx = 0usize;
    for caps in re.captures_iter(html) {
        let whole = caps.get(0).expect("whole match");
        // Advance past anchors that opened before this name; the name
        // belongs to the first anchor at or after it.
        while anchor_idx < anchors.len() && anchors[anchor_idx].0 < whole.start() {
            anchor_idx += 1;
        }
        let Some((anchor_pos, _)) = anchors.get(anchor_idx) else {
            continue;
        };
        if *anchor_pos - whole.start() > ATTRIBUTION_WINDOW {
            continue;
        }
        out[anchor_idx] = Some(caps.extract::<1>().1[0].replace("\\u0026", "&"));
    }
    out
}

/// Variant preference for speed reads — deliberately NOT the Intelligence
/// Index order. Throughput barely moves between effort variants, but the
/// max-effort TTFT includes queue/fallback time that scales with effort
/// (Claude Opus 5.5 measures 9 s at low effort and 706 s at max on the same
/// board), so the calmest variant is the cleaner generation-speed read:
/// bare entry first, then efforts low→max, explicit non-reasoning last.
fn speed_variant_rank(effort: Option<&str>, non_reasoning: bool) -> usize {
    if non_reasoning {
        return 0;
    }
    let Some(effort) = effort else {
        return 6;
    };
    match NAMED_EFFORTS.iter().position(|t| *t == effort) {
        // NAMED_EFFORTS runs low→max, so this inverts it.
        Some(position) => NAMED_EFFORTS.len() - position,
        None => NAMED_EFFORTS.len(),
    }
}

/// Parse the standardized-speed table out of the leaderboard HTML. `Err` when
/// no usable rows are found (page shape drift), so callers can report it.
pub(crate) fn parse_aa_speed_snapshot(html: &str) -> Result<AaSpeedSnapshot, String> {
    let anchors = speed_anchors(html);
    if anchors.is_empty() {
        return Err("no speed records found in payload".into());
    }
    let names = string_for_anchor(html, "shortName", &anchors);
    let creators = string_for_anchor(html, "modelCreatorName", &anchors);
    let tps_p05 = values_by_anchor(html, "percentile05OutputTokensPerSecond", &anchors);
    let tps_p25 = values_by_anchor(html, "quartile25OutputTokensPerSecond", &anchors);
    let tps_p75 = values_by_anchor(html, "quartile75OutputTokensPerSecond", &anchors);
    let tps_p95 = values_by_anchor(html, "percentile95OutputTokensPerSecond", &anchors);
    let ttft_p50 = values_by_anchor(html, "medianTimeToFirstTokenSeconds", &anchors);
    let ttft_p95 = values_by_anchor(html, "percentile95TimeToFirstTokenSeconds", &anchors);
    let e2e = values_by_anchor(html, "medianEndToEndResponseTimeSeconds", &anchors);

    // Variants fold into families by the shared classifier, keeping the same
    // preference order as the Intelligence Index overlay.
    let mut best: std::collections::HashMap<String, AaSpeedEntry> =
        std::collections::HashMap::new();
    let mut preference: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for idx in 0..anchors.len() {
        let Some(short) = names[idx].as_deref() else {
            continue;
        };
        let has_telemetry =
            anchors[idx].1.is_some() || ttft_p50[idx].is_some() || e2e[idx].is_some();
        if !has_telemetry {
            continue;
        }
        let Some((key, name, effort, non_reasoning)) = classify_aa_name(short) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let rank = speed_variant_rank(effort.as_deref(), non_reasoning);
        if preference.get(&key).is_none_or(|old| rank > *old) {
            preference.insert(key.clone(), rank);
            let entry = AaSpeedEntry {
                key,
                name,
                creator: creators[idx].clone().unwrap_or_default(),
                effort,
                median_tps: anchors[idx].1,
                tps_p05: tps_p05[idx],
                tps_p25: tps_p25[idx],
                tps_p75: tps_p75[idx],
                tps_p95: tps_p95[idx],
                median_ttft_s: ttft_p50[idx],
                ttft_p95_s: ttft_p95[idx],
                median_e2e_s: e2e[idx],
            };
            best.insert(entry.key.clone(), entry);
        }
    }
    if best.is_empty() {
        return Err("no named speed records survived parsing".into());
    }
    let mut entries: Vec<AaSpeedEntry> = best.into_values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(AaSpeedSnapshot {
        entries,
        fetched_at: Utc::now(),
    })
}

/// Join a Surplus quote to its AA standardized-speed family, using the same
/// rules as the benchmark join: exact canonical key first, then the 0.80
/// fuzzy bar with creator compatibility and the release-tag fallback.
pub(crate) fn aa_speed_for_quote<'a>(
    entries: &'a [AaSpeedEntry],
    quote: &Quote,
) -> Option<&'a AaSpeedEntry> {
    let q_model = benchmark_model_key(&quote.model);
    let q_name = benchmark_model_key(&quote.display_name);
    if let Some(exact) = entries
        .iter()
        .find(|e| !e.key.is_empty() && (e.key == q_model || e.key == q_name))
    {
        return Some(exact);
    }

    let inferred = infer_creator(&quote.model);
    let q_creator = canonical_creator(if quote.creator.is_empty() {
        &inferred
    } else {
        &quote.creator
    });
    let mut best: Option<(&AaSpeedEntry, f64)> = None;
    for entry in entries {
        if entry.key.is_empty()
            || !creators_compatible(&q_creator, &canonical_creator(&entry.creator))
        {
            continue;
        }
        let mut score = token_jaccard(&q_name, &entry.key).max(token_jaccard(&q_model, &entry.key));
        if keys_differ_only_by_release_tag(&q_name, &entry.key)
            || keys_differ_only_by_release_tag(&q_model, &entry.key)
        {
            score = score.max(0.95);
        }
        if score >= 0.80 && best.map(|(_, current)| score > current).unwrap_or(true) {
            best = Some((entry, score));
        }
    }
    best.map(|(entry, _)| entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::aa_evals::alias_display_name;
    use crate::testfix::test_quote;

    /// Family lookup by display name, for tests.
    fn aa_speed_named<'a>(entries: &'a [AaSpeedEntry], name: &str) -> Option<&'a AaSpeedEntry> {
        let key = benchmark_model_key(alias_display_name(name));
        entries.iter().find(|e| e.key == key)
    }

    /// Shaped from the live leaderboard payload (2026-10): a bare record, an
    /// effort-variant pair, a record with null throughput, and i18n label
    /// noise carrying field-shaped keys with no names around.
    fn live_payload() -> String {
        [
            r#"{"slug":"jt-35b-flash","name":"JT-35B-Flash","shortName":"JT-35B-Flash","#,
            r#""modelCreatorName":"China Mobile","contextWindowTokens":256000,"#,
            r#""intelligenceIndex":18.66,"medianOutputTokensPerSecond":119.689357199807,"#,
            r#""percentile05OutputTokensPerSecond":90.6496724270326,"#,
            r#""quartile25OutputTokensPerSecond":104.815414571175,"#,
            r#""quartile75OutputTokensPerSecond":131.832782597235,"#,
            r#""percentile95OutputTokensPerSecond":159.262559703555,"#,
            r#""medianTimeToFirstTokenSeconds":2.09896740350001,"#,
            r#""medianTimeToFirstAnswerTokenSeconds":2.09896740350001,"#,
            r#""percentile05TimeToFirstTokenSeconds":1.98065877010001,"#,
            r#""percentile95TimeToFirstTokenSeconds":3.34039210780005,"#,
            r#""medianEndToEndResponseTimeSeconds":6.276448273125787},"#,
            r#"{"slug":"opus-max","shortName":"Claude Opus 5.5 (max with fallback)","#,
            r#""modelCreatorName":"Anthropic","medianOutputTokensPerSecond":61.2,"#,
            r#""percentile05OutputTokensPerSecond":50.1,"quartile25OutputTokensPerSecond":55.0,"#,
            r#""quartile75OutputTokensPerSecond":70.0,"percentile95OutputTokensPerSecond":80.9,"#,
            r#""medianTimeToFirstTokenSeconds":0.68,"percentile95TimeToFirstTokenSeconds":1.4,"#,
            r#""medianEndToEndResponseTimeSeconds":5.1},"#,
            r#"{"slug":"opus-xhigh","shortName":"Claude Opus 5.5 (xhigh)","#,
            r#""modelCreatorName":"Anthropic","medianOutputTokensPerSecond":72.5,"#,
            r#""percentile05OutputTokensPerSecond":60.0,"quartile25OutputTokensPerSecond":65.0,"#,
            r#""quartile75OutputTokensPerSecond":80.0,"percentile95OutputTokensPerSecond":90.0,"#,
            r#""medianTimeToFirstTokenSeconds":0.7,"percentile95TimeToFirstTokenSeconds":1.5,"#,
            r#""medianEndToEndResponseTimeSeconds":4.9},"#,
            r#"{"slug":"not-measured","shortName":"Not Measured Model","#,
            r#""modelCreatorName":"Example","medianOutputTokensPerSecond":null,"#,
            r#""percentile05OutputTokensPerSecond":null,"quartile25OutputTokensPerSecond":null,"#,
            r#""quartile75OutputTokensPerSecond":null,"percentile95OutputTokensPerSecond":null,"#,
            r#""medianTimeToFirstTokenSeconds":null,"percentile95TimeToFirstTokenSeconds":null,"#,
            r#""medianEndToEndResponseTimeSeconds":null},"#,
            // Label-table noise: field-shaped keys far from any shortName.
            r#"{"columns":{"speed":"Speed","median":"Median","#,
            r#""tokensPerSecond":"Tokens/s","latency":"Latency"}}"#,
        ]
        .concat()
    }

    #[test]
    fn parses_full_distributions_and_folds_variants_by_preference() {
        let snapshot = parse_aa_speed_snapshot(&live_payload()).expect("parses");
        // The all-null record and the label noise contribute nothing.
        assert_eq!(snapshot.entries.len(), 2);

        let jt = aa_speed_named(&snapshot.entries, "JT-35B-Flash").unwrap();
        assert_eq!(jt.creator, "China Mobile");
        assert!((jt.median_tps.unwrap() - 119.689357199807).abs() < 1e-9);
        assert!((jt.tps_p05.unwrap() - 90.6496724270326).abs() < 1e-9);
        assert!((jt.tps_p95.unwrap() - 159.262559703555).abs() < 1e-9);
        assert!((jt.median_ttft_s.unwrap() - 2.09896740350001).abs() < 1e-9);
        assert!((jt.ttft_p95_s.unwrap() - 3.34039210780005).abs() < 1e-9);
        assert!((jt.median_e2e_s.unwrap() - 6.276448273125787).abs() < 1e-9);

        // Speed preference is low-effort-first: (xhigh) outranks (max), the
        // max read being queue-time-polluted.
        let opus = aa_speed_named(&snapshot.entries, "Claude Opus 5.5").unwrap();
        assert_eq!(opus.effort.as_deref(), Some("xhigh"));
        assert!((opus.median_tps.unwrap() - 72.5).abs() < 1e-9);

        assert!(snapshot.fetched_at <= Utc::now());
    }

    #[test]
    fn payloads_without_speed_records_are_rejected() {
        assert!(parse_aa_speed_snapshot("<html>empty</html>").is_err());
        // Only the label-table noise, no real records.
        let noise = r#"{"columns":{"medianOutputTokensPerSecond":"x"}}"#;
        // The key must be followed by a number/null to anchor; a string value
        // does not match, so this is not an anchor at all.
        assert!(parse_aa_speed_snapshot(noise).is_err());
    }

    #[test]
    fn quote_join_uses_exact_key_then_fuzzy_bar() {
        let snapshot = parse_aa_speed_snapshot(&live_payload()).unwrap();

        // Exact display-name match (the folded family keeps the xhigh read).
        let mut quote = test_quote("claude-opus-5-5", 1.0, true);
        quote.display_name = "Claude Opus 5.5".into();
        quote.creator = "Anthropic".into();
        let hit = aa_speed_for_quote(&snapshot.entries, &quote).unwrap();
        assert!((hit.median_tps.unwrap() - 72.5).abs() < 1e-9);

        // Fuzzy: a dated listing of the SAME family still joins through the
        // release-tag fallback.
        let mut dated = test_quote("anthropic/claude-opus-5-5-0528", 1.0, true);
        dated.display_name = "Claude Opus 5.5 0528".into();
        dated.creator = "Anthropic".into();
        assert!(aa_speed_for_quote(&snapshot.entries, &dated).is_some());

        // A DIFFERENT model never joins just because set-deduped tokens
        // collapse `opus 5` onto `opus 5 5` (Opus 5 vs Opus 5.5).
        let mut near = test_quote("anthropic/claude-opus-5", 1.0, true);
        near.display_name = "Claude Opus 5".into();
        near.creator = "Anthropic".into();
        assert!(aa_speed_for_quote(&snapshot.entries, &near).is_none());

        // Different creator never fuzzy-joins.
        let mut stranger = test_quote("acme-opus-5-5-turbo", 1.0, true);
        stranger.display_name = "Acme Opus 5.5 Turbo".into();
        stranger.creator = "Acme".into();
        assert!(aa_speed_for_quote(&snapshot.entries, &stranger).is_none());

        // Nothing remotely close.
        let mut other = test_quote("totally-unrelated", 1.0, true);
        other.display_name = "Totally Unrelated".into();
        assert!(aa_speed_for_quote(&snapshot.entries, &other).is_none());
    }
}
