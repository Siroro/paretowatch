//! History tab UI: multi-metric, multi-model time charts over the persisted
//! event log. All rendering lives here so main.rs keeps zero chart code for
//! history.

use chrono::{DateTime, Utc};
use eframe::egui;
use egui_plot::{Legend, Line, Plot, PlotPoints, Points, VLine};

use super::HistoryTracker;
use super::track::{ModelSeries, blended_series};
use crate::types::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryMetric {
    Blended,
    Input,
    Output,
    CacheRead,
    Capability,
    Deployment,
    VolumeDaily,
}

impl HistoryMetric {
    fn label(self) -> &'static str {
        match self {
            Self::Blended => "Blended price",
            Self::Input => "Input price",
            Self::Output => "Output price",
            Self::CacheRead => "Cache read",
            Self::Capability => "Capability composite",
            Self::Deployment => "Deployment composite",
            Self::VolumeDaily => "Daily volume",
        }
    }

    fn is_price(self) -> bool {
        matches!(
            self,
            Self::Blended | Self::Input | Self::Output | Self::CacheRead
        )
    }

    fn y_label(self) -> String {
        match self {
            Self::Blended | Self::Input | Self::Output => "$ / 1M tokens".into(),
            Self::CacheRead => "cache $ / 1M tokens".into(),
            Self::Capability | Self::Deployment => "composite score (0-100)".into(),
            Self::VolumeDaily => "24h volume ($)".into(),
        }
    }

    fn format_value(self, v: f64) -> String {
        match self {
            Self::Blended | Self::Input | Self::Output | Self::CacheRead => {
                crate::format::format_price_tick(v)
            }
            Self::Capability | Self::Deployment => format!("{v:.1}"),
            Self::VolumeDaily => crate::format::format_compact_number(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryRange {
    Day1,
    Day7,
    Day30,
    All,
}

impl HistoryRange {
    fn label(self) -> &'static str {
        match self {
            Self::Day1 => "24h",
            Self::Day7 => "7d",
            Self::Day30 => "30d",
            Self::All => "All",
        }
    }

    fn seconds(self) -> Option<i64> {
        match self {
            Self::Day1 => Some(86_400),
            Self::Day7 => Some(7 * 86_400),
            Self::Day30 => Some(30 * 86_400),
            Self::All => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct HistoryUiState {
    /// Model slugs, primary first. Unlimited comparisons; colors cycle.
    pub selected: Vec<String>,
    pub metric: HistoryMetric,
    pub range: HistoryRange,
    pub log_y: bool,
    pub normalize: bool,
    pub reset_zoom: bool,
    /// False until the persisted selection has been restored from disk.
    restored: bool,
}

impl Default for HistoryUiState {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            metric: HistoryMetric::Blended,
            range: HistoryRange::All,
            log_y: true,
            normalize: false,
            // Auto bounds on a flat/single-point series degenerate to a huge
            // default window; the first frame must run the sane-bounds pass.
            reset_zoom: true,
            restored: false,
        }
    }
}

const COMPARE_COLORS: [egui::Color32; 6] = [
    egui::Color32::from_rgb(251, 191, 36),  // amber
    egui::Color32::from_rgb(192, 132, 252), // violet
    egui::Color32::from_rgb(74, 222, 128),  // green
    egui::Color32::from_rgb(96, 165, 250),  // blue
    egui::Color32::from_rgb(248, 113, 113), // red
    egui::Color32::from_rgb(45, 212, 191),  // teal
];

struct PreparedSeries {
    name: String,
    color: egui::Color32,
    /// Raw change points within (or anchoring) the visible window.
    points: Vec<(f64, f64)>,
    /// Step polyline derived from `points`, extended to `now`.
    steps: Vec<[f64; 2]>,
    anchor: f64,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    settings: &Settings,
    snapshot: Option<&crate::types::PriceSnapshot>,
    tracker: &HistoryTracker,
    state: &mut HistoryUiState,
) {
    let now = Utc::now();

    // Restore the persisted comparison selection once, before the catalog is
    // consulted: the log replays every model ever seen at startup, so saved
    // slugs survive the catalog prune below even before feeds deliver.
    if !state.restored {
        let saved = load_selection_from(&super::ui_state_path());
        if !saved.is_empty() {
            state.selected = saved;
        }
        state.restored = true;
    }

    // Model catalog for pickers: everything the log has ever seen, plus
    // anything live in the current snapshot that history has not added yet.
    // Borrowed slugs plus a seen-set: no per-frame string clones or O(n·m)
    // scans while the History tab repaints.
    let mut models: Vec<(&str, &str)> = Vec::new();
    let mut known: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for s in tracker.series_list() {
        known.insert(s.slug.as_str());
        models.push((s.slug.as_str(), s.display.as_str()));
    }
    if let Some(snap) = snapshot {
        for q in &snap.quotes {
            if known.insert(q.model.as_str()) {
                models.push((q.model.as_str(), q.display_name.as_str()));
            }
        }
    }
    models.sort_by_cached_key(|(_, display)| display.to_lowercase());
    if models.is_empty() {
        ui.label("History will start recording with the first data poll.");
        return;
    }

    // Retain first, then re-default: a persisted selection whose models have
    // all vanished (e.g. degraded history plus a restart) must never leave
    // `selected` empty — `controls` indexes `[0]`.
    state
        .selected
        .retain(|slug| models.iter().any(|(m, _)| m == slug));
    if state.selected.is_empty() {
        state.selected.push(models[0].0.to_owned());
    }

    let selected_before_controls = state.selected.clone();
    controls(ui, &models, state);
    if state.selected != selected_before_controls {
        save_selection_to(&super::ui_state_path(), &state.selected);
    }
    ui.add_space(6.0);

    let display_of = |slug: &str| {
        models
            .iter()
            .find(|(m, _)| *m == slug)
            .map(|(_, d)| (*d).to_owned())
            .unwrap_or_else(|| slug.to_owned())
    };

    let mut prepared: Vec<PreparedSeries> = Vec::new();
    for (i, slug) in state.selected.iter().enumerate() {
        let Some(series) = tracker.series(slug) else {
            continue;
        };
        let Some(points) = windowed_metric_series(series, state.metric, settings, state.range, now)
        else {
            continue;
        };
        let anchor = points[0].1;
        let mut shown = points;
        if state.normalize && anchor.abs() > f64::EPSILON {
            for (_, v) in &mut shown {
                *v = *v / anchor * 100.0;
            }
        }
        let steps = super::track::step_points(&shown, now.timestamp() as f64);
        let color = if i == 0 {
            crate::theme::creator_color(&series.creator)
        } else {
            COMPARE_COLORS[(i - 1) % COMPARE_COLORS.len()]
        };
        prepared.push(PreparedSeries {
            name: display_of(slug),
            color,
            points: shown,
            steps,
            anchor,
        });
    }

    if prepared.is_empty() {
        ui.label(format!(
            "No {} history for this model yet.",
            state.metric.label()
        ));
        footer(ui, tracker);
        return;
    }

    chart(ui, &mut prepared, state, now);
    ui.add_space(6.0);
    event_log(ui, &prepared, state);
    footer(ui, tracker);
}

fn controls(ui: &mut egui::Ui, models: &[(&str, &str)], state: &mut HistoryUiState) {
    ui.horizontal_wrapped(|ui| {
        let selected_label = models
            .iter()
            .find(|(m, _)| *m == state.selected[0])
            .map(|(_, d)| *d)
            .unwrap_or("Select model");
        egui::ComboBox::from_id_salt("history_model")
            .selected_text(selected_label)
            .width(ui.available_width() * 0.4)
            .show_ui(ui, |ui| {
                for &(slug, display) in models {
                    let is_selected = slug == state.selected[0];
                    ui.selectable_value(&mut state.selected[0], slug.to_owned(), display);
                    if !is_selected && slug == state.selected[0] {
                        state.reset_zoom = true;
                    }
                }
            });

        // A ComboBox, not a menu: egui popups from `menu_button` render
        // their full content height (a 190-model wall), while ComboBox
        // dropdowns are scrollable and capped by egui itself.
        let mut pick = String::new();
        egui::ComboBox::from_id_salt("history_compare")
            .selected_text("+ compare")
            .width(150.0)
            .show_ui(ui, |ui| {
                for &(slug, display) in models {
                    if state.selected.iter().any(|s| s == slug) {
                        continue;
                    }
                    ui.selectable_value(&mut pick, slug.to_owned(), display);
                }
            });
        if !pick.is_empty() {
            state.selected.push(pick);
            state.reset_zoom = true;
        }
        for i in 1..state.selected.len() {
            let slug = state.selected[i].clone();
            let display = models
                .iter()
                .find(|(m, _)| *m == slug)
                .map(|(_, d)| *d)
                .unwrap_or(slug.as_str());
            let chip = egui::Button::new(format!("{display} ×"))
                .small()
                .fill(egui::Color32::from_rgb(40, 46, 60));
            if ui.add(chip).clicked() {
                state.selected.remove(i);
                state.reset_zoom = true;
                break;
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label("Metric:");
        for metric in [
            HistoryMetric::Blended,
            HistoryMetric::Input,
            HistoryMetric::Output,
            HistoryMetric::CacheRead,
            HistoryMetric::Capability,
            HistoryMetric::Deployment,
            HistoryMetric::VolumeDaily,
        ] {
            if ui
                .selectable_label(state.metric == metric, metric.label())
                .clicked()
            {
                state.metric = metric;
                state.log_y = metric.is_price();
                state.normalize = false;
                state.reset_zoom = true;
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label("Range:");
        for range in [
            HistoryRange::Day1,
            HistoryRange::Day7,
            HistoryRange::Day30,
            HistoryRange::All,
        ] {
            if ui
                .selectable_label(state.range == range, range.label())
                .clicked()
            {
                state.range = range;
                state.reset_zoom = true;
            }
        }
        ui.separator();
        if state.metric.is_price() {
            ui.checkbox(&mut state.log_y, "log y");
        }
        if state.selected.len() > 1 {
            ui.checkbox(&mut state.normalize, "% vs window start");
        }
        if ui.button("reset view").clicked() {
            state.reset_zoom = true;
        }
    });
}

/// Windowed change points for one metric. Direct metrics slice into their
/// stored series (copying only the visible window); derived metrics are
/// computed first and then windowed. Never clones the full stored history
/// except for the "All" range, which is inherently fully visible.
fn windowed_metric_series(
    series: &ModelSeries,
    metric: HistoryMetric,
    settings: &Settings,
    range: HistoryRange,
    now: DateTime<Utc>,
) -> Option<Vec<(f64, f64)>> {
    let derived;
    let src: &[(f64, f64)] = match metric {
        HistoryMetric::Input => &series.input,
        HistoryMetric::Output => &series.output,
        HistoryMetric::CacheRead => &series.cache_read,
        HistoryMetric::Capability => &series.capability,
        HistoryMetric::Deployment => &series.deployment,
        HistoryMetric::Blended => {
            derived = blended_series(
                series,
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            );
            &derived
        }
        HistoryMetric::VolumeDaily => {
            derived = series
                .telemetry
                .iter()
                .map(|(ts, _, vol)| (*ts, *vol))
                .collect();
            &derived
        }
    };
    let points = window(src, range, now);
    (!points.is_empty()).then_some(points)
}

/// Keeps points inside the window plus the last point before it as a
/// left-edge anchor so step charts enter with the correct starting value.
fn window(points: &[(f64, f64)], range: HistoryRange, now: DateTime<Utc>) -> Vec<(f64, f64)> {
    let Some(secs) = range.seconds() else {
        return points.to_vec();
    };
    let since = (now.timestamp() as f64) - secs as f64;
    let start = points
        .partition_point(|(ts, _)| *ts < since)
        .saturating_sub(1);
    points[start..].to_vec()
}

fn chart(
    ui: &mut egui::Ui,
    prepared: &mut [PreparedSeries],
    state: &mut HistoryUiState,
    now: DateTime<Utc>,
) {
    let plot_height = (ui.available_height() * 0.62).max(320.0);
    let saved_style = ui.style().clone();
    {
        let style = ui.style_mut();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));
    }

    let metric = state.metric;
    let normalize = state.normalize;
    let now_ts = now.timestamp() as f64;
    let mut hover_x: Option<f64> = None;

    let response = Plot::new("history_plot")
        .height(plot_height)
        .legend(Legend::default().position(egui_plot::Corner::LeftBottom))
        .x_axis_formatter(|mark, range| {
            let span = range.end() - range.start();
            format_ts_axis(mark.value, span)
        })
        .y_axis_formatter(move |mark, _| {
            if normalize {
                format!("{:.0}%", mark.value)
            } else {
                metric.format_value(mark.value)
            }
        })
        .y_axis_label(if normalize {
            "% of window start".into()
        } else {
            metric.y_label()
        })
        .x_axis_label("time (UTC)")
        .allow_zoom(true)
        .allow_scroll(true)
        .allow_drag(true)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(true)
        .pan_pointer_button(egui::PointerButton::Middle)
        .set_margin_fraction(egui::vec2(0.03, 0.05))
        .label_formatter(|_| None)
        .show(ui, |plot_ui| {
            if state.reset_zoom {
                // Compute and SET explicit bounds for both axes. Do not leave
                // either axis on auto bounds here: the flag is sticky across
                // frames and then every frame refits content, including the
                // transient cursor shapes below, so drifting the pointer past
                // an edge stretched the view sideways.
                // A flat or single-point series has a degenerate (zero-height)
                // y span, and auto bounds then fall back to an enormous
                // default window ($0.35 reads on a $0-100 axis). Override with
                // a readable window around the actual value.
                let mut x_min = f64::MAX;
                let mut x_max = f64::MIN;
                let mut y_min = f64::MAX;
                let mut y_max = f64::MIN;
                for s in &*prepared {
                    for [x, y] in &s.steps {
                        x_min = x_min.min(*x);
                        x_max = x_max.max(*x);
                        y_min = y_min.min(*y);
                        y_max = y_max.max(*y);
                    }
                }
                let mid = (x_min + x_max) / 2.0;
                let (x_lo, x_hi) = if x_max - x_min < MIN_X_SPAN_SECS {
                    (mid - MIN_X_SPAN_SECS / 2.0, mid + MIN_X_SPAN_SECS / 2.0)
                } else {
                    // Mirror the 3% relative margin auto bounds used to add.
                    let pad = (x_max - x_min) * 0.03;
                    (x_min - pad, x_max + pad)
                };
                plot_ui.set_plot_bounds_x(x_lo..=x_hi);
                let (y_lo, y_hi) = padded_y_bounds(metric, y_min, y_max).unwrap_or_else(|| {
                    // Meaningful spread: match the 5% y margin from
                    // set_margin_fraction, not a hard clip to the data.
                    let pad = (y_max - y_min) * 0.05;
                    (y_min - pad, y_max + pad)
                });
                plot_ui.set_plot_bounds_y(y_lo..=y_hi);
            }
            // The pointer coordinate maps anywhere on screen through the last
            // transform, so clamp to the visible x range: past the left/right
            // edge it would otherwise draw the crosshair outside the data and,
            // under auto bounds, expand those bounds toward the cursor.
            hover_x = plot_ui.pointer_coordinate().map(|c| c.x).filter(|&x| {
                let range = plot_ui.plot_bounds().range_x();
                x >= *range.start() && x <= *range.end()
            });

            if let Some(x) = hover_x {
                plot_ui.vline(
                    // Unnamed shapes stay out of the legend.
                    VLine::new("", x)
                        .color(egui::Color32::from_rgba_unmultiplied(160, 174, 192, 110))
                        .width(1.0),
                );
            }
            for s in &mut *prepared {
                plot_ui.line(
                    Line::new(
                        s.name.clone(),
                        PlotPoints::from(std::mem::take(&mut s.steps)),
                    )
                    .color(s.color)
                    .width(2.0),
                );
                if let Some(x) = hover_x {
                    if let Some((_, v)) = nearest_at_or_before(&s.points, x) {
                        plot_ui.points(
                            Points::new("", PlotPoints::from(vec![[x, *v]]))
                                .radius(3.5)
                                .color(egui::Color32::WHITE),
                        );
                    }
                }
            }
        });
    state.reset_zoom = false;

    if let Some(x) = hover_x {
        if response.response.hovered() {
            response.response.on_hover_ui(|ui| {
                ui.vertical(|ui| {
                    ui.strong(format_time_utc(x));
                    for s in &*prepared {
                        match nearest_at_or_before(&s.points, x) {
                            Some((ts, v)) => {
                                let prev =
                                    nearest_strictly_before(&s.points, *ts).map(|(_, pv)| *pv);
                                ui.horizontal(|ui| {
                                    ui.colored_label(s.color, "●");
                                    ui.label(series_tooltip_row(
                                        s, *ts, *v, prev, metric, normalize,
                                    ));
                                });
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    ui.colored_label(s.color, "●");
                                    ui.weak(format!("no data at cursor ({})", s.name));
                                });
                            }
                        }
                    }
                    ui.weak(format!("{} ago", age_string(now_ts - x)));
                });
            });
        }
    }
    ui.set_style(saved_style);
}

fn series_tooltip_row(
    s: &PreparedSeries,
    ts: f64,
    v: f64,
    prev: Option<f64>,
    metric: HistoryMetric,
    normalize: bool,
) -> String {
    let value = if normalize && s.anchor.abs() > f64::EPSILON {
        format!("{:.1}% (was {:.2})", v, v / 100.0 * s.anchor)
    } else {
        metric.format_value(v)
    };
    let change = prev.map(|p| {
        if normalize {
            format!("  Δ{:+.1}pt", v - p)
        } else if p.abs() > f64::EPSILON {
            format!(
                "  {} ({:+.1}%)",
                metric.format_value(v - p),
                (v - p) / p * 100.0
            )
        } else {
            String::new()
        }
    });
    format!(
        "{}: {} · at {}{}",
        s.name,
        value,
        format_time_utc(ts),
        change.unwrap_or_default()
    )
}

fn event_log(ui: &mut egui::Ui, prepared: &[PreparedSeries], state: &HistoryUiState) {
    let Some(primary) = prepared.first() else {
        return;
    };
    let rows: Vec<String> = primary
        .points
        .iter()
        .enumerate()
        .rev()
        .map(|(i, (ts, v))| {
            let prev = i
                .checked_sub(1)
                .and_then(|j| primary.points.get(j))
                .map(|(_, pv)| *pv);
            match prev {
                Some(p) if p.abs() > f64::EPSILON => format!(
                    "{}  {} → {}  ({:+.1}%)",
                    format_time_utc(*ts),
                    state.metric.format_value(p),
                    state.metric.format_value(*v),
                    (v - p) / p * 100.0
                ),
                Some(_) => format!(
                    "{}  {}",
                    format_time_utc(*ts),
                    state.metric.format_value(*v)
                ),
                None => format!(
                    "{}  first record: {}",
                    format_time_utc(*ts),
                    state.metric.format_value(*v)
                ),
            }
        })
        .take(80)
        .collect();

    ui.collapsing("Change log", |ui| {
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                if rows.is_empty() {
                    ui.label("No changes recorded for this metric yet.");
                }
                for row in rows {
                    ui.monospace(row);
                }
            });
    });
}

fn footer(ui: &mut egui::Ui, tracker: &HistoryTracker) {
    let (bytes, frames) = tracker.store_stats();
    let size = if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    };
    ui.horizontal(|ui| {
        ui.weak(format!(
            "{} models · {} events · {} on disk",
            tracker.model_count(),
            frames,
            size
        ));
        if let Some(reason) = tracker.degraded_reason() {
            ui.colored_label(egui::Color32::from_rgb(252, 165, 165), reason);
        }
    });
}

/// Minimum visible time span (30 minutes) so a single-point series still
/// renders a sensible axis instead of a degenerate sliver.
const MIN_X_SPAN_SECS: f64 = 1_800.0;

fn load_selection_from(path: &std::path::Path) -> Vec<String> {
    let raw = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    raw.into_iter()
        .filter(|slug| !slug.is_empty() && seen.insert(slug.clone()))
        .collect()
}

fn save_selection_to(path: &std::path::Path, selected: &[String]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(selected) {
        let _ = std::fs::write(path, json);
    }
}

/// Expands a degenerate (flat/single-value) y range into a readable window
/// around the value. Multiplicative padding keeps log axes well-formed.
/// Returns `None` when the existing span is already meaningful.
fn padded_y_bounds(metric: HistoryMetric, y_min: f64, y_max: f64) -> Option<(f64, f64)> {
    if !y_min.is_finite() || !y_max.is_finite() {
        return None;
    }
    let magnitude = y_max.abs().max(y_min.abs());
    let (factor, floor) = match metric {
        HistoryMetric::Capability | HistoryMetric::Deployment => (0.25, 2.5),
        HistoryMetric::VolumeDaily => (0.25, 1.0),
        _ => (0.5, 0.05),
    };
    let min_span = (magnitude * factor).max(floor);
    if y_max - y_min >= min_span {
        return None;
    }
    let pad = (min_span - (y_max - y_min)) / 2.0;
    Some((y_min - pad, y_max + pad))
}

fn nearest_at_or_before(points: &[(f64, f64)], x: f64) -> Option<&(f64, f64)> {
    let idx = points.partition_point(|(ts, _)| *ts <= x);
    idx.checked_sub(1).and_then(|i| points.get(i))
}

fn nearest_strictly_before(points: &[(f64, f64)], x: f64) -> Option<&(f64, f64)> {
    let idx = points.partition_point(|(ts, _)| *ts < x);
    idx.checked_sub(1).and_then(|i| points.get(i))
}

fn format_time_utc(ts: f64) -> String {
    DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%d %b %Y %H:%M UTC").to_string())
        .unwrap_or_else(|| format!("{ts:.0}"))
}

/// Axis labels that adapt granularity to the visible span (seconds).
fn format_ts_axis(value: f64, span_seconds: f64) -> String {
    let Some(dt) = DateTime::from_timestamp(value as i64, 0) else {
        return String::new();
    };
    if span_seconds <= 21_600.0 {
        dt.format("%H:%M").to_string()
    } else if span_seconds <= 3.0 * 86_400.0 {
        dt.format("%d %b %H:%M").to_string()
    } else if span_seconds <= 250.0 * 86_400.0 {
        dt.format("%d %b").to_string()
    } else {
        dt.format("%b %Y").to_string()
    }
}

fn age_string(secs: f64) -> String {
    let secs = secs.max(0.0) as i64;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::track::carry_forward;

    #[test]
    fn window_keeps_pre_window_anchor() {
        let now = DateTime::from_timestamp(9_000_000, 0).unwrap();
        let pts: Vec<(f64, f64)> = (0..10)
            .map(|i| (i as f64 * 1_000_000.0, i as f64))
            .collect();
        // "All" keeps everything.
        assert_eq!(window(&pts, HistoryRange::All, now).len(), 10);
        let w = window(&pts, HistoryRange::Day30, now);
        assert!(w.len() < pts.len(), "points older than 30d drop out");
        assert!(w.len() >= 2, "anchor + in-window points retained");
        // The first retained point is the last one *before* the cutoff, even
        // though it is outside the window, so steps enter at the right level.
        let cutoff = 9_000_000.0 - 2_592_000.0;
        assert!(w[0].0 < cutoff);
        assert!(w.iter().skip(1).all(|(ts, _)| *ts >= cutoff));
    }

    #[test]
    fn nearest_point_lookup_is_inclusive_before() {
        let pts = vec![(1.0, 10.0), (5.0, 50.0)];
        assert_eq!(nearest_at_or_before(&pts, 0.5), None);
        assert_eq!(nearest_at_or_before(&pts, 1.0).unwrap().1, 10.0);
        assert_eq!(nearest_at_or_before(&pts, 4.9).unwrap().1, 10.0);
        assert_eq!(nearest_at_or_before(&pts, 99.0).unwrap().1, 50.0);
        assert_eq!(nearest_strictly_before(&pts, 5.0).unwrap().1, 10.0);
    }

    #[test]
    fn axis_formatting_adapts_to_span() {
        let ts = 1_750_000_000.0;
        let hour = format_ts_axis(ts, 3600.0);
        assert!(
            hour.len() == 5 && hour.contains(':'),
            "HH:MM for short spans: {hour}"
        );
        let month = format_ts_axis(ts, 900.0 * 86_400.0);
        assert!(
            !month.contains(':'),
            "month-level labels for long spans: {month}"
        );
    }

    #[test]
    fn carry_forward_matches_lookup() {
        let pts = vec![(0.0, 1.0), (10.0, 2.0)];
        assert_eq!(carry_forward(&pts, 5.0), Some(1.0));
        assert_eq!(carry_forward(&pts, 10.0), Some(2.0));
        assert_eq!(carry_forward(&pts, -1.0), None);
    }

    #[test]
    fn selection_roundtrips_through_disk() {
        let path = std::env::temp_dir()
            .join("paretowatch-history-tests")
            .join(format!("uisel-{}", std::process::id()))
            .join("history-ui.json");
        let _ = std::fs::remove_file(&path);
        save_selection_to(&path, &["openai/gpt-x".into(), "zai/glm-5-3".into()]);
        assert_eq!(
            load_selection_from(&path),
            vec!["openai/gpt-x", "zai/glm-5-3"]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn selection_load_survives_missing_file_garbage_and_dupes() {
        let dir = std::env::temp_dir()
            .join("paretowatch-history-tests")
            .join(format!("uiselbad-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Missing file.
        assert!(load_selection_from(&dir.join("none.json")).is_empty());
        // Garbage JSON.
        let bad = dir.join("bad.json");
        std::fs::write(&bad, "{not json").unwrap();
        assert!(load_selection_from(&bad).is_empty());
        // Dupes and empty slugs collapse.
        let dupes = dir.join("dupes.json");
        std::fs::write(&dupes, r#"["a/b","a/b","","c/d"]"#).unwrap();
        assert_eq!(load_selection_from(&dupes), vec!["a/b", "c/d"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_point_price_gets_tight_readable_window() {
        // $0.35 model, one sample: must NOT sit on a $0-100 default axis.
        let (lo, hi) =
            padded_y_bounds(HistoryMetric::Blended, 0.35, 0.35).expect("degenerate y padded");
        assert!(
            lo > 0.2 && hi < 0.55,
            "tight window around $0.35, got {lo}..{hi}"
        );
        // A meaningful span is left alone.
        assert_eq!(padded_y_bounds(HistoryMetric::Blended, 1.0, 20.0), None);
        // Scores use a fixed floor so a flat 77.x line does not span 0-100.
        let (lo, hi) = padded_y_bounds(HistoryMetric::Capability, 77.7, 77.7).unwrap();
        assert!(
            lo > 65.0 && hi < 90.0,
            "score window around 77.7, got {lo}..{hi}"
        );
        // Zero stays representable (free model).
        let (lo, hi) = padded_y_bounds(HistoryMetric::Blended, 0.0, 0.0).unwrap();
        assert!(
            lo < 0.0 && hi > 0.0 && hi <= 0.05,
            "tiny window around $0, got {lo}..{hi}"
        );
    }
}
