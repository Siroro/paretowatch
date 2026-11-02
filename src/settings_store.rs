//! Persistence for user settings: poll interval, blended weights, and alert
//! rules.

use std::{
    fs,
    path::{Path, PathBuf},
};

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
    load_settings_at(&config_path())
}

fn load_settings_at(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    // An unreadable file must survive the error: the caller falls back to
    // defaults, and a later save would otherwise overwrite the original with
    // them. Archive the bytes first. A file that vanished between the
    // exists() check and the read just makes the archive a no-op.
    parse_settings_file(path).inspect_err(|_| archive_unreadable(path))
}

fn parse_settings_file(path: &Path) -> Result<Settings> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
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

/// Move an unreadable config aside as `<name>.bad-<utc timestamp>` so the
/// original bytes survive for inspection and a previous archive is never
/// overwritten. Best-effort: if the rename is refused (locked file), fall
/// back to a copy and continue either way.
fn archive_unreadable(path: &Path) {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let archived = path.with_extension(format!("json.bad-{stamp}"));
    if fs::rename(path, &archived).is_err() {
        let _ = fs::copy(path, &archived);
    }
}

pub(crate) fn save_settings(settings: &Settings) -> Result<()> {
    save_settings_at(settings, &config_path())
}

fn save_settings_at(settings: &Settings, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings)?;
    // Atomic temp-file + rename (the notification-log pattern) so a crash
    // mid-write cannot leave a truncated config. Some Windows filesystems
    // refuse rename-over-existing, so remove the old file only as a last
    // resort — and only after the new bytes are safely on disk.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    if fs::rename(&tmp, path).is_err() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("paretowatch-settings-tests");
        let _ = fs::create_dir_all(&dir);
        dir.join(format!("{}-{name}.json", std::process::id()))
    }

    /// Leftover `.bad-` archives of `path` from earlier runs of this test.
    fn bad_archives(path: &Path) -> Vec<PathBuf> {
        let prefix = format!("{}.bad-", path.file_name().unwrap().to_string_lossy());
        let mut hits: Vec<PathBuf> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
            })
            .collect();
        hits.sort();
        hits
    }

    fn remove_sidecars(path: &Path) {
        for archive in bad_archives(path) {
            let _ = fs::remove_file(archive);
        }
    }

    #[test]
    fn save_then_load_round_trips_through_the_atomic_write() {
        let path = temp_config("round-trip");
        let _ = fs::remove_file(&path);
        remove_sidecars(&path);
        let mut settings = Settings::default();
        // The second save is the one that has to replace an existing file.
        save_settings_at(&settings, &path).expect("first save");
        save_settings_at(&settings, &path).expect("overwrite save");
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file must not survive the save"
        );
        settings.poll_seconds = 120;
        settings.quiet_hours_start = 23;
        save_settings_at(&settings, &path).expect("final save");
        let loaded = load_settings_at(&path).expect("load after save");
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&settings).unwrap()
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_config_is_archived_not_destroyed() {
        let path = temp_config("corrupt");
        let _ = fs::remove_file(&path);
        remove_sidecars(&path);
        fs::write(&path, b"{ not valid json").unwrap();
        assert!(load_settings_at(&path).is_err());
        // The bytes moved aside under a fresh `.bad-` timestamp instead of
        // staying where the next save would destroy them.
        assert!(!path.exists(), "original must be gone after archiving");
        let archives = bad_archives(&path);
        assert_eq!(archives.len(), 1, "exactly one archive expected");
        assert_eq!(fs::read(&archives[0]).unwrap(), b"{ not valid json");
        // With the original gone, the next load is clean defaults.
        let loaded = load_settings_at(&path).expect("defaults after archive");
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(Settings::default()).unwrap()
        );
        let _ = fs::remove_file(&archives[0]);
    }

    #[test]
    fn schema_mismatch_config_is_archived_too() {
        // from_value failing (a field serde rejects) must archive the same
        // way a JSON syntax error does.
        let path = temp_config("schema");
        let _ = fs::remove_file(&path);
        remove_sidecars(&path);
        fs::write(&path, r#"{"poll_seconds": "not a number"}"#).unwrap();
        assert!(load_settings_at(&path).is_err());
        assert!(!path.exists(), "original must be gone after archiving");
        let archives = bad_archives(&path);
        assert_eq!(archives.len(), 1, "exactly one archive expected");
        let _ = fs::remove_file(&archives[0]);
    }
}
