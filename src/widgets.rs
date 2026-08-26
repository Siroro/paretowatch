//! One compact always-on-top window for pinned model prices.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use eframe::egui;
use egui::{ViewportClass, ViewportCommand, ViewportId, WindowLevel};

use crate::alerts::prices_differ;
use crate::theme::{creator_color, discount_color};
use crate::types::{LiquidityFilter, PriceSnapshot, Quote, Settings, blended_price};

/// The pinned row's prices under the active market-quality filter: the
/// cheapest provider that satisfies it, or `None` when nothing does.
fn effective_quote(
    quote: &Quote,
    liquidity: LiquidityFilter,
    settings: &Settings,
) -> Option<Quote> {
    liquidity.apply(
        quote,
        settings.input_weight,
        settings.cache_read_weight,
        settings.output_weight,
    )
}

/// Base pinned-price text size the layout constants were measured at. The
/// user-facing setting scales every text size (and the window width) from
/// here; 10.0 reproduces the original hard-coded look.
pub(crate) const BASE_FONT_SIZE: f32 = 10.0;
/// Allowed range for the pinned-prices font size setting.
pub(crate) const PINNED_FONT_MIN: f32 = 8.0;
pub(crate) const PINNED_FONT_MAX: f32 = 16.0;

const WINDOW_WIDTH: f32 = 240.0;
/// Everything around the row list (title, separators, footer, frame
/// margins), measured from a real `window_frame` layout at
/// `BASE_FONT_SIZE` (see `window_size_estimate_tracks_measured_layout`).
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

fn font_scale(font_size: f32) -> f32 {
    (font_size.clamp(PINNED_FONT_MIN, PINNED_FONT_MAX) / BASE_FONT_SIZE).max(0.1)
}

/// One of the window's original text sizes, scaled to the configured font.
fn scaled(size: f32, font_size: f32) -> f32 {
    size * font_scale(font_size)
}

const CARD_FILL: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(16, 18, 23, 220);
const CARD_STROKE: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(86, 95, 111, 230);
const PRIMARY_TEXT: egui::Color32 = egui::Color32::from_rgb(232, 237, 244);
const METRIC_TEXT: egui::Color32 = egui::Color32::from_rgb(196, 205, 219);
const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(148, 157, 171);
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
    /// Base text size pushed from `Settings` by `refresh`; zero until then.
    font_size: f32,
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
        liquidity: LiquidityFilter,
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
                scroll_cap: ROW_PITCH_HEIGHT
                    * font_scale(settings.pinned_price_font_size)
                    * MAX_VISIBLE_ROWS as f32,
                font_size: settings.pinned_price_font_size,
                ..Default::default()
            }));
            let mut builder = egui::ViewportBuilder::default()
                .with_title("ParetoWatch · pinned prices")
                .with_inner_size(window_size(1, settings.pinned_price_font_size))
                .with_decorations(false)
                .with_resizable(false)
                .with_always_on_top()
                .with_active(false)
                .with_taskbar(false)
                .with_transparent(true);
            if let Some(pos) =
                spawn_position(ctx, near, window_size(1, settings.pinned_price_font_size))
            {
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
            // Metadata comes from the raw quote; prices come from the
            // market-quality-filtered selection. A model excluded by the
            // filter pins as not priced, mirroring a missing quote.
            let effective = effective_quote(quote, liquidity, settings);
            let (mut feed, observed) = initial_feed(effective.as_ref().unwrap_or(quote), settings);
            feed.priced = effective.is_some();
            window.rows.push(PinnedRow {
                model_id: model_id.to_owned(),
                last_seen: effective.is_some().then_some(observed),
            });
            window.builder = window.builder.clone().with_inner_size(window_size(
                window.rows.len(),
                settings.pinned_price_font_size,
            ));
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
        liquidity: LiquidityFilter,
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
            // Rows follow the market-quality filter: prices re-select to the
            // cheapest qualifying provider, and a model with no qualifying
            // provider reads as not priced.
            let effective = quote.and_then(|quote| effective_quote(quote, liquidity, settings));
            let feed = match effective {
                Some(effective) => {
                    let observed = observe(&effective, settings);
                    let same_feed = pin
                        .last_seen
                        .is_some_and(|old| old.live_market == observed.live_market);
                    let last_change = match pin.last_seen {
                        Some(old) if same_feed && observed.differs_from(&old) => Some(LastChange {
                            at: snapshot.map(|s| s.fetched_at).unwrap_or_else(Utc::now),
                            old_blended: old.blended,
                            new_blended: observed.blended,
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
                        model_id: effective.model.clone(),
                        display_name: effective.display_name.clone(),
                        creator_color: creator_color(&effective.creator),
                        live_market: effective.live_market,
                        priced: true,
                        free_offer_listed: effective.free_offer_listed,
                        input: effective.input,
                        cache_read: effective.cache_read,
                        output: effective.output,
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
        let font_changed = shared.font_size != settings.pinned_price_font_size;
        let changed = shared.rows != rows || shared.fetched_at != fetched_at || font_changed;
        shared.rows = rows;
        shared.fetched_at = fetched_at;
        shared.font_size = settings.pinned_price_font_size;
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
        let (close, remove, font_size) = {
            let mut shared = window.shared.lock().expect("quote window state");
            (
                shared.close,
                std::mem::take(&mut shared.remove),
                shared.font_size,
            )
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
                    .with_inner_size(window_size(window.rows.len(), font_size));
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
/// Only the width scales with the font: across the allowed 8–16 range the
/// row height is dominated by egui's minimum widget sizes (the × buttons),
/// so the base-font height estimate stays close at every size.
fn window_size(row_count: usize, font_size: f32) -> egui::Vec2 {
    let visible = row_count.clamp(1, MAX_VISIBLE_ROWS) as f32;
    egui::vec2(
        WINDOW_WIDTH * font_scale(font_size),
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
    let (rows, fetched_at, scroll_cap, font_size) = {
        let state = shared.lock().expect("quote window state");
        (
            state.rows.clone(),
            state.fetched_at,
            state.scroll_cap,
            state.font_size,
        )
    };
    // Before the first `refresh` pushes settings the defaults are zero;
    // keep the list visible and the text at the original size.
    let font_size = if font_size > 0.0 {
        font_size
    } else {
        BASE_FONT_SIZE
    };
    let scroll_cap = if scroll_cap > 0.0 {
        scroll_cap
    } else {
        ROW_PITCH_HEIGHT * font_scale(font_size) * MAX_VISIBLE_ROWS as f32
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
        font_size,
        &mut remove,
        &mut close_all,
    );

    // Fit the native window to the laid-out content. The constants behind
    // `window_size` only estimate the size until the first frame; eframe
    // never re-applies later `ViewportBuilder` inner-size changes, so a
    // resize command is the only reliable channel. Both the row height and
    // the width track the configured font size, so this also re-fits the
    // window whenever the setting changes.
    if !rows.is_empty() {
        let per_row = rows_height / rows.len() as f32;
        shared.lock().expect("quote window state").scroll_cap = per_row * MAX_VISIBLE_ROWS as f32;
    }
    if class == ViewportClass::Deferred {
        let needed = egui::vec2(
            WINDOW_WIDTH * font_scale(font_size),
            frame_height + DPI_ROUNDING_GUARD,
        );
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
/// clips what is visible. All text is scaled from `font_size`.
fn window_frame(
    ui: &mut egui::Ui,
    rows: &[RowFeed],
    fetched_at: Option<DateTime<Utc>>,
    scroll_cap: f32,
    font_size: f32,
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
                        .size(scaled(9.5, font_size))
                        .color(MUTED_TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = scaled(13.0, font_size);
                    if ui
                        .add_sized(
                            [button, button],
                            egui::Button::new(
                                egui::RichText::new("×")
                                    .size(scaled(10.0, font_size))
                                    .color(MUTED_TEXT),
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
                        render_row(ui, row, font_size, remove);
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
                        .size(scaled(8.5, font_size))
                        .color(if live > 0 { LIVE_ACCENT } else { MUTED_TEXT }),
                );
                if let Some(at) = fetched_at {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("upd {}", format_clock(at)))
                                .size(scaled(8.5, font_size))
                                .color(MUTED_TEXT),
                        );
                    });
                }
            });
        });
    (frame.response.rect.height(), rows_height)
}

fn render_row(ui: &mut egui::Ui, row: &RowFeed, font_size: f32, remove: &mut Option<String>) {
    let price_color = row.discount_pct.map(discount_color).unwrap_or(PRIMARY_TEXT);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("●")
                .color(row.creator_color)
                .size(scaled(9.0, font_size)),
        );
        let mut name = row.display_name.clone();
        if row.free_offer_listed {
            name.push('*');
        }
        ui.label(
            egui::RichText::new(name)
                .strong()
                .size(scaled(10.0, font_size))
                .color(PRIMARY_TEXT),
        );
        if !row.priced {
            ui.label(
                egui::RichText::new("not priced")
                    .size(scaled(8.0, font_size))
                    .color(MUTED_TEXT),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = scaled(12.0, font_size);
            if ui
                .add_sized(
                    [button, button],
                    egui::Button::new(
                        egui::RichText::new("×")
                            .size(scaled(9.0, font_size))
                            .color(MUTED_TEXT),
                    )
                    .frame(false),
                )
                .on_hover_text("Remove this model")
                .clicked()
            {
                *remove = Some(row.model_id.clone());
            }
            let price = ui.label(
                egui::RichText::new(format!("${:.4}", row.blended))
                    .strong()
                    .size(scaled(11.0, font_size))
                    .color(price_color),
            );
            // The change trail never owns visible space; it lives on hover so
            // the discount label below cannot be displaced by tiny jitters.
            if let Some(change) = row.last_change {
                price.on_hover_text(format!(
                    "${:.4} → ${:.4} · {}",
                    change.old_blended,
                    change.new_blended,
                    format_age(change.at),
                ));
            }
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
            .size(scaled(8.5, font_size))
            .color(METRIC_TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(discount) = row.discount_pct {
                ui.label(
                    egui::RichText::new(format!("{discount:.1}% off"))
                        .size(scaled(8.5, font_size))
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
    use crate::types::ProviderMarketQuote;

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
            LiquidityFilter::Any,
            "model-a",
            egui::pos2(1.0, 1.0),
        );
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Any,
            "model-b",
            egui::pos2(2.0, 2.0),
        );
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Any,
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
        mgr.spawn(
            &ctx,
            Some(&initial),
            &settings,
            LiquidityFilter::Any,
            "model-a",
            egui::Pos2::ZERO,
        );
        mgr.spawn(
            &ctx,
            Some(&initial),
            &settings,
            LiquidityFilter::Any,
            "model-b",
            egui::Pos2::ZERO,
        );

        let next = snapshot_at(
            vec![
                test_quote("model-a", 0.9, true),
                test_quote("model-b", 2.1, true),
            ],
            t1,
        );
        assert!(mgr.refresh(Some(&next), &settings, LiquidityFilter::Any));
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
        assert!(!mgr.refresh(Some(&next), &settings, LiquidityFilter::Any));
    }

    #[test]
    fn pinned_rows_follow_the_market_quality_filter() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let mut quote = test_quote("model-a", 1.0, true);
        quote.provider = "cheap-untrusted".into();
        quote.input = 0.5;
        quote.output = 1.0;
        quote.cache_read = Some(0.05);
        quote.market_options = vec![
            ProviderMarketQuote {
                provider: "cheap-untrusted".into(),
                input: 0.5,
                output: 1.0,
                cache_read: Some(0.05),
                trusted: Some(false),
                healthy_seller_count: Some(2),
            },
            ProviderMarketQuote {
                provider: "trusted-provider".into(),
                input: 0.6,
                output: 1.1,
                cache_read: Some(0.06),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
        ];
        let snap = snapshot_at(vec![quote], Utc::now());
        let mut mgr = PriceWidgetManager::default();

        // Trusted live provider: the row prices from the trusted ask, not
        // the cheaper untrusted headline.
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Trusted,
            "model-a",
            egui::Pos2::ZERO,
        );
        let feed = &rows(&mgr)[0];
        assert!(feed.priced);
        assert!((feed.input - 0.6).abs() < 1e-9);
        assert_eq!(feed.cache_read, Some(0.06));
        assert!((feed.blended - 0.193).abs() < 1e-9);

        // Tightened past every seller: not priced, keeps the last display.
        assert!(mgr.refresh(Some(&snap), &settings, LiquidityFilter::Healthy10));
        assert!(!rows(&mgr)[0].priced);
        assert!((rows(&mgr)[0].input - 0.6).abs() < 1e-9);

        // Any pricing: back to the cheapest ask.
        assert!(mgr.refresh(Some(&snap), &settings, LiquidityFilter::Any));
        let feed = &rows(&mgr)[0];
        assert!(feed.priced);
        assert!((feed.input - 0.5).abs() < 1e-9);
    }

    #[test]
    fn source_switch_is_not_a_price_change_and_missing_model_keeps_value() {
        let ctx = egui::Context::default();
        let settings = Settings::default();
        let now = Utc::now();
        let initial = snapshot_at(vec![test_quote("model-a", 1.0, true)], now);
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(
            &ctx,
            Some(&initial),
            &settings,
            LiquidityFilter::Any,
            "model-a",
            egui::Pos2::ZERO,
        );

        mgr.refresh(
            Some(&snapshot_at(vec![test_quote("model-a", 0.9, false)], now)),
            &settings,
            LiquidityFilter::Any,
        );
        assert!(rows(&mgr)[0].last_change.is_none());
        assert!(!rows(&mgr)[0].live_market);

        mgr.refresh(
            Some(&snapshot_at(Vec::new(), now)),
            &settings,
            LiquidityFilter::Any,
        );
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
        mgr.spawn(
            &ctx,
            Some(&initial),
            &settings,
            LiquidityFilter::Any,
            "model-a",
            egui::Pos2::ZERO,
        );

        quote.discount_pct = Some(85.0);
        mgr.refresh(
            Some(&snapshot_at(vec![quote.clone()], t1)),
            &settings,
            LiquidityFilter::Any,
        );
        assert_eq!(rows(&mgr)[0].discount_flash_at, Some(t1));

        let mut catalog = test_quote("model-a", 1.0, false);
        catalog.discount_pct = None;
        mgr.refresh(
            Some(&snapshot_at(vec![catalog], t1)),
            &settings,
            LiquidityFilter::Any,
        );
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
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Any,
            "model-a",
            egui::Pos2::ZERO,
        );
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Any,
            "model-b",
            egui::Pos2::ZERO,
        );

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
        assert_eq!(
            window_size(0, BASE_FONT_SIZE),
            window_size(1, BASE_FONT_SIZE)
        );
        assert_eq!(
            window_size(MAX_VISIBLE_ROWS, BASE_FONT_SIZE),
            window_size(MAX_VISIBLE_ROWS + 10, BASE_FONT_SIZE)
        );
        assert!(window_size(2, BASE_FONT_SIZE).y > window_size(1, BASE_FONT_SIZE).y);
        assert_eq!(
            window_size(1, BASE_FONT_SIZE).y,
            WINDOW_CHROME_HEIGHT + ROW_PITCH_HEIGHT + DPI_ROUNDING_GUARD
        );
        // A bigger font widens the estimate; the height stays at the
        // base-font estimate (widget minimums dominate it), and the
        // setting range is clamped.
        let bigger = window_size(2, PINNED_FONT_MAX);
        let smaller = window_size(2, PINNED_FONT_MIN);
        assert!(bigger.x > smaller.x && bigger.y >= smaller.y);
        assert_eq!(window_size(1, 4.0), window_size(1, PINNED_FONT_MIN));
        assert_eq!(window_size(1, 99.0), window_size(1, PINNED_FONT_MAX));
    }

    /// `window_size` is only a first-frame estimate — `render` fits the
    /// window to measured content afterwards — but it should stay close to
    /// the real layout so the window does not flash at the wrong size.
    /// If this drifts after an egui or font update, retune the constants.
    #[test]
    fn window_size_estimate_tracks_measured_layout() {
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
        let measure = |font_size: f32| -> Vec<f32> {
            let ctx = egui::Context::default();
            let mut measured = Vec::new();
            for pass in 0..3 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    ui.set_max_width(WINDOW_WIDTH * font_scale(font_size));
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
                            font_size,
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
            measured
        };

        let base = measure(BASE_FONT_SIZE);
        for (index, height) in base.iter().enumerate() {
            let estimate = window_size(index + 1, BASE_FONT_SIZE).y - DPI_ROUNDING_GUARD;
            assert!(
                (estimate - height).abs() <= 8.0,
                "rows {}: estimate {} vs measured {height}",
                index + 1,
                estimate
            );
        }

        // Off-base fonts: egui's minimum widget sizes (the × buttons) keep
        // the real row height nearly flat across the 8–16 range, so the
        // base-font height estimate should stay close at every size.
        // `render` sizes the window from measurement, not this estimate, so
        // the other property that matters is that the per-row pitch stays
        // uniform — the scroll cap divides content height by row count.
        for font in [PINNED_FONT_MIN, 12.0, PINNED_FONT_MAX] {
            let measured = measure(font);
            let pitches = measured.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
            for pitch in &pitches {
                assert!(
                    (pitch - pitches[0]).abs() <= 2.0,
                    "font {font}: non-uniform row pitch {pitches:?}"
                );
            }
            for (index, height) in measured.iter().enumerate() {
                let estimate = window_size(index + 1, font).y - DPI_ROUNDING_GUARD;
                assert!(
                    (estimate - height).abs() <= 10.0,
                    "font {font} rows {}: estimate {} vs measured {height}",
                    index + 1,
                    estimate
                );
            }
        }
    }

    #[test]
    fn font_size_setting_flows_into_window_state() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let now = Utc::now();
        let snap = snapshot_at(vec![test_quote("model-a", 1.0, true)], now);
        let mut mgr = PriceWidgetManager::default();
        mgr.spawn(
            &ctx,
            Some(&snap),
            &settings,
            LiquidityFilter::Any,
            "model-a",
            egui::Pos2::ZERO,
        );
        assert_eq!(
            mgr.window
                .as_ref()
                .unwrap()
                .shared
                .lock()
                .unwrap()
                .font_size,
            settings.pinned_price_font_size
        );

        settings.pinned_price_font_size = 14.0;
        assert!(mgr.refresh(Some(&snap), &settings, LiquidityFilter::Any));
        assert_eq!(
            mgr.window
                .as_ref()
                .unwrap()
                .shared
                .lock()
                .unwrap()
                .font_size,
            14.0
        );
        // Nothing left to change: the same settings report no delta.
        assert!(!mgr.refresh(Some(&snap), &settings, LiquidityFilter::Any));
    }
}
