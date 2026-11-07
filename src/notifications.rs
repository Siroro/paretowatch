//! Persistent desktop-notification fan-out, delivery policy, and alert audio.
//!
//! Every alert event is retained on disk even when toast/audio delivery is
//! suppressed by quiet hours, a temporary mute, or an alert cooldown. Sounds
//! are WAV assets compiled into the executable with `include_bytes!`, so the
//! release remains a single binary with no runtime asset directory.

use std::collections::VecDeque;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Local, Timelike, Utc};
use notify_rust::Notification;
use serde::{Deserialize, Serialize};

use crate::types::{AlertRearm, AlertRule, AlertSound, Settings};

const MAX_RECORDS: usize = 2_000;
const MAX_PRICE_MOVES: usize = 2_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum NotificationKind {
    Price,
    Discount,
    SellerHealth,
    Frontier,
    Listing,
    #[default]
    Other,
}

impl NotificationKind {
    pub(crate) const ALL: [Self; 6] = [
        Self::Price,
        Self::Discount,
        Self::SellerHealth,
        Self::Frontier,
        Self::Listing,
        Self::Other,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Price => "Price",
            Self::Discount => "Discount",
            Self::SellerHealth => "Seller health",
            Self::Frontier => "Frontier / benchmark",
            Self::Listing => "Listings",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NotificationRecord {
    pub(crate) at: DateTime<Utc>,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) kind: NotificationKind,
    pub(crate) model: Option<String>,
    pub(crate) alert_id: Option<u64>,
    pub(crate) delivered: bool,
    pub(crate) suppressed_reason: Option<String>,
}

impl Default for NotificationRecord {
    fn default() -> Self {
        Self {
            at: Utc::now(),
            summary: String::new(),
            body: String::new(),
            kind: NotificationKind::Other,
            model: None,
            alert_id: None,
            delivered: true,
            suppressed_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PriceMoveRecord {
    pub(crate) model: String,
    pub(crate) display_name: String,
    pub(crate) at: DateTime<Utc>,
    pub(crate) old_blended: f64,
    pub(crate) new_blended: f64,
    pub(crate) old_input: f64,
    pub(crate) new_input: f64,
    pub(crate) old_output: f64,
    pub(crate) new_output: f64,
    pub(crate) source: String,
}

impl PriceMoveRecord {
    pub(crate) fn delta(&self) -> f64 {
        self.new_blended - self.old_blended
    }
    pub(crate) fn percent_delta(&self) -> Option<f64> {
        if self.old_blended.abs() <= f64::EPSILON {
            None
        } else {
            Some((self.new_blended - self.old_blended) / self.old_blended * 100.0)
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedLog {
    records: VecDeque<NotificationRecord>,
    price_moves: VecDeque<PriceMoveRecord>,
    muted_until: Option<DateTime<Utc>>,
}

pub(crate) struct NotificationLog {
    records: VecDeque<NotificationRecord>,
    price_moves: VecDeque<PriceMoveRecord>,
    muted_until: Option<DateTime<Utc>>,
    quiet_hours_enabled: bool,
    quiet_hours_start: u8,
    quiet_hours_end: u8,
    path: PathBuf,
}

/// Whether local hour `hour` falls in the quiet-hours window. A start
/// greater than the end wraps midnight (22..07 = quiet overnight).
fn quiet_at(enabled: bool, start: u8, end: u8, hour: u8) -> bool {
    if !enabled || start == end {
        return false;
    }
    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

impl NotificationLog {
    pub(crate) fn open(settings: &Settings) -> Self {
        Self::open_at(notification_log_path(), settings)
    }

    pub(crate) fn open_at(path: PathBuf, settings: &Settings) -> Self {
        let persisted = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedLog>(&bytes).ok())
            .unwrap_or_default();
        let mut log = Self {
            records: persisted.records,
            price_moves: persisted.price_moves,
            muted_until: persisted.muted_until.filter(|until| *until > Utc::now()),
            quiet_hours_enabled: settings.quiet_hours_enabled,
            quiet_hours_start: settings.quiet_hours_start.min(23),
            quiet_hours_end: settings.quiet_hours_end.min(23),
            path,
        };
        log.trim();
        log
    }

    pub(crate) fn update_settings(&mut self, settings: &Settings) {
        self.quiet_hours_enabled = settings.quiet_hours_enabled;
        self.quiet_hours_start = settings.quiet_hours_start.min(23);
        self.quiet_hours_end = settings.quiet_hours_end.min(23);
    }

    pub(crate) fn mute_for(&mut self, duration: Duration) {
        self.muted_until = Some(Utc::now() + duration);
        self.persist();
    }

    pub(crate) fn unmute(&mut self) {
        self.muted_until = None;
        self.persist();
    }

    pub(crate) fn muted_until(&self) -> Option<DateTime<Utc>> {
        self.muted_until.filter(|until| *until > Utc::now())
    }

    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.persist();
    }

    pub(crate) fn clear_price_moves(&mut self) {
        self.price_moves.clear();
        self.persist();
    }

    /// Batch insert for feed-wide repricing: every record goes through the
    /// same newest-first insert and trim/cap, but the log is serialized and
    /// written once for the whole batch. Records land newest-first — the last
    /// item ends up at the front of the list.
    pub(crate) fn push_price_moves(&mut self, moves: impl IntoIterator<Item = PriceMoveRecord>) {
        for move_ in moves {
            self.price_moves.push_front(move_);
        }
        while self.price_moves.len() > MAX_PRICE_MOVES {
            self.price_moves.pop_back();
        }
        self.persist();
    }

    pub(crate) fn price_moves(&self) -> impl Iterator<Item = &PriceMoveRecord> {
        self.price_moves.iter()
    }

    /// Newest first.
    pub(crate) fn records(&self) -> impl Iterator<Item = &NotificationRecord> {
        self.records.iter()
    }

    /// For `AfterCooldown` level alerts, avoid evaluating delivery every poll.
    /// A zero-minute cooldown is treated as one minute in this repeat mode so
    /// enabling it can never produce a notification every 30 seconds. Only
    /// *delivered* records gate the repeat: a firing suppressed by quiet
    /// hours or a mute was never seen by the user and must not delay the
    /// real notification by a full cooldown (matching
    /// [`Self::delivered_within_cooldown`]).
    pub(crate) fn ready_for_repeat(&self, alert: &AlertRule) -> bool {
        if alert.rearm != AlertRearm::AfterCooldown {
            return false;
        }
        let minutes = alert.cooldown_minutes.max(1) as i64;
        let cutoff = Utc::now() - Duration::minutes(minutes);
        self.records
            .iter()
            .find(|record| record.alert_id == Some(alert.id) && record.delivered)
            .is_none_or(|record| record.at <= cutoff)
    }

    fn delivered_within_cooldown(&self, alert: &AlertRule, now: DateTime<Utc>) -> bool {
        if alert.cooldown_minutes == 0 {
            return false;
        }
        let cutoff = now - Duration::minutes(alert.cooldown_minutes as i64);
        self.records.iter().any(|record| {
            record.alert_id == Some(alert.id) && record.delivered && record.at > cutoff
        })
    }

    fn quiet_now(&self) -> bool {
        quiet_at(
            self.quiet_hours_enabled,
            self.quiet_hours_start,
            self.quiet_hours_end,
            Local::now().hour() as u8,
        )
    }

    fn record(
        &mut self,
        alert: Option<&AlertRule>,
        kind: NotificationKind,
        model: Option<&str>,
        summary: &str,
        body: &str,
        force_delivery: bool,
    ) -> bool {
        let now = Utc::now();
        let suppressed_reason = if force_delivery {
            None
        } else if self.muted_until().is_some() {
            Some("muted".to_owned())
        } else if self.quiet_now() {
            Some("quiet hours".to_owned())
        } else if alert.is_some_and(|rule| self.delivered_within_cooldown(rule, now)) {
            Some("cooldown".to_owned())
        } else {
            None
        };
        let delivered = suppressed_reason.is_none();
        self.records.push_front(NotificationRecord {
            at: now,
            summary: summary.to_owned(),
            body: body.to_owned(),
            kind,
            model: model.map(str::to_owned),
            alert_id: alert.map(|rule| rule.id),
            delivered,
            suppressed_reason,
        });
        self.trim();
        self.persist();
        delivered
    }

    fn trim(&mut self) {
        while self.records.len() > MAX_RECORDS {
            self.records.pop_back();
        }
    }

    fn persist(&self) {
        let persisted = PersistedLog {
            records: self.records.clone(),
            price_moves: self.price_moves.clone(),
            muted_until: self.muted_until,
        };
        if let Ok(bytes) = serde_json::to_vec(&persisted) {
            let _ = crate::persist::atomic_write(&self.path, &bytes);
        }
    }
}

/// Shared between the UI and fetch worker. The mutex serializes both the
/// in-memory activity timeline and its small JSON persistence writes.
pub(crate) type SharedNotifications = Arc<Mutex<NotificationLog>>;

pub(crate) fn notification_log_path() -> PathBuf {
    crate::persist::data_file("notifications.json")
}

pub(crate) fn notify_alert(
    log: &SharedNotifications,
    alert: &AlertRule,
    kind: NotificationKind,
    model: Option<&str>,
    summary: &str,
    body: &str,
) {
    emit(log, Some(alert), kind, model, summary, body, false);
}

fn emit(
    log: &SharedNotifications,
    alert: Option<&AlertRule>,
    kind: NotificationKind,
    model: Option<&str>,
    summary: &str,
    body: &str,
    force_delivery: bool,
) {
    let delivered = log
        .lock()
        .map(|mut log| log.record(alert, kind, model, summary, body, force_delivery))
        .unwrap_or(true);
    if !delivered {
        return;
    }
    // Windows toasts only display for a registered AUMID (app identity).
    // Startup registers `win_identity::AUMID` per-user, so this is valid from
    // the first seconds of the first run onward; a toast fired before that
    // registration lands simply fails to display (silently, by `let _`),
    // while the alert itself is still recorded and its sound still plays.
    // The builder methods return `&mut Self`, so the notification must be a
    // named binding first — chaining onto the temporary breaks on Linux.
    let mut notification = Notification::new();
    notification.summary(summary).body(body);
    #[cfg(target_os = "windows")]
    notification.app_id(crate::win_identity::AUMID);
    let _ = notification.show();
    if let Some(alert) = alert {
        play_sound(alert.sound);
    }
}

pub(crate) fn play_sound(sound: AlertSound) {
    let bytes: Option<&'static [u8]> = match sound {
        AlertSound::None => None,
        AlertSound::Soft => Some(include_bytes!("../assets/alert-soft.wav")),
        AlertSound::Chime => Some(include_bytes!("../assets/alert-chime.wav")),
        AlertSound::Urgent => Some(include_bytes!("../assets/alert-urgent.wav")),
    };
    let Some(bytes) = bytes else {
        return;
    };
    std::thread::spawn(move || {
        let Ok(mut sink) = rodio::DeviceSinkBuilder::open_default_sink() else {
            return;
        };
        sink.log_on_drop(false);
        let Ok(player) = rodio::play(sink.mixer(), Cursor::new(bytes)) else {
            return;
        };
        player.sleep_until_end();
        // Keep the device sink alive until playback completes.
        std::thread::sleep(StdDuration::from_millis(10));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AlertDirection, AlertMode, AlertSound, BenchmarkMetric, BenchmarkSource, ComparisonMode,
        CostBasis, LiquidityFilter, MoveDirection, PriceMetric, Settings,
    };

    fn temp_log(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("paretowatch-notification-tests")
            .join(format!("{}-{name}.json", std::process::id()))
    }

    fn move_record(model: &str) -> PriceMoveRecord {
        PriceMoveRecord {
            model: model.into(),
            display_name: format!("Model {model}"),
            at: Utc::now(),
            old_blended: 1.0,
            new_blended: 0.9,
            old_input: 1.0,
            new_input: 0.9,
            old_output: 2.0,
            new_output: 1.8,
            source: "test".into(),
        }
    }

    fn cooldown_alert() -> AlertRule {
        AlertRule {
            id: 9,
            model: "model-a".into(),
            mode: AlertMode::Threshold,
            metric: PriceMetric::Blended,
            cost_basis: CostBasis::PerMillion,
            direction: AlertDirection::BelowOrEqual,
            threshold: 0.0,
            enabled: true,
            benchmark_source: BenchmarkSource::CompositeAgentic,
            benchmark_metric: BenchmarkMetric::AgenticCoding,
            comparison_mode: ComparisonMode::BestAvailableAgent,
            common_scaffold: crate::types::default_common_scaffold(),
            liquidity_filter: LiquidityFilter::Any,
            score_threshold: 50.0,
            min_move_pct: 0.0,
            move_direction: MoveDirection::Any,
            discount_threshold_pct: crate::types::default_discount_threshold(),
            healthy_seller_floor: 0,
            cooldown_minutes: 60,
            rearm: AlertRearm::AfterCooldown,
            sound: AlertSound::Chime,
        }
    }

    #[test]
    fn notification_log_roundtrips_and_caps_records() {
        let path = temp_log("records");
        let _ = std::fs::remove_file(&path);
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        for i in 0..MAX_RECORDS + 5 {
            log.record(
                None,
                NotificationKind::Other,
                None,
                &format!("event {i}"),
                "",
                false,
            );
        }
        assert_eq!(log.records().count(), MAX_RECORDS);
        drop(log);
        let reopened = NotificationLog::open_at(path.clone(), &settings);
        assert_eq!(reopened.records().count(), MAX_RECORDS);
        assert_eq!(reopened.records().next().unwrap().summary, "event 2004");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn price_moves_survive_reopen() {
        let path = temp_log("moves");
        let _ = std::fs::remove_file(&path);
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        log.push_price_moves(std::iter::once(PriceMoveRecord {
            model: "model-a".into(),
            display_name: "Model A".into(),
            at: Utc::now(),
            old_blended: 1.0,
            new_blended: 0.8,
            old_input: 1.0,
            new_input: 0.8,
            old_output: 2.0,
            new_output: 1.6,
            source: "test".into(),
        }));
        drop(log);

        let reopened = NotificationLog::open_at(path.clone(), &settings);
        let move_ = reopened.price_moves().next().expect("persisted price move");
        assert_eq!(move_.model, "model-a");
        assert_eq!(move_.display_name, "Model A");
        assert!((move_.percent_delta().unwrap() + 20.0).abs() < 1e-9);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mute_suppresses_delivery_but_keeps_activity() {
        let path = temp_log("mute");
        let _ = std::fs::remove_file(&path);
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        log.mute_for(Duration::hours(1));
        assert!(!log.record(
            None,
            NotificationKind::Other,
            Some("model-a"),
            "suppressed event",
            "body",
            false,
        ));
        let record = log.records().next().expect("suppressed record retained");
        assert!(!record.delivered);
        assert_eq!(record.suppressed_reason.as_deref(), Some("muted"));
        assert_eq!(record.model.as_deref(), Some("model-a"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn overnight_quiet_hours_wrap_midnight() {
        // 22..07 wraps midnight: quiet late at night and early morning,
        // awake all day. The non-wrap window and the disabled/equal cases
        // must stay quiet-hour-free at the same hours.
        for hour in [22u8, 23, 0, 3, 6] {
            assert!(quiet_at(true, 22, 7, hour), "hour {hour}");
        }
        for hour in [7u8, 12, 21] {
            assert!(!quiet_at(true, 22, 7, hour), "hour {hour}");
        }
        for hour in [7u8, 12, 21] {
            assert!(!quiet_at(false, 22, 7, hour), "hour {hour}");
            assert!(!quiet_at(true, 7, 7, hour), "hour {hour}");
        }
        // A non-wrap window covers exactly its hours: 9..17 is quiet from
        // 9:00 through 16:59 and never outside.
        for hour in [9u8, 12, 16] {
            assert!(quiet_at(true, 9, 17, hour), "hour {hour}");
        }
        for hour in [0u8, 8, 17, 23] {
            assert!(!quiet_at(true, 9, 17, hour), "hour {hour}");
        }
    }

    #[test]
    fn batch_price_moves_land_newest_first_and_persist_once() {
        let path = temp_log("batch");
        let _ = std::fs::remove_file(&path);
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        let batch: Vec<PriceMoveRecord> =
            (0..5).map(|i| move_record(&format!("model-{i}"))).collect();
        log.push_price_moves(batch);
        // Newest-first, like the single push: the batch's last record leads.
        let models: Vec<_> = log.price_moves().map(|m| m.model.as_str()).collect();
        assert_eq!(
            models,
            ["model-4", "model-3", "model-2", "model-1", "model-0"]
        );
        drop(log);
        // One persist for the whole batch: the reopened log holds them all.
        let reopened = NotificationLog::open_at(path.clone(), &settings);
        assert_eq!(reopened.price_moves().count(), 5);
        assert_eq!(reopened.price_moves().next().unwrap().model, "model-4");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn batch_price_moves_trim_to_the_cap_in_one_pass() {
        let path = temp_log("batch-cap");
        let _ = std::fs::remove_file(&path);
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        let batch: Vec<PriceMoveRecord> = (0..MAX_PRICE_MOVES + 5)
            .map(|i| move_record(&format!("model-{i}")))
            .collect();
        log.push_price_moves(batch);
        assert_eq!(log.price_moves().count(), MAX_PRICE_MOVES);
        assert_eq!(
            log.price_moves().next().unwrap().model,
            format!("model-{}", MAX_PRICE_MOVES + 4)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ready_for_repeat_gates_only_on_delivered_records() {
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(
            std::path::Path::new("unused-ready-repeat.json").into(),
            &settings,
        );
        let alert = cooldown_alert();
        // Nothing on record: the repeat is immediately ready.
        assert!(log.ready_for_repeat(&alert));
        // A firing suppressed by quiet hours must not reset the repeat clock.
        log.records.push_front(NotificationRecord {
            at: Utc::now() - Duration::minutes(5),
            alert_id: Some(alert.id),
            delivered: false,
            ..Default::default()
        });
        assert!(log.ready_for_repeat(&alert));
        // A delivered record inside the cooldown does gate it...
        log.records.push_front(NotificationRecord {
            at: Utc::now() - Duration::minutes(5),
            alert_id: Some(alert.id),
            delivered: true,
            ..Default::default()
        });
        assert!(!log.ready_for_repeat(&alert));
        // ...until it ages past the cooldown.
        log.records[0].at = Utc::now() - Duration::minutes(61);
        assert!(log.ready_for_repeat(&alert));
    }

    #[test]
    fn persist_fallback_archives_instead_of_deleting_first() {
        // Force the direct tmp→log rename to fail: a directory occupies the
        // log path, so the rename is refused the same way Windows refuses
        // rename-over-existing. The fallback must move the live node aside
        // under a timestamped archive and retry — never remove it first,
        // which would lose the history if the process died in between.
        let dir = std::env::temp_dir()
            .join("paretowatch-notification-tests")
            .join(format!("{}-swap", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("notifications.json");
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        let settings = Settings::default();
        let mut log = NotificationLog::open_at(path.clone(), &settings);
        log.push_price_moves(std::iter::once(move_record("model-a")));
        // The persisted log is in place and the old node was archived aside.
        assert!(path.is_file());
        let archived: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("notifications.json.old-"))
            })
            .collect();
        assert_eq!(archived.len(), 1);
        drop(log);
        let reopened = NotificationLog::open_at(path.clone(), &settings);
        assert_eq!(reopened.price_moves().count(), 1);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(dir);
    }
}
