//! Small text/JSON parsing utilities shared across the board fetchers.

use regex::Regex;
use serde_json::Value;

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
