//! LiveBench: discovers the latest release, then parses category CSVs.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

use super::fetch_text;
use crate::theme::infer_creator;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const LIVEBENCH_CONSTANTS_URL: &str =
    "https://raw.githubusercontent.com/LiveBench/new-livebench/main/src/lib/constants.js";
pub(crate) const LIVEBENCH_PUBLIC_BASE: &str =
    "https://raw.githubusercontent.com/LiveBench/new-livebench/main/public";

pub(crate) fn fetch_livebench(client: &Client) -> Result<Vec<Benchmark>> {
    let constants = fetch_text(client, LIVEBENCH_CONSTANTS_URL, "LiveBench release list")?;
    let release = latest_livebench_release(&constants)
        .ok_or_else(|| anyhow!("Could not find a release date in LiveBench constants.js"))?;
    let file_date = release.replace('-', "_");
    let table_url = format!("{LIVEBENCH_PUBLIC_BASE}/table_{file_date}.csv");
    let categories_url = format!("{LIVEBENCH_PUBLIC_BASE}/categories_{file_date}.json");
    let table = fetch_text(client, &table_url, "LiveBench score table")?;
    let categories_text = fetch_text(client, &categories_url, "LiveBench categories")?;
    let categories = parse_livebench_categories(&categories_text)?;
    let benchmarks = parse_livebench_csv(&table, &categories)?;
    if benchmarks.is_empty() {
        return Err(anyhow!(
            "LiveBench {release} contained no usable model scores"
        ));
    }
    Ok(benchmarks)
}

pub(crate) fn latest_livebench_release(text: &str) -> Option<String> {
    text.split(|c: char| {
        c == '"' || c == '\'' || c == ',' || c == '[' || c == ']' || c.is_whitespace()
    })
    .filter(|token| is_iso_date(token))
    .max()
    .map(str::to_owned)
}

pub(crate) fn is_iso_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
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
        let agentic_coding = categories
            .get("Agentic Coding")
            .and_then(|tasks| livebench_category_average(&headers, &row, tasks));
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
        if let Ok(value) = raw.trim().parse::<f64>() {
            if value.is_finite() {
                scores.push(value);
            }
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
