//! LiveBench 2.0: discovers the latest release from the site's app bundle,
//! then parses the release's category CSVs.
//!
//! The project deleted its `LiveBench/new-livebench` GitHub repository in the
//! October 2026 consolidation and now serves everything from livebench.ai
//! itself; the 2.0 boards (long-horizon agentic work) replaced the v1
//! question-set boards, which are gone from the site's navigation and are not
//! fetched. The SPA homepage is a static shell that names its JS bundle, and
//! the bundle carries the release list as a plain string array of compact
//! `YYYYMMDD` labels with tables under `/data/<label>/`. Discovery therefore
//! reads the homepage, pulls the bundle URL out of the script tag, and takes
//! the newest label it finds — the same read-a-JS-file-and-regex-it approach
//! the old `constants.js` flow used.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

use super::fetch_text;
use crate::theme::infer_creator;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const LIVEBENCH_HOME_URL: &str = "https://livebench.ai/";
pub(crate) const LIVEBENCH_BUNDLE_BASE: &str = "https://livebench.ai/static/js/";
pub(crate) const LIVEBENCH_DATA_BASE: &str = "https://livebench.ai/data";

pub(crate) fn fetch_livebench(client: &Client) -> Result<Vec<Benchmark>> {
    let home = fetch_text(client, LIVEBENCH_HOME_URL, "LiveBench site")?;
    let bundle_path = livebench_bundle_path(&home).ok_or_else(|| {
        anyhow!("Could not find the LiveBench app bundle path in the homepage HTML")
    })?;
    let bundle_url = format!("{LIVEBENCH_BUNDLE_BASE}{bundle_path}");
    let bundle = fetch_text(client, &bundle_url, "LiveBench app bundle")?;
    let label = livebench_release_label(&bundle)
        .ok_or_else(|| anyhow!("Could not find a release label in the LiveBench app bundle"))?;
    let table_url = format!("{LIVEBENCH_DATA_BASE}/{label}/table.csv");
    let categories_url = format!("{LIVEBENCH_DATA_BASE}/{label}/categories.json");
    let table = fetch_text(client, &table_url, "LiveBench score table")?;
    let categories_text = fetch_text(client, &categories_url, "LiveBench categories")?;
    let categories = parse_livebench_categories(&categories_text)?;
    let benchmarks = parse_livebench_csv(&table, &categories)?;
    if benchmarks.is_empty() {
        return Err(anyhow!(
            "LiveBench release {label} contained no usable model scores"
        ));
    }
    Ok(benchmarks)
}

/// The SPA shell names its chunk inline
/// (`<script defer="defer" src="./static/js/main.<hash>.js">`); pull the path
/// after `./static/js/` out of the first such attribute.
pub(crate) fn livebench_bundle_path(home_html: &str) -> Option<String> {
    const MARKER: &str = "./static/js/";
    let start = home_html.find(MARKER)? + MARKER.len();
    let end = start + home_html[start..].find('"')?;
    let path = &home_html[start..end];
    (!path.is_empty() && path.ends_with(".js")).then(|| path.to_owned())
}

/// Newest `YYYYMMDD` release label advertised in the app bundle.
pub(crate) fn livebench_release_label(bundle: &str) -> Option<String> {
    quoted_string_array_items(bundle, is_release_label)
        .into_iter()
        .max()
}

/// Collect the items of every `["a","b",...]` string array whose items all
/// satisfy `valid`. Mixed or non-literal arrays (nested objects, ids, colors)
/// fail the predicate and are skipped, which is what keeps bundle scanning
/// from mistaking unrelated arrays for the release list.
pub(crate) fn quoted_string_array_items(haystack: &str, valid: fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find("[\"") {
        let open = search_from + rel;
        let Some(rel_close) = haystack[open..].find(']') else {
            break;
        };
        let close = open + rel_close;
        // `close` sits on `]`; step back over the closing quote so the last
        // item does not carry a trailing `"` into the predicate.
        let body_end = if haystack[..close].ends_with('"') {
            close - 1
        } else {
            close
        };
        let body = &haystack[open + 2..body_end];
        if !body.is_empty() {
            let items: Vec<&str> = body.split("\",\"").collect();
            if items.iter().all(|item| valid(item)) {
                out.extend(items.into_iter().map(str::to_owned));
            }
        }
        search_from = close + 1;
    }
    out
}

pub(crate) fn is_release_label(s: &str) -> bool {
    s.len() == 8 && s.starts_with("20") && s.as_bytes().iter().all(|b| b.is_ascii_digit())
}

pub(crate) fn parse_livebench_categories(text: &str) -> Result<HashMap<String, Vec<String>>> {
    let value: Value =
        serde_json::from_str(text).context("Could not parse LiveBench categories JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("LiveBench categories root is not an object"))?;
    let mut out = HashMap::new();
    for (category, tasks) in object {
        let Some(tasks) = tasks.as_array() else {
            continue;
        };
        let names = tasks
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !names.is_empty() {
            out.insert(category.clone(), names);
        }
    }
    Ok(out)
}

pub(crate) fn parse_livebench_csv(
    table: &str,
    categories: &HashMap<String, Vec<String>>,
) -> Result<Vec<Benchmark>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(table.as_bytes());
    let headers = reader
        .headers()
        .context("LiveBench CSV has no header")?
        .clone();
    let model_index = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("model"))
        .ok_or_else(|| anyhow!("LiveBench CSV has no model column"))?;
    let mut out = Vec::new();
    for row in reader.records() {
        let row = row.context("Could not parse a LiveBench CSV row")?;
        let Some(model) = row
            .get(model_index)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let category_scores = categories
            .values()
            .filter_map(|tasks| livebench_category_average(&headers, &row, tasks))
            .collect::<Vec<_>>();
        let overall = mean(&category_scores);
        let coding = categories
            .get("Coding")
            .and_then(|tasks| livebench_category_average(&headers, &row, tasks));
        // The 2.0 board's agentic category is "Long Agentic"; fall back to the
        // overall mean so a release without that column still contributes
        // rows instead of silently mapping nothing.
        let agentic_coding = categories
            .get("Long Agentic")
            .and_then(|tasks| livebench_category_average(&headers, &row, tasks))
            .or(overall);
        out.push(Benchmark {
            slug: model.to_owned(),
            name: model.to_owned(),
            creator: infer_creator(model),
            overall,
            coding,
            agentic_coding,
            agent: None,
            reasoning_effort: None,
            kind: BenchmarkKind::Model,
            tokens_per_task: None,
            token_profile: None,
        });
    }
    Ok(out)
}

pub(crate) fn livebench_category_average(
    headers: &csv::StringRecord,
    row: &csv::StringRecord,
    tasks: &[String],
) -> Option<f64> {
    let mut scores = Vec::new();
    for task in tasks {
        let Some(index) = headers.iter().position(|h| h == task) else {
            continue;
        };
        let Some(raw) = row.get(index) else { continue };
        if let Ok(value) = raw.trim().parse::<f64>()
            && value.is_finite()
        {
            scores.push(value);
        }
    }
    mean(&scores)
}

pub(crate) fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_path_comes_from_the_homepage_script_tag() {
        let home = r#"<script defer="defer" src="./static/js/main.074b2ee8.js"></script>"#;
        assert_eq!(
            livebench_bundle_path(home),
            Some("main.074b2ee8.js".to_owned())
        );
        assert_eq!(livebench_bundle_path("<html>no scripts</html>"), None);
    }

    #[test]
    fn newest_release_label_wins() {
        let bundle = r#"const a=["20261001","20261014"],other=[1,2];"#;
        assert_eq!(livebench_release_label(bundle), Some("20261014".to_owned()));
        assert_eq!(livebench_release_label("no arrays here"), None);
    }

    #[test]
    fn array_scanning_skips_mixed_and_nested_arrays() {
        let bundle = r##"["20261014"],["2026-06-25"],["#fff","#000"],["20261014",42],[{a:1}]"##;
        assert_eq!(
            quoted_string_array_items(bundle, is_release_label),
            vec!["20261014".to_owned()]
        );
    }

    #[test]
    fn two_point_zero_categories_feed_the_parser() {
        // The 2.0 board ships one column per category; "Long Agentic" is the
        // agentic capability column.
        let categories: HashMap<String, Vec<String>> = serde_json::from_str::<Value>(
            r#"{"Long Agentic":["long_agentic"],"Reasoning":["reasoning"],"Coding":["coding"]}"#,
        )
        .ok()
        .and_then(|v| parse_livebench_categories(&v.to_string()).ok())
        .unwrap();
        let table = "model,long_agentic,reasoning,coding\nbeam-1.0,17.3,80.2,83.9\n";
        let rows = parse_livebench_csv(table, &categories).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slug, "beam-1.0");
        assert_eq!(rows[0].coding, Some(83.9));
        assert_eq!(rows[0].agentic_coding, Some(17.3));
        assert!((rows[0].overall.unwrap() - (17.3 + 80.2 + 83.9) / 3.0).abs() < 1e-9);
    }

    #[test]
    fn missing_agentic_category_falls_back_to_the_overall_mean() {
        let categories: HashMap<String, Vec<String>> =
            serde_json::from_str::<Value>(r#"{"Reasoning":["reasoning"],"Coding":["coding"]}"#)
                .ok()
                .and_then(|v| parse_livebench_categories(&v.to_string()).ok())
                .unwrap();
        let table = "model,reasoning,coding\nsome-model,50.0,70.0\n";
        let rows = parse_livebench_csv(table, &categories).unwrap();
        assert_eq!(rows[0].agentic_coding, Some(60.0));
    }
}
