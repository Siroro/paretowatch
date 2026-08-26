//! Turns live snapshots into change events and rebuilds queryable series.
//!
//! Recording runs after every successful price poll. It diffs the new
//! snapshot against the last recorded state (quantized values, so sub-epsilon
//! wobble is invisible) and only then appends price and telemetry events.
//!
//! Composite scores are live-derived by the benchmark layer and are
//! intentionally not part of historical storage.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;

use super::store::{EventKind, EventStore, Frame, PriceField, dequantize_price, quantize_price};
use crate::types::{PriceSnapshot, Quote};

#[derive(Debug, Clone, Default)]
pub(crate) struct ModelSeries {
    pub slug: String,
    pub display: String,
    pub creator: String,
    pub added_ts: f64,
    pub input: Vec<(f64, f64)>,
    pub output: Vec<(f64, f64)>,
    pub cache_read: Vec<(f64, f64)>,
    /// (ts, requests_24h, volume_24h in dollars), once per UTC day.
    pub telemetry: Vec<(f64, u64, f64)>,
}

#[derive(Debug, Default)]
struct CurrentState {
    input_q: i32,
    output_q: i32,
    cache_q: Option<i32>,
    telemetry_day: u32,
}

pub(crate) struct HistoryTracker {
    store: EventStore,
    ids: HashMap<String, u16>,
    series: HashMap<u16, ModelSeries>,
    current: HashMap<u16, CurrentState>,
    next_id: u16,
}

const MAX_TRACKED_MODELS: u16 = 60_000;

impl HistoryTracker {
    pub(crate) fn open(path: &Path) -> HistoryTracker {
        let (store, replay) = EventStore::open(path);
        let mut tracker = HistoryTracker {
            store,
            ids: HashMap::new(),
            series: HashMap::new(),
            current: HashMap::new(),
            next_id: 0,
        };
        for (ts, frame) in replay.frames {
            tracker.apply_event(ts, frame);
        }
        tracker.next_id = tracker
            .ids
            .values()
            .copied()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        tracker
    }

    /// Records a poll. Cheap when nothing changed: quantized diff finds no
    /// deltas and the store write is skipped entirely.
    pub(crate) fn record(&mut self, snapshot: &PriceSnapshot, now: DateTime<Utc>) {
        let ts = now.timestamp();
        let mut events: Vec<(i64, Frame)> = Vec::new();

        for q in &snapshot.quotes {
            let Some(id) = self.ensure_added(&mut events, q, ts) else {
                continue;
            };
            self.diff_price(&mut events, id, ts, PriceField::Input, q.input);
            self.diff_price(&mut events, id, ts, PriceField::Output, q.output);
            if let Some(cache_read) = q.cache_read {
                self.diff_price(&mut events, id, ts, PriceField::CacheRead, cache_read);
            }
            self.diff_telemetry(&mut events, id, ts, q);
        }
        if !events.is_empty() {
            self.store.append(&events);
        }
    }

    fn ensure_added(&mut self, events: &mut Vec<(i64, Frame)>, q: &Quote, ts: i64) -> Option<u16> {
        if let Some(&id) = self.ids.get(&q.model) {
            return Some(id);
        }
        if self.next_id >= MAX_TRACKED_MODELS {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame {
            id,
            dt: 0,
            kind: EventKind::Added {
                slug: q.model.clone(),
                display: q.display_name.clone(),
                creator: q.creator.clone(),
            },
        };
        self.apply_event(ts, frame.clone());
        events.push((ts, frame));
        Some(id)
    }

    fn diff_price(
        &mut self,
        events: &mut Vec<(i64, Frame)>,
        id: u16,
        ts: i64,
        field: PriceField,
        value: f64,
    ) {
        let new_q = quantize_price(value);
        let Some(st) = self.current.get(&id) else {
            return;
        };
        let old_q = match field {
            PriceField::Input => st.input_q,
            PriceField::Output => st.output_q,
            PriceField::CacheRead => st.cache_q.unwrap_or(0),
        };
        if new_q == old_q {
            return;
        }
        let frame = Frame {
            id,
            dt: 0,
            kind: EventKind::Price {
                field,
                delta: new_q - old_q,
            },
        };
        self.apply_event(ts, frame.clone());
        events.push((ts, frame));
    }

    fn diff_telemetry(&mut self, events: &mut Vec<(i64, Frame)>, id: u16, ts: i64, q: &Quote) {
        if q.requests_24h.is_none() && q.volume_24h.is_none() {
            return;
        }
        let day = (ts / 86_400) as u32;
        let Some(st) = self.current.get(&id) else {
            return;
        };
        if st.telemetry_day == day {
            return;
        }
        let frame = Frame {
            id,
            dt: 0,
            kind: EventKind::Telemetry {
                requests: q.requests_24h.unwrap_or(0),
                volume_cents: q
                    .volume_24h
                    .map(|v| (v * 100.0).round() as u64)
                    .unwrap_or(0),
            },
        };
        self.apply_event(ts, frame.clone());
        events.push((ts, frame));
    }

    /// Single source of truth for state mutation: used both while recording
    /// (so later diffs in the same poll see earlier events) and while
    /// replaying the log at startup.
    fn apply_event(&mut self, ts: i64, frame: Frame) {
        let id = frame.id;
        match frame.kind {
            EventKind::Added {
                slug,
                display,
                creator,
            } => {
                self.ids.insert(slug.clone(), id);
                self.series.insert(
                    id,
                    ModelSeries {
                        slug,
                        display,
                        creator,
                        added_ts: ts as f64,
                        ..ModelSeries::default()
                    },
                );
                self.current.entry(id).or_default();
            }
            EventKind::Price { field, delta } => {
                let Some(st) = self.current.get_mut(&id) else {
                    return;
                };
                let new_q = match field {
                    PriceField::Input => {
                        st.input_q += delta;
                        st.input_q
                    }
                    PriceField::Output => {
                        st.output_q += delta;
                        st.output_q
                    }
                    PriceField::CacheRead => {
                        let new_q = st.cache_q.unwrap_or(0) + delta;
                        st.cache_q = Some(new_q);
                        new_q
                    }
                };
                let dequant = dequantize_price(new_q);
                if let Some(series) = self.series.get_mut(&id) {
                    match field {
                        PriceField::Input => series.input.push((ts as f64, dequant)),
                        PriceField::Output => series.output.push((ts as f64, dequant)),
                        PriceField::CacheRead => series.cache_read.push((ts as f64, dequant)),
                    }
                }
            }
            // Composite events are retained in the wire format for compatibility
            // with existing logs, but composites are no longer historical data.
            EventKind::Composite { .. } => {}
            EventKind::Telemetry {
                requests,
                volume_cents,
            } => {
                let day = (ts / 86_400) as u32;
                if let Some(st) = self.current.get_mut(&id) {
                    st.telemetry_day = day;
                }
                if let Some(series) = self.series.get_mut(&id) {
                    series
                        .telemetry
                        .push((ts as f64, requests, volume_cents as f64 / 100.0));
                }
            }
        }
    }

    pub(crate) fn series(&self, slug: &str) -> Option<&ModelSeries> {
        self.ids.get(slug).and_then(|id| self.series.get(id))
    }

    /// Borrowed iteration over every tracked series, so UI code can build
    /// catalogs without cloning each slug first.
    pub(crate) fn series_list(&self) -> impl Iterator<Item = &ModelSeries> {
        self.series.values()
    }

    pub(crate) fn model_count(&self) -> usize {
        self.series.len()
    }

    pub(crate) fn store_stats(&self) -> (u64, u64) {
        (self.store.file_bytes, self.store.frame_count)
    }

    pub(crate) fn degraded_reason(&self) -> Option<&str> {
        self.store.degraded_reason()
    }
}

/// Merges the three price timelines into a blended series using carry-forward
/// values (cache falls back to input at each instant, matching
/// `crate::blended_price`). Change timestamps are deduplicated.
pub(crate) fn blended_series(
    series: &ModelSeries,
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
) -> Vec<(f64, f64)> {
    let mut times: Vec<f64> = Vec::new();
    if series.input.first().map(|(ts, _)| *ts).unwrap_or(f64::MAX) == series.added_ts
        || series.output.first().map(|(ts, _)| *ts).unwrap_or(f64::MAX) == series.added_ts
        || series
            .cache_read
            .first()
            .map(|(ts, _)| *ts)
            .unwrap_or(f64::MAX)
            == series.added_ts
    {
        times.push(series.added_ts);
    }
    times.extend(
        series
            .input
            .iter()
            .chain(&series.output)
            .chain(&series.cache_read)
            .map(|(ts, _)| *ts),
    );
    times.sort_by(|a, b| a.total_cmp(b));
    times.dedup();

    let mut out = Vec::with_capacity(times.len());
    for ts in times {
        // Blending needs both cash legs; cache alone falling back to input
        // matches `blended_price`, but input/output gaps just mean the
        // series has not started yet at that timestamp.
        let (Some(input), Some(output)) = (
            carry_forward(&series.input, ts),
            carry_forward(&series.output, ts),
        ) else {
            continue;
        };
        let cache = carry_forward(&series.cache_read, ts);
        out.push((
            ts,
            crate::types::blended_price(
                input,
                cache,
                output,
                input_weight,
                cache_read_weight,
                output_weight,
            ),
        ));
    }
    out
}

/// Last value at or before `ts` (series are chronological change points).
pub(crate) fn carry_forward(series: &[(f64, f64)], ts: f64) -> Option<f64> {
    match series.partition_point(|(t, _)| *t <= ts) {
        0 => None,
        i => Some(series[i - 1].1),
    }
}

/// Converts change points into step polyline points, extending the final
/// plateau to `until` so the chart reaches the right edge.
pub(crate) fn step_points(series: &[(f64, f64)], until: f64) -> Vec<[f64; 2]> {
    if series.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(series.len() * 2);
    for window in series.windows(2) {
        let (t0, v0) = window[0];
        let (t1, _) = window[1];
        out.push([t0, v0]);
        if t1 > t0 {
            out.push([t1, v0]);
        }
    }
    let (tl, vl) = *series.last().unwrap();
    out.push([tl, vl]);
    if until > tl {
        out.push([until, vl]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::store::quantize_price;
    use std::fs;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("paretowatch-history-tests")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir.join("history.bin")
    }

    fn quote(model: &str, input: f64, output: f64) -> Quote {
        Quote {
            model: model.into(),
            display_name: model.to_uppercase(),
            creator: "openai".into(),
            provider: "surplus".into(),
            input,
            output,
            cache_read: None,
            seller_count: None,
            healthy_seller_count: None,
            provider_trusted: None,
            requests_24h: Some(1_000),
            volume_24h: Some(12.34),
            discount_pct: None,
            discount_direction: None,
            free_offer_listed: false,
            market_options: Vec::new(),
            live_market: false,
            vision: false,
        }
    }

    fn snapshot(quotes: Vec<Quote>) -> PriceSnapshot {
        PriceSnapshot {
            quotes,
            fetched_at: Utc::now(),
            comparison_updated_at: None,
            market_overlay_count: 0,
            market_error: None,
            base_source: "test".into(),
        }
    }

    fn now_plus(secs: i64) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(secs)
    }

    #[test]
    fn record_persists_and_reloads_identically() {
        let path = temp_path("reload");
        let _ = fs::remove_file(&path);
        let base = Utc::now();
        {
            let mut t = HistoryTracker::open(&path);
            t.record(&snapshot(vec![quote("openai/gpt-x", 2.5, 10.0)]), base);
            t.record(
                &snapshot(vec![quote("openai/gpt-x", 2.0, 10.0)]),
                base + chrono::Duration::seconds(3600),
            );
        }
        let t = HistoryTracker::open(&path);
        let s = t.series("openai/gpt-x").expect("model survived restart");
        assert_eq!(s.input.len(), 2);
        assert_eq!(s.input[0].1, 2.5);
        assert_eq!(s.input[1].1, 2.0);
        assert_eq!(s.output.len(), 1);
        assert_eq!(s.telemetry.len(), 1);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unchanged_poll_writes_zero_bytes() {
        let path = temp_path("noop");
        let _ = fs::remove_file(&path);
        let mut t = HistoryTracker::open(&path);
        let snap = snapshot(vec![quote("openai/gpt-x", 2.5, 10.0)]);
        t.record(&snap, now_plus(0));
        let (bytes, frames) = t.store_stats();
        t.record(&snap, now_plus(60));
        let (bytes2, frames2) = t.store_stats();
        assert_eq!((bytes, frames), (bytes2, frames2));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sub_epsilon_wobble_writes_nothing() {
        let path = temp_path("epsilon");
        let _ = fs::remove_file(&path);
        let mut t = HistoryTracker::open(&path);
        t.record(&snapshot(vec![quote("a/b", 1.0, 4.0)]), now_plus(0));
        let (_, frames) = t.store_stats();
        // 0.0004 < PRICE_QUANT/2 wiggle quantizes identically.
        t.record(&snapshot(vec![quote("a/b", 1.0004, 4.0)]), now_plus(60));
        let (_, frames2) = t.store_stats();
        assert_eq!(frames, frames2);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn telemetry_recorded_once_per_utc_day() {
        let path = temp_path("telemetry");
        let _ = fs::remove_file(&path);
        let mut t = HistoryTracker::open(&path);
        let snap = snapshot(vec![quote("a/b", 1.0, 4.0)]);
        t.record(&snap, now_plus(0));
        t.record(&snap, now_plus(3600));
        assert_eq!(t.series("a/b").unwrap().telemetry.len(), 1);
        t.record(&snap, now_plus(90_000));
        assert_eq!(t.series("a/b").unwrap().telemetry.len(), 2);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn blended_series_carries_forward_and_falls_back() {
        let mut s = ModelSeries {
            slug: "a".into(),
            display: "a".into(),
            creator: "".into(),
            added_ts: 0.0,
            ..Default::default()
        };
        s.input = vec![(0.0, 2.0), (100.0, 4.0)];
        s.output = vec![(50.0, 10.0)];
        s.cache_read = vec![]; // cache falls back to input
        let blended = blended_series(&s, 25.0, 50.0, 25.0);
        // t=0: (2*25 + 2*50 + 10*? ) — output unknown at t=0, series only has
        // change points, so before t=50 output is absent; blended then only
        // starts once all legs exist. Verify the fully-known points:
        let at = |t: f64| carry_forward(&blended, t);
        let b100 = at(100.0).expect("value at t=100");
        assert!((b100 - (4.0 * 25.0 + 4.0 * 50.0 + 10.0 * 25.0) / 100.0).abs() < 1e-9);
    }

    #[test]
    fn step_points_duplicate_x_at_changes_and_extend() {
        let pts = step_points(&[(0.0, 1.0), (10.0, 2.0), (20.0, 3.0)], 35.0);
        assert_eq!(
            pts,
            vec![
                [0.0, 1.0],
                [10.0, 1.0],
                [10.0, 2.0],
                [20.0, 2.0],
                [20.0, 3.0],
                [35.0, 3.0]
            ]
        );
        assert_eq!(step_points(&[], 35.0), Vec::<[f64; 2]>::new());
    }

    #[test]
    fn price_deltas_round_trip_through_quantization() {
        let path = temp_path("deltas");
        let _ = fs::remove_file(&path);
        let mut t = HistoryTracker::open(&path);
        t.record(&snapshot(vec![quote("a/b", 3.0, 12.0)]), now_plus(0));
        t.record(&snapshot(vec![quote("a/b", 1.5, 12.0)]), now_plus(60));
        let s = t.series("a/b").unwrap();
        assert_eq!(s.input.len(), 2);
        assert_eq!(s.input[1].1, 1.5);
        // And a fresh reload decodes the same deltas.
        let t2 = HistoryTracker::open(&path);
        assert_eq!(t2.series("a/b").unwrap().input, s.input);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn quantize_helpers_match_store() {
        assert_eq!(super::quantize_price(2.5), quantize_price(2.5));
    }

    /// Ignored network-ish diagnostic: replays the developer's real history
    /// log and prints what survived a restart. Run with
    /// `cargo test live_history -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_history_log_replays() {
        let path = crate::history::history_log_path();
        let t = HistoryTracker::open(&path);
        let (bytes, frames) = t.store_stats();
        println!(
            "{} models · {} frames · {} bytes",
            t.model_count(),
            frames,
            bytes
        );
        let shown = t.series_list().take(3).cloned().collect::<Vec<_>>();
        for s in &shown {
            println!(
                "{}: input {} events (last {:?})",
                s.display,
                s.input.len(),
                s.input.last().map(|(_, v)| *v),
            );
        }
        assert!(t.model_count() > 50, "expected the live catalog in the log");
        assert!(bytes > 0);
    }
}
