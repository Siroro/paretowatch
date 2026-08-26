//! One compact always-on-top window for pinned model prices.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use eframe::egui;
use egui::{ViewportClass, ViewportCommand, ViewportId, WindowLevel};

use crate::alerts::prices_differ;
use crate::theme::{creator_color, discount_color};
use crate::types::{PriceSnapshot, Settings, blended_price};

const WINDOW_WIDTH: f32 = 240.0;
/// Everything around the row list (title, separators, footer, frame
/// margins), measured from a real `window_frame` layout with the default
/// fonts (see `window_size_estimate_tracks_measured_layout`).
const WINDOW_CHROME_HEIGHT: f32 = 45.0;
/// One pinned row plus the separator below it, measured the same way. This
/// only sizes the window until its first laid-out frame; the renderer then
/// fits the native window to the measured content.
const ROW_PITCH_HEIGHT: f32 = 46.5;
const DPI_ROUNDING_GUARD: f32 = 2.0;
/// Inner-size mismatches up to this are treated as matches: the OS
/// quantizes window sizes to whole physical pixels.
const SIZE_MATCH_EPSILON: f32 = 1.0;
const MAX_VISIBLE_ROWS: usize = 6;
const FLASH_SECS: f64 = 1.6;
const RAISE_INTERVAL: Duration = Duration::from_secs(1);

const CARD_FILL: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(16, 18, 23, 220);
const CARD_STROKE: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(86, 95, 111, 230);
const PRIMARY_TEXT: egui::Color32 = egui::Color32::from_rgb(232, 237, 244);
const METRIC_TEXT: egui::Color32 = egui::Color32::from_rgb(196, 205, 219);
const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(148, 157, 171);
const DOWN_GREEN: egui::Color32 = egui::Color32::from_rgb(54, 179, 126);
const UP_RED: egui::Color32 = egui::Color32::from_rgb(224, 86, 86);
const LIVE_ACCENT: egui::Color32 = egui::Color32::from_rgb(92, 186, 255);

#[derive(Debug, Clone, PartialEq)]
struct RowFeed {
    model_id: String,
    display_name: String,
    creator_color: egui::Color32,
    live_market: bool,
    priced: bool,
    free_offer_listed: bool,
    input: f64,
    cache_read: Option<f64>,
    output: f64,
    blended: f64,
    discount_pct: Option<f64>,
    discount_flash_at: Option<DateTime<Utc>>,
    last_change: Option<LastChange>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LastChange {
    at: DateTime<Utc>,
    old_blended: f64,
    new_blended: f64,
    old_input: f64,
    new_input: f64,
    old_output: f64,
    new_output: f64,
}

impl LastChange {
    fn delta(&self) -> f64 {
        self.new_blended - self.old_blended
    }

    fn percent_delta(&self) -> Option<f64> {
        (self.old_blended.abs() > f64::EPSILON)
            .then(|| (self.new_blended - self.old_blended) / self.old_blended * 100.0)
    }
}

#[derive(Default)]
struct SharedWindow {
    rows: Vec<RowFeed>,
    fetched_at: Option<DateTime<Utc>>,
    remove: Vec<String>,
    close: bool,
    last_raise: Option<Instant>,
    /// Height budget for the row list: the measured list height scaled to
    /// `MAX_VISIBLE_ROWS`, refreshed by the renderer every frame.
    scroll_cap: f32,
}

type SharedHandle = Arc<Mutex<SharedWindow>>;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ObservedPrices {
    live_market: bool,
    input: f64,
    cache_read: Option<f64>,
    output: f64,
    blended: f64,
    discount_pct: Option<f64>,
}

impl ObservedPrices {
    fn differs_from(&self, other: &Self) -> bool {
        prices_differ(self.input, other.input)
            || prices_differ(self.output, other.output)
            || prices_differ(self.blended, other.blended)
    }
}

fn discount_differs(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => prices_differ(a, b),
        (None, None) => false,
        _ => true,
    }
}

fn observe(quote: &crate::types::Quote, settings: &Settings) -> ObservedPrices {
    ObservedPrices {
        live_market: quote.live_market,
        input: quote.input,
        cache_read: quote.cache_read,
        output: quote.output,
        blended: blended_price(
            quote.input,
            quote.cache_read,
            quote.output,
            settings.input_weight,
            settings.cache_read_weight,
            settings.output_weight,
        ),
        discount_pct: quote.discount_pct,
    }
}

fn initial_feed(quote: &crate::types::Quote, settings: &Settings) -> (RowFeed, ObservedPrices) {
    let observed = observe(quote, settings);
    (
        RowFeed {
            model_id: quote.model.clone(),
            display_name: quote.display_name.clone(),
            creator_color: creator_color(&quote.creator),
            live_market: quote.live_market,
            priced: true,
            free_offer_listed: quote.free_offer_listed,
            input: quote.input,
            cache_read: quote.cache_read,
            output: quote.output,
            blended: observed.blended,
            discount_pct: observed.discount_pct,
            discount_flash_at: None,
            last_change: None,
        },
        observed,
    )
}

struct PinnedRow {
    model_id: String,
    last_seen: Option<ObservedPrices>,
}

struct QuoteWindow {
    viewport_id: ViewportId,
    builder: egui::ViewportBuilder,
    shared: SharedHandle,
    rows: Vec<PinnedRow>,
}

#[derive(Default)]
pub(crate) struct PriceWidgetManager {
    window: Option<QuoteWindow>,
}

impl PriceWidgetManager {
    pub(crate) fn spawn(
        &mut self,
        ctx: &egui::Context,
        snapshot: Option<&PriceSnapshot>,
        settings: &Settings,
        model_id: &str,
        near: egui::Pos2,
    ) {
        let Some(quote) = snapshot.and_then(|s| s.quotes.iter().find(|q| q.model == model_id))
        else {
            return;
        };

        if self.window.is_none() {
            let shared = Arc::new(Mutex::new(SharedWindow {
                fetched_at: snapshot.map(|s| s.fetched_at),
                scroll_cap: ROW_PITCH_HEIGHT * MAX_VISIBLE_ROWS as f32,
                ..Default::default()
            }));
            let mut builder = egui::ViewportBuilder::default()
                .with_title("ParetoWatch · pinned prices")
                .with_inner_size(window_size(1))
                .with_decorations(false)
                .with_resizable(false)
                .with_always_on_top()
                .with_active(false)
                .with_taskbar(false)
                .with_transparent(true);
            if let Some(pos) = spawn_position(ctx, near, window_size(1)) {
                builder = builder.with_position(pos);
            }
            self.window = Some(QuoteWindow {
                viewport_id: ViewportId::from_hash_of("paretowatch/pinned-prices"),
                builder,
                shared,
                rows: Vec::new(),
            });
        }

        let window = self.window.as_mut().expect("quote window just created");
        let already_pinned = window.rows.iter().any(|row| row.model_id == model_id);
        if !already_pinned {
            let (feed, observed) = initial_feed(quote, settings);
            window.rows.push(PinnedRow {
                model_id: model_id.to_owned(),
                last_seen: Some(observed),
            });
            window.builder = window
                .builder
                .clone()
                .with_inner_size(window_size(window.rows.len()));
            let mut shared = window.shared.lock().expect("quote window state");
            shared.rows.push(feed);
            shared.fetched_at = snapshot.map(|s| s.fetched_at);
        }

        raise_to_top(ctx, window.viewport_id);
    }

    pub(crate) fn refresh(
        &mut self,
        snapshot: Option<&PriceSnapshot>,
        settings: &Settings,
    ) -> bool {
        let Some(window) = &mut self.window else {
            return false;
        };
        let mut shared = window.shared.lock().expect("quote window state");
        let previous = shared.rows.clone();
        let mut rows = Vec::with_capacity(window.rows.len());

        for pin in &mut window.rows {
            let old_feed = previous.iter().find(|feed| feed.model_id == pin.model_id);
            let quote = snapshot.and_then(|s| s.quotes.iter().find(|q| q.model == pin.model_id));
            let feed = match quote {
                Some(quote) => {
                    let observed = observe(quote, settings);
                    let same_feed = pin
                        .last_seen
                        .is_some_and(|old| old.live_market == observed.live_market);
                    let last_change = match pin.last_seen {
                        Some(old) if same_feed && observed.differs_from(&old) => Some(LastChange {
                            at: snapshot.map(|s| s.fetched_at).unwrap_or_else(Utc::now),
                            old_blended: old.blended,
                            new_blended: observed.blended,
                            old_input: old.input,
                            new_input: observed.input,
                            old_output: old.output,
                            new_output: observed.output,
                        }),
                        _ => old_feed.and_then(|feed| feed.last_change),
                    };
                    let discount_flash_at = if same_feed
                        && pin.last_seen.is_some_and(|old| {
                            discount_differs(old.discount_pct, observed.discount_pct)
                        }) {
                        snapshot.map(|s| s.fetched_at)
                    } else {
                        old_feed.and_then(|feed| feed.discount_flash_at)
                    };
                    pin.last_seen = Some(observed);
                    RowFeed {
                        model_id: quote.model.clone(),
                        display_name: quote.display_name.clone(),
                        creator_color: creator_color(&quote.creator),
                        live_market: quote.live_market,
                        priced: true,
                        free_offer_listed: quote.free_offer_listed,
                        input: quote.input,
                        cache_read: quote.cache_read,
                        output: quote.output,
                        blended: observed.blended,
                        discount_pct: observed.discount_pct,
                        discount_flash_at,
                        last_change,
                    }
                }
                None => {
                    let Some(mut feed) = old_feed.cloned() else {
                        continue;
                    };
                    feed.priced = false;
                    pin.last_seen = None;
                    feed
                }
            };
            rows.push(feed);
        }

        let fetched_at = snapshot.map(|s| s.fetched_at);
        let changed = shared.rows != rows || shared.fetched_at != fetched_at;
        shared.rows = rows;
        shared.fetched_at = fetched_at;
        changed
    }

    pub(crate) fn show(&self, ctx: &egui::Context) {
        let Some(window) = &self.window else {
            return;
        };
        let shared = window.shared.clone();
        ctx.show_viewport_deferred(
            window.viewport_id,
            window.builder.clone(),
            move |ui, class| render(ui, class, &shared),
        );
    }

    pub(crate) fn request_repaints(&self, ctx: &egui::Context) {
        if let Some(window) = &self.window {
            ctx.request_repaint_of(window.viewport_id);
        }
    }

    pub(crate) fn prune_closed(&mut self) {
        let Some(window) = &mut self.window else {
            return;
        };
        let (close, remove) = {
            let mut shared = window.shared.lock().expect("quote window state");
            (shared.close, std::mem::take(&mut shared.remove))
        };
        if close {
            self.window = None;
            return;
        }
        if !remove.is_empty() {
            window.rows.retain(|row| !remove.contains(&row.model_id));
            if window.rows.is_empty() {
                self.window = None;
            } else {
                window.builder = window
                    .builder
                    .clone()
                    .with_inner_size(window_size(window.rows.len()));
            }
        }
    }
}

/// Raise the widget back to the top of the topmost band.
///
/// winit only issues the native z-order change when its tracked window flags
/// change, so a lone `AlwaysOnTop` command is a no-op for a window that already
/// reports topmost (even when another topmost window has raised above it).
/// Round-tripping through `Normal` forces a real lower-then-raise.
fn raise_to_top(ctx: &egui::Context, viewport_id: ViewportId) {
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Visible(true));
    ctx.send_viewport_cmd_to(
        viewport_id,
        ViewportCommand::WindowLevel(WindowLevel::Normal),
    );
    ctx.send_viewport_cmd_to(
        viewport_id,
        ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop),
    );
}

/// Estimated inner size for the given number of pinned rows, used when the
/// window is created or a row is added — before the next frame measures the
/// real layout. `render` corrects the native size from measured content.
fn window_size(row_count: usize) -> egui::Vec2 {
    let visible = row_count.clamp(1, MAX_VISIBLE_ROWS) as f32;
    egui::vec2(
        WINDOW_WIDTH,
        WINDOW_CHROME_HEIGHT + ROW_PITCH_HEIGHT * visible + DPI_ROUNDING_GUARD,
    )
}

fn spawn_position(ctx: &egui::Context, near: egui::Pos2, size: egui::Vec2) -> Option<egui::Pos2> {
    let root = ctx.input(|i| i.viewport().outer_rect)?;
    let global = root.left_top() + egui::vec2(near.x + 12.0, near.y + 12.0);
    let min = root.left_top();
    let max = egui::pos2(
        (root.right() - size.x).max(min.x),
        (root.bottom() - size.y).max(min.y),
    );
    Some(egui::pos2(
        global.x.clamp(min.x, max.x),
        global.y.clamp(min.y, max.y),
    ))
}

fn render(ui: &mut egui::Ui, class: ViewportClass, shared: &SharedHandle) {
    let (rows, fetched_at, scroll_cap) = {
        let state = shared.lock().expect("quote window state");
        (state.rows.clone(), state.fetched_at, state.scroll_cap)
    };
    // Before the first measured frame the cap is zero; keep the list visible.
    let scroll_cap = if scroll_cap > 0.0 {
        scroll_cap
    } else {
        ROW_PITCH_HEIGHT * MAX_VISIBLE_ROWS as f32
    };
    let os_close = ui.ctx().input(|i| i.viewport().close_requested());

    let flash = rows
        .iter()
        .map(|row| flash_intensity(row.discount_flash_at))
        .fold(0.0, f32::max);
    let card_rect = ui.max_rect();
    let radius = egui::CornerRadius::same(8);
    ui.painter().rect_filled(card_rect, radius, CARD_FILL);
    ui.painter().rect_stroke(
        card_rect,
        radius,
        egui::Stroke::new(1.0 + flash, CARD_STROKE),
        egui::StrokeKind::Inside,
    );
    let drag = ui.interact(
        card_rect,
        egui::Id::new("pinned-prices-drag"),
        egui::Sense::drag(),
    );

    let mut remove = None;
    let mut close_all = false;
    let (frame_height, rows_height) = window_frame(
        ui,
        &rows,
        fetched_at,
        scroll_cap,
        &mut remove,
        &mut close_all,
    );

    // Fit the native window to the laid-out content. The constants behind
    // `window_size` only estimate the size until the first frame; eframe
    // never re-applies later `ViewportBuilder` inner-size changes, so a
    // resize command is the only reliable channel.
    if !rows.is_empty() {
        let per_row = rows_height / rows.len() as f32;
        shared.lock().expect("quote window state").scroll_cap = per_row * MAX_VISIBLE_ROWS as f32;
    }
    if class == ViewportClass::Deferred {
        let needed = egui::vec2(WINDOW_WIDTH, frame_height + DPI_ROUNDING_GUARD);
        let matches = ui.input(|i| i.viewport().inner_rect).is_some_and(|rect| {
            (rect.width() - needed.x).abs() <= SIZE_MATCH_EPSILON
                && (rect.height() - needed.y).abs() <= SIZE_MATCH_EPSILON
        });
        if !matches {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::InnerSize(needed));
        }
    }

    if drag.drag_started() && class == ViewportClass::Deferred {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if let Some(model_id) = remove {
        let mut state = shared.lock().expect("quote window state");
        state.rows.retain(|row| row.model_id != model_id);
        state.remove.push(model_id);
        if state.rows.is_empty() {
            state.close = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(false));
        } else {
            ui.ctx().request_repaint();
        }
    }
    if close_all || os_close {
        shared.lock().expect("quote window state").close = true;
        if class == ViewportClass::Deferred {
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }

    // Topmost is not sticky: any other always-on-top window (task manager,
    // overlays) can raise above this one and stay there, because winit still
    // reports the widget as topmost and ignores a plain AlwaysOnTop command.
    // Re-forcing the raise once per repaint tick keeps the widget on top.
    let now = Instant::now();
    let reassert = {
        let mut state = shared.lock().expect("quote window state");
        match state.last_raise {
            Some(at) if now.duration_since(at) < RAISE_INTERVAL => false,
            _ => {
                state.last_raise = Some(now);
                true
            }
        }
    };
    if reassert && class == ViewportClass::Deferred {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
    }

    if flash > 0.0 {
        ui.ctx().request_repaint();
    } else {
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
}

/// Lay out the whole pinned-prices card content (title, row list, footer)
/// and return `(frame outer height, full height of the row list)`. The row
/// list height is the unclamped content height, even when `scroll_cap`
/// clips what is visible.
fn window_frame(
    ui: &mut egui::Ui,
    rows: &[RowFeed],
    fetched_at: Option<DateTime<Utc>>,
    scroll_cap: f32,
    remove: &mut Option<String>,
    close_all: &mut bool,
) -> (f32, f32) {
    let mut rows_height = 0.0;
    let frame = egui::Frame::default()
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.style_mut().spacing.item_spacing = egui::vec2(3.0, 0.5);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("PINNED PRICES")
                        .strong()
                        .size(9.5)
                        .color(MUTED_TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [13.0, 13.0],
                            egui::Button::new(
                                egui::RichText::new("×").size(10.0).color(MUTED_TEXT),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Close all pinned prices")
                        .clicked()
                    {
                        *close_all = true;
                    }
                });
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(scroll_cap)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, row) in rows.iter().enumerate() {
                        if index > 0 {
                            ui.separator();
                        }
                        render_row(ui, row, remove);
                    }
                    rows_height = ui.min_rect().height();
                });

            ui.horizontal(|ui| {
                let live = rows
                    .iter()
                    .filter(|row| row.live_market && row.priced)
                    .count();
                ui.label(
                    egui::RichText::new(format!("{} models · {live} live", rows.len()))
                        .size(8.5)
                        .color(if live > 0 { LIVE_ACCENT } else { MUTED_TEXT }),
                );
                if let Some(at) = fetched_at {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("upd {}", format_clock(at)))
                                .size(8.5)
                                .color(MUTED_TEXT),
                        );
                    });
                }
            });
        });
    (frame.response.rect.height(), rows_height)
}

fn render_row(ui: &mut egui::Ui, row: &RowFeed, remove: &mut Option<String>) {
    let price_color = row.discount_pct.map(discount_color).unwrap_or(PRIMARY_TEXT);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("●").color(row.creator_color).size(9.0));
        let mut name = row.display_name.clone();
        if row.free_offer_listed {
            name.push('*');
        }
        ui.label(
            egui::RichText::new(name)
                .strong()
                .size(10.0)
                .color(PRIMARY_TEXT),
        );
        if !row.priced {
            ui.label(
                egui::RichText::new("not priced")
                    .size(8.0)
                    .color(MUTED_TEXT),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_sized(
                    [12.0, 12.0],
                    egui::Button::new(egui::RichText::new("×").size(9.0).color(MUTED_TEXT))
                        .frame(false),
                )
                .on_hover_text("Remove this model")
                .clicked()
            {
                *remove = Some(row.model_id.clone());
            }
            ui.label(
                egui::RichText::new(format!("${:.4}", row.blended))
                    .strong()
                    .size(11.0)
                    .color(price_color),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!(
                "in {}  {}out {}",
                compact_price(row.input),
                row.cache_read
                    .map(|value| format!("cache {}  ", compact_price(value)))
                    .unwrap_or_default(),
                compact_price(row.output),
            ))
            .size(8.5)
            .color(METRIC_TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(change) = row.last_change {
                let delta = change.delta();
                let (direction, color) = if delta < 0.0 {
                    ("-", DOWN_GREEN)
                } else if delta > 0.0 {
                    ("+", UP_RED)
                } else {
                    ("=", MUTED_TEXT)
                };
                let pct = change
                    .percent_delta()
                    .map(|value| format!(" {value:+.1}%"))
                    .unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!("{direction}{pct}"))
                        .size(8.5)
                        .color(color),
                )
                .on_hover_text(format!(
                    "${:.4} → ${:.4} · {}",
                    change.old_blended,
                    change.new_blended,
                    format_age(change.at),
                ));
            } else if let Some(discount) = row.discount_pct {
                ui.label(
                    egui::RichText::new(format!("{discount:.1}% off"))
                        .size(8.5)
                        .color(price_color),
                );
            }
        });
    });
}

fn flash_intensity(flash_at: Option<DateTime<Utc>>) -> f32 {
    flash_at
        .map(|at| {
            let age = (Utc::now() - at).num_milliseconds() as f64 / 1000.0;
            if !(0.0..FLASH_SECS).contains(&age) {
                0.0
            } else {
                ((1.0 - age / FLASH_SECS) as f32).powi(2)
            }
        })
        .unwrap_or(0.0)
}

fn compact_price(value: f64) -> String {
    if value >= 100.0 {
        format!("${value:.1}")
    } else if value >= 1.0 {
        format!("${value:.2}")
    } else {
        format!("${value:.4}")
    }
}

fn format_clock(at: DateTime<Utc>) -> String {
    at.format("%H:%M:%S").to_string()
}

fn format_age(at: DateTime<Utc>) -> String {
    let secs = (Utc::now() - at).num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;

    fn snapshot_at(quotes: Vec<crate::types::Quote>, fetched_at: DateTime<Utc>) -> PriceSnapshot {
        PriceSnapshot {
            quotes,
            fetched_at,
            comparison_updated_at: None,
            market_overlay_count: 0,
            market_error: None,
            base_source: "test".into(),
        }
    }

    fn rows(mgr: &PriceWidgetManager) -> Vec<RowFeed> {
        mgr.window
            .as_ref()
            .unwrap()
            .shared
            .lock()
            .unwrap()
            .rows
            .clone()
    }

    #[test]
    fn pins_multiple_models_in_one_window_and_dedupes() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let now = Utc::now();
        let snap = snapshot_at(
            vec![
                test_quote("model-a", 1.0, true),
                test_quote("model-b", 2.0, true),
            ],
            now,
        );
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            "model-a",
            egui::pos2(1.0, 1.0),
        );
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            "model-b",
            egui::pos2(2.0, 2.0),
        );
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            "model-a",
            egui::pos2(3.0, 3.0),
        );

        assert_eq!(mgr.window.as_ref().unwrap().rows.len(), 2);
        assert_eq!(rows(&mgr).len(), 2);
        assert_eq!(
            mgr.window
                .as_ref()
                .unwrap()
                .shared
                .lock()
                .unwrap()
                .fetched_at,
            Some(now)
        );
    }

    #[test]
    fn refresh_updates_rows_as_one_snapshot() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let t0 = Utc::now();
        let t1 = t0 + chrono::Duration::seconds(30);
        let initial = snapshot_at(
            vec![
                test_quote("model-a", 1.0, true),
                test_quote("model-b", 2.0, true),
            ],
            t0,
        );
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(&ctx, Some(&initial), &settings, "model-a", egui::Pos2::ZERO);
        mgr.spawn(&ctx, Some(&initial), &settings, "model-b", egui::Pos2::ZERO);

        let next = snapshot_at(
            vec![
                test_quote("model-a", 0.9, true),
                test_quote("model-b", 2.1, true),
            ],
            t1,
        );
        assert!(mgr.refresh(Some(&next), &settings));
        let feeds = rows(&mgr);
        assert!(feeds.iter().all(|feed| feed.last_change.is_some()));
        assert_eq!(
            mgr.window
                .as_ref()
                .unwrap()
                .shared
                .lock()
                .unwrap()
                .fetched_at,
            Some(t1)
        );
        assert!(!mgr.refresh(Some(&next), &settings));
    }

    #[test]
    fn source_switch_is_not_a_price_change_and_missing_model_keeps_value() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let now = Utc::now();
        let initial = snapshot_at(vec![test_quote("model-a", 1.0, true)], now);
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(&ctx, Some(&initial), &settings, "model-a", egui::Pos2::ZERO);

        mgr.refresh(
            Some(&snapshot_at(vec![test_quote("model-a", 0.9, false)], now)),
            &settings,
        );
        assert!(rows(&mgr)[0].last_change.is_none());
        assert!(!rows(&mgr)[0].live_market);

        mgr.refresh(Some(&snapshot_at(Vec::new(), now)), &settings);
        assert!(!rows(&mgr)[0].priced);
        assert!((rows(&mgr)[0].input - 0.9).abs() < 1e-9);
    }

    #[test]
    fn discount_changes_flash_only_on_same_feed() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let t0 = Utc::now();
        let t1 = t0 + chrono::Duration::seconds(30);
        let mut quote = test_quote("model-a", 1.0, true);
        let initial = snapshot_at(vec![quote.clone()], t0);
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(&ctx, Some(&initial), &settings, "model-a", egui::Pos2::ZERO);

        quote.discount_pct = Some(85.0);
        mgr.refresh(Some(&snapshot_at(vec![quote.clone()], t1)), &settings);
        assert_eq!(rows(&mgr)[0].discount_flash_at, Some(t1));

        let mut catalog = test_quote("model-a", 1.0, false);
        catalog.discount_pct = None;
        mgr.refresh(Some(&snapshot_at(vec![catalog], t1)), &settings);
        assert_eq!(rows(&mgr)[0].discount_flash_at, Some(t1));
    }

    #[test]
    fn individual_removal_and_close_empty_window() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let snap = snapshot_at(
            vec![
                test_quote("model-a", 1.0, true),
                test_quote("model-b", 2.0, true),
            ],
            Utc::now(),
        );
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(&ctx, Some(&snap), &settings, "model-a", egui::Pos2::ZERO);
        mgr.spawn(&ctx, Some(&snap), &settings, "model-b", egui::Pos2::ZERO);

        {
            let window = mgr.window.as_ref().unwrap();
            let mut shared = window.shared.lock().unwrap();
            shared.rows.retain(|row| row.model_id != "model-a");
            shared.remove.push("model-a".into());
        }
        mgr.prune_closed();
        assert_eq!(mgr.window.as_ref().unwrap().rows.len(), 1);

        {
            let window = mgr.window.as_ref().unwrap();
            let mut shared = window.shared.lock().unwrap();
            shared.rows.clear();
            shared.remove.push("model-b".into());
        }
        mgr.prune_closed();
        assert!(mgr.window.is_none());
    }

    #[test]
    fn window_height_is_capped() {
        assert_eq!(window_size(0), window_size(1));
        assert_eq!(
            window_size(MAX_VISIBLE_ROWS),
            window_size(MAX_VISIBLE_ROWS + 10)
        );
        assert!(window_size(2).y > window_size(1).y);
        assert_eq!(
            window_size(1).y,
            WINDOW_CHROME_HEIGHT + ROW_PITCH_HEIGHT + DPI_ROUNDING_GUARD
        );
    }

    /// `window_size` is only a first-frame estimate — `render` fits the
    /// window to measured content afterwards — but it should stay close to
    /// the real layout so the window does not flash at the wrong size.
    /// If this drifts after an egui or font update, retune the constants.
    #[test]
    fn window_size_estimate_tracks_measured_layout() {
        let ctx = egui::Context::default();
        let row = RowFeed {
            model_id: "m".into(),
            display_name: "Model A".into(),
            creator_color: PRIMARY_TEXT,
            live_market: true,
            priced: true,
            free_offer_listed: false,
            input: 1.0,
            cache_read: Some(0.1),
            output: 2.0,
            blended: 1.2,
            discount_pct: Some(25.0),
            discount_flash_at: None,
            last_change: None,
        };
        let mut measured = Vec::new();
        for pass in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_max_width(WINDOW_WIDTH);
                let mut remove = None;
                let mut close_all = false;
                if pass == 0 {
                    measured.clear();
                }
                for n in 1..=4usize {
                    let rows = vec![row.clone(); n];
                    let (frame_height, _) = window_frame(
                        ui,
                        &rows,
                        Some(Utc::now()),
                        f32::INFINITY,
                        &mut remove,
                        &mut close_all,
                    );
                    if pass == 0 {
                        measured.push(frame_height);
                    }
                }
            });
            out.textures_delta.clear();
        }
        for (index, height) in measured.iter().enumerate() {
            let estimate = window_size(index + 1).y - DPI_ROUNDING_GUARD;
            assert!(
                (estimate - height).abs() <= 8.0,
                "rows {}: estimate {} vs measured {height}",
                index + 1,
                estimate
            );
        }
    }
}
