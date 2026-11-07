//! Small text/JSON parsing utilities shared across the board fetchers,
//! including the escaped-RSC `shortName` attribution every AA board read
//! builds on.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// A `shortName`/score field may sit at most this far from the name it
/// belongs to; anything farther back belongs to another record (or the i18n
/// label tables, which carry field-shaped keys with no names around).
pub(crate) const ATTRIBUTION_WINDOW: usize = 2_500;

static SHORT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex"));

/// Every `\"shortName\":\"…\"` occurrence in an escaped-JSON RSC payload,
/// in document order, with the display name's `\u0026` escapes resolved.
pub(crate) fn rsc_short_names(html: &str) -> Vec<(usize, String)> {
    SHORT_NAME_RE
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).expect("whole match").start(),
                c.extract::<1>().1[0].replace("\\u0026", "&"),
            )
        })
        .collect()
}

/// The nearest shortName strictly before `pos`, when it sits within
/// `max_gap` bytes (`None` = unlimited) — the attribution rule the AA board
/// parsers share: the score occurrence belongs to the model named just
/// before it.
pub(crate) fn short_name_before(
    names: &[(usize, String)],
    pos: usize,
    max_gap: Option<usize>,
) -> Option<&str> {
    let (start, name) = names.iter().rev().find(|(start, _)| *start < pos)?;
    if let Some(gap) = max_gap
        && pos - start > gap
    {
        return None;
    }
    Some(name)
}

/// Sort rows by `agentic_coding` descending — the leaderboard display order
/// every board parser ends with.
pub(crate) fn sort_rows_by_score_desc(rows: &mut [crate::types::Benchmark]) {
    rows.sort_by(|a, b| {
        b.agentic_coding
            .unwrap_or_default()
            .total_cmp(&a.agentic_coding.unwrap_or_default())
    });
}

pub(crate) fn strip_html_tags(html: &str) -> String {
    let re = Regex::new(r"(?s)<[^>]*>").expect("valid tag regex");
    let stripped = re.replace_all(html, " ");
    stripped
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
}

pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

pub(crate) fn json_shape(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().take(12).cloned().collect::<Vec<_>>();
            keys.sort();
            format!("object keys [{}]", keys.join(", "))
        }
        Value::Array(items) => format!("array len {}", items.len()),
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Number(_) => "number".into(),
        Value::String(v) => format!("string len {}", v.len()),
    }
}

pub(crate) fn first_number_path(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths
        .iter()
        .find_map(|p| value_at_path(value, p).and_then(number_value))
}

pub(crate) fn first_string_path(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|p| {
        value_at_path(value, p)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn value_at_path<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

pub(crate) fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(*k).and_then(Value::as_str).map(str::to_owned))
}
