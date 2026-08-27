//! Desktop-notification fan-out plus the in-app history log.
//!
//! Every alert path (worker-thread price rules, UI-thread semantic rules,
//! feed-wide listing rules) routes through [`notify`], so a toast always has
//! a matching record the Alerts tab can replay. The log is session-only and
//! capped; long-term history lives in `crate::history`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use notify_rust::Notification;

#[derive(Debug, Clone)]
pub(crate) struct NotificationRecord {
    pub(crate) at: DateTime<Utc>,
    pub(crate) summary: String,
    pub(crate) body: String,
}

const MAX_RECORDS: usize = 200;

#[derive(Default)]
pub(crate) struct NotificationLog {
    records: VecDeque<NotificationRecord>,
}

impl NotificationLog {
    fn push(&mut self, summary: String, body: String) {
        self.records.push_front(NotificationRecord {
            at: Utc::now(),
            summary,
            body,
        });
        while self.records.len() > MAX_RECORDS {
            self.records.pop_back();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.records.clear();
    }

    /// Newest first.
    pub(crate) fn records(&self) -> impl Iterator<Item = &NotificationRecord> {
        self.records.iter()
    }
}

/// Shared between the UI and the fetch worker: the worker fires price alerts
/// off-thread and appends here without a channel hop.
pub(crate) type SharedNotifications = Arc<Mutex<NotificationLog>>;

/// Show the desktop toast and record it in the shared history log.
pub(crate) fn notify(log: &SharedNotifications, summary: &str, body: &str) {
    let _ = Notification::new().summary(summary).body(body).show();
    if let Ok(mut log) = log.lock() {
        log.push(summary.to_owned(), body.to_owned());
    }
}
