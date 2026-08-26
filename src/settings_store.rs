//! Persistence for user settings: poll interval, blended weights, and alert
//! rules.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde_json::Value;

use crate::types::Settings;

pub(crate) fn config_path() -> PathBuf {
    if let Some(project) = ProjectDirs::from("ai", "ParetoWatch", "ParetoWatch") {
        project.config_dir().join("config.json")
    } else {
        PathBuf::from("paretowatch-config.json")
    }
}

pub(crate) fn load_settings() -> Result<Settings> {
    let path = config_path();
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut raw: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let had_cache_weight = raw.get("cache_read_weight").is_some();
    // SWE-bench Verified and SWE-bench Live were removed as stale boards.
    // Drop alerts that still point at them before deserializing, so one dead
    // variant cannot fail the whole file and reset every alert and weight to
    // defaults.
    if let Some(alerts) = raw.get_mut("alerts").and_then(Value::as_array_mut) {
        alerts.retain(|alert| {
            !matches!(
                alert.get("benchmark_source").and_then(Value::as_str),
                Some("SWEBenchVerified") | Some("SWEBenchLive"),
            )
        });
    }
    let mut settings: Settings =
        serde_json::from_value(raw).with_context(|| format!("parse {}", path.display()))?;

    if !had_cache_weight {
        // v0.3 had a two-leg, unnormalized 1:3 input/output default. That is
        // backwards for agentic coding. Migrate only that exact old default to
        // the new agentic preset; preserve custom old mixes as no-cache weights.
        if (settings.input_weight - 1.0).abs() < 1e-9 && (settings.output_weight - 3.0).abs() < 1e-9
        {
            settings.input_weight = 15.0;
            settings.cache_read_weight = 80.0;
            settings.output_weight = 5.0;
        } else {
            settings.cache_read_weight = 0.0;
        }
    }
    Ok(settings)
}

pub(crate) fn save_settings(settings: &Settings) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings)?;
    fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))
}
