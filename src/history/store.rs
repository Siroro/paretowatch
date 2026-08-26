//! Append-only, event-sourced history log.
//!
//! Space efficiency contract: a poll that changes nothing writes zero bytes.
//! The log stores *change events*, not samples:
//! - model identity strings are written once (`Added`), later frames reference
//!   a u16 id;
//! - timestamps are u32 varint deltas from the previous frame in the file;
//! - prices are quantized to 0.001 $/M tokens and stored as zigzag deltas of
//!   the quantized value, so sub-quantum wobble never writes (this is the
//!   change-detection epsilon);
//! - composite scores are quantized to 0.1 and zigzag-deltaed the same way;
//! - daily market telemetry (requests/volume) is summarized once per model per
//!   UTC day instead of per poll.
//!
//! Frames are `postcard`-encoded back to back with no length prefix (postcard
//! is self-delimiting per type). A crash mid-write can only corrupt the tail
//! frame; replay stops at the first undecodable frame and truncates the file
//! back to the last good offset.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

const MAGIC: [u8; 4] = *b"PWH1";
const FORMAT_VERSION: u8 = 1;

/// Price resolution: 0.001 $/M tokens. Also the minimum price movement that
/// registers as a change.
pub(crate) const PRICE_QUANT: f64 = 0.001;
/// Composite score resolution: 0.1 points on the 0..100 scale.
pub(crate) const SCORE_QUANT: f64 = 0.1;

pub(crate) fn quantize_price(v: f64) -> i32 {
    (v / PRICE_QUANT).round() as i32
}

pub(crate) fn dequantize_price(q: i32) -> f64 {
    q as f64 * PRICE_QUANT
}

pub(crate) fn quantize_score(v: f64) -> i32 {
    (v / SCORE_QUANT).round() as i32
}

pub(crate) fn dequantize_score(q: i32) -> f64 {
    q as f64 * SCORE_QUANT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PriceField {
    Input,
    Output,
    CacheRead,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum EventKind {
    /// First time a model is ever seen. Owns the identity strings so every
    /// later frame only pays a u16 id.
    Added {
        slug: String,
        display: String,
        creator: String,
    },
    /// Zigzag delta (postcard varint) of the quantized price.
    Price { field: PriceField, delta: i32 },
    /// Composite scores. `None` fields are unchanged; boards usually move
    /// both flavors at once, so one frame covers it.
    Composite {
        capability: Option<i32>,
        deployment: Option<i32>,
    },
    /// Once-per-UTC-day summary of market telemetry.
    Telemetry { requests: u64, volume_cents: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Frame {
    pub id: u16,
    /// Seconds since the previously written frame (0 for same-tick events).
    pub dt: u32,
    pub kind: EventKind,
}

pub(crate) struct Replay {
    /// Decoded frames with absolute unix timestamps (seconds).
    pub frames: Vec<(i64, Frame)>,
    pub file_bytes: u64,
}

pub(crate) struct EventStore {
    file: Option<File>,
    /// Unix seconds of the last written frame; used to compute `dt`.
    last_ts: i64,
    pub file_bytes: u64,
    pub frame_count: u64,
    pub degraded_reason: Option<String>,
}

impl EventStore {
    /// Opens (or creates) the log and replays it. A corrupt/unknown-format
    /// file is moved aside rather than lost; an unwritable location degrades
    /// to memory-only operation instead of taking the app down.
    pub fn open(path: &Path) -> (EventStore, Replay) {
        let (mut replay, note) = match Self::read_all(path) {
            Ok(r) => (r, None),
            Err(why) => (
                Replay {
                    frames: Vec::new(),
                    file_bytes: 0,
                },
                Some(why),
            ),
        };
        replay.frames.shrink_to_fit();
        let mut store = EventStore {
            file: None,
            last_ts: replay.frames.last().map(|(ts, _)| *ts).unwrap_or(0),
            file_bytes: replay.file_bytes,
            frame_count: replay.frames.len() as u64,
            degraded_reason: note,
        };
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => store.file = Some(file),
            Err(err) => store.degraded_reason = Some(format!("history not persisted: {err}")),
        }
        (store, replay)
    }

    fn read_all(path: &Path) -> Result<Replay, String> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let mut file = File::create(path).map_err(|e| e.to_string())?;
                file.write_all(&MAGIC)
                    .and_then(|_| file.write_all(&[FORMAT_VERSION]))
                    .map_err(|e| e.to_string())?;
                return Ok(Replay {
                    frames: Vec::new(),
                    file_bytes: 5,
                });
            }
            Err(err) => return Err(err.to_string()),
        };
        if bytes.len() < 5 || bytes[..4] != MAGIC || bytes[4] != FORMAT_VERSION {
            let aside = path.with_extension("bin.old");
            let _ = fs::rename(path, &aside);
            let mut file = File::create(path).map_err(|e| e.to_string())?;
            file.write_all(&MAGIC)
                .and_then(|_| file.write_all(&[FORMAT_VERSION]))
                .map_err(|e| e.to_string())?;
            return Ok(Replay {
                frames: Vec::new(),
                file_bytes: 5,
            });
        }

        let mut frames = Vec::new();
        let mut ts: i64 = 0;
        let mut rest = &bytes[5..];
        let mut good = 5usize;
        let mut discarded = 0usize;
        while !rest.is_empty() {
            match postcard::take_from_bytes::<Frame>(rest) {
                Ok((frame, remaining)) => {
                    ts += frame.dt as i64;
                    good += rest.len() - remaining.len();
                    rest = remaining;
                    frames.push((ts, frame));
                }
                Err(_) => {
                    // Truncated/corrupt tail (crash mid-write). Drop it and
                    // shrink the file so the next append starts clean.
                    discarded = rest.len();
                    break;
                }
            }
        }
        let file_bytes = if discarded > 0 {
            good as u64
        } else {
            fs::metadata(path).map(|m| m.len()).unwrap_or(good as u64)
        };
        if discarded > 0 {
            if let Ok(file) = OpenOptions::new().write(true).open(path) {
                let _ = file.set_len(good as u64);
            }
        }
        Ok(Replay { frames, file_bytes })
    }

    /// Appends frames (absolute timestamps), flushing once. Returns bytes
    /// written; a write failure switches the store to degraded mode rather
    /// than aborting the poll.
    pub fn append(&mut self, events: &[(i64, Frame)]) -> usize {
        let Some(file) = self.file.as_mut() else {
            return 0;
        };
        if events.is_empty() {
            return 0;
        }
        let mut buf = Vec::new();
        let mut last = self.last_ts;
        for (ts, frame) in events {
            let dt = (*ts - last).clamp(0, u32::MAX as i64) as u32;
            let mut encoded = postcard::to_allocvec(&Frame {
                dt,
                ..frame.clone()
            })
            .expect("history frame serialization cannot fail");
            buf.append(&mut encoded);
            last = *ts;
        }
        match file.write_all(&buf).and_then(|_| file.flush()) {
            Ok(()) => {
                self.last_ts = last;
                self.file_bytes += buf.len() as u64;
                self.frame_count += events.len() as u64;
                buf.len()
            }
            Err(err) => {
                self.degraded_reason = Some(format!("history write failed: {err}"));
                0
            }
        }
    }

    pub fn degraded_reason(&self) -> Option<&str> {
        self.degraded_reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("paretowatch-history-tests")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir.join("history.bin")
    }

    fn added_frame(id: u16, slug: &str) -> Frame {
        Frame {
            id,
            dt: 0,
            kind: EventKind::Added {
                slug: slug.into(),
                display: slug.into(),
                creator: "openai".into(),
            },
        }
    }

    #[test]
    fn frames_roundtrip_through_disk() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        let base = 1_700_000_000i64;
        {
            let mut store = EventStore::open(&path).0;
            assert!(store.degraded_reason().is_none());
            store.append(&[
                (base, added_frame(0, "openai/gpt-x")),
                (
                    base + 60,
                    Frame {
                        id: 0,
                        dt: 0,
                        kind: EventKind::Price {
                            field: PriceField::Input,
                            delta: quantize_price(2.5),
                        },
                    },
                ),
                (
                    base + 120,
                    Frame {
                        id: 0,
                        dt: 0,
                        kind: EventKind::Composite {
                            capability: Some(quantize_score(77.7)),
                            deployment: None,
                        },
                    },
                ),
            ]);
        }
        {
            let (store, replay) = EventStore::open(&path);
            assert_eq!(replay.frames.len(), 3);
            assert_eq!(replay.frames[0].0, base);
            assert_eq!(replay.frames[2].0, base + 120);
            assert_eq!(
                replay.frames[1].1.kind,
                EventKind::Price {
                    field: PriceField::Input,
                    delta: quantize_price(2.5)
                }
            );
            assert_eq!(store.frame_count, 3);
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn zero_events_write_zero_bytes() {
        let path = temp_path("noop");
        let _ = fs::remove_file(&path);
        let mut store = EventStore::open(&path).0;
        let before = store.file_bytes;
        assert_eq!(store.append(&[]), 0);
        assert_eq!(store.file_bytes, before);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn truncated_tail_is_discarded_and_file_shrunk() {
        let path = temp_path("truncated");
        let _ = fs::remove_file(&path);
        {
            let mut store = EventStore::open(&path).0;
            store.append(&[(1, added_frame(0, "a/b"))]);
            drop(store);
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            // Simulate a crash mid-frame: half a valid frame's bytes.
            let half = postcard::to_allocvec(&added_frame(1, "c/d")).unwrap();
            file.write_all(&half[..half.len() / 2]).unwrap();
            drop(file);
        }
        let corrupted_len = fs::metadata(&path).unwrap().len();
        {
            let (_, replay) = EventStore::open(&path);
            assert_eq!(replay.frames.len(), 1);
            assert_eq!(fs::metadata(&path).unwrap().len(), replay.file_bytes);
            assert!(replay.file_bytes < corrupted_len);
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn quantization_is_stable_and_lossless_for_repr() {
        assert_eq!(quantize_price(2.5004), 2500);
        assert_eq!(dequantize_price(quantize_price(2.5)), 2.5);
        assert_eq!(dequantize_score(quantize_score(77.74)), 77.7);
        // Sub-epsilon change quantizes identically → no event.
        assert_eq!(quantize_price(1.0001), quantize_price(1.0004));
    }

    #[test]
    fn bad_magic_archives_and_restarts() {
        let path = temp_path("badmagic");
        let _ = fs::remove_file(&path);
        fs::write(&path, b"JUNKJUNKJUNK").unwrap();
        let (store, replay) = EventStore::open(&path);
        assert_eq!(replay.frames.len(), 0);
        assert_eq!(store.frame_count, 0);
        assert!(path.with_extension("bin.old").exists() || fs::read(&path).unwrap()[..4] == MAGIC);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("bin.old"));
    }
}
