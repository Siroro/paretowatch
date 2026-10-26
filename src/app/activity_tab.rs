//! Persistent activity: observed price moves and alert events across restarts.

use chrono::{DateTime, Utc};
use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::HashMap;

use crate::bench::{benchmarks_for_source, best_benchmark_match, format_percentile};
use crate::format::{format_compact_number, format_compact_usd, format_price_tick};
use crate::history::track::{blended_series, historical_low, step_points};
use crate::history::ui::{HistoryMetric, HistoryRange};
use crate::notifications::NotificationKind;
use crate::theme::{
    PRICE_DOWN, PRICE_UP, STATUS_OK, copyable_slug, creator_color, discount_color, free_offer_badge,
};
use crate::types::{
    AlertDirection, AlertMode, Benchmark, BenchmarkSource, ComparisonMode, CostBasis, PriceMetric,
    Quote,
};

use super::{ActivityTab, NewListing, NewListingsWindow, ParetoWatchApp, Tab};

impl ParetoWatchApp {
    pub(super) fn activity_tab(&mut self, ui: &mut egui::Ui) {
        let (price_count, notification_count) = self
            .notifications
            .lock()
            .map(|log| (log.price_moves().count(), log.records().count()))
            .unwrap_or_default();
        self.ensure_new_listings();
        let cutoff = window_cutoff(self.new_listings_window, Utc::now().timestamp() as f64);
        let listing_count = self
            .new_listings_cache
            .rows
            .iter()
            .filter(|row| listed_within(row.added_ts, cutoff))
            .count();

        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.activity_tab,
                ActivityTab::PriceMoves,
                format!("Price moves ({price_count})"),
            );
            ui.selectable_value(
                &mut self.activity_tab,
                ActivityTab::Notifications,
                format!("Alert activity ({notification_count})"),
            );
            ui.selectable_value(
                &mut self.activity_tab,
                ActivityTab::NewListings,
                format!("New listings ({listing_count})"),
            );
        });
        ui.separator();

        match self.activity_tab {
            ActivityTab::PriceMoves => self.recent_price_moves(ui),
            ActivityTab::Notifications => self.notification_history(ui),
            ActivityTab::NewListings => self.new_listings(ui),
        }
    }

    fn activity_model_filter(&mut self, ui: &mut egui::Ui) {
        ui.label("Model");
        ui.add(
            egui::TextEdit::singleline(&mut self.activity_model_filter)
                .hint_text("Filter model…")
                .desired_width(220.0),
        );
        if !self.activity_model_filter.is_empty() && ui.small_button("✗ Clear filter").clicked() {
            self.activity_model_filter.clear();
        }
    }

    fn recent_price_moves(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.heading("Price moves");
            ui.label(egui::RichText::new("Persistent · newest first · up to 2,000").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clear = ui.small_button("✗ Clear price moves").clicked();
            });
        });
        ui.horizontal(|ui| self.activity_model_filter(ui));
        ui.add_space(4.0);

        if clear && let Ok(mut log) = self.notifications.lock() {
            log.clear_price_moves();
        }

        let filter = self.activity_model_filter.trim().to_lowercase();
        let moves = self
            .notifications
            .lock()
            .map(|log| {
                log.price_moves()
                    .filter(|change| {
                        filter.is_empty()
                            || change.model.to_lowercase().contains(&filter)
                            || change.display_name.to_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if moves.is_empty() {
            ui.label(if filter.is_empty() {
                "No persisted price moves yet."
            } else {
                "No price moves match this model filter."
            });
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("activity_recent_price_moves")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for change in &moves {
                    let delta = change.delta();
                    let (arrow, color) = if delta < 0.0 {
                        ("↓", PRICE_DOWN)
                    } else if delta > 0.0 {
                        ("↑", PRICE_UP)
                    } else {
                        ("↔", ui.visuals().text_color())
                    };

                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.colored_label(color, egui::RichText::new(arrow).size(20.0).strong());
                            ui.strong(&change.display_name);
                            if let Some(pct) = change.percent_delta() {
                                ui.colored_label(color, format!("{pct:+.2}%"));
                            } else {
                                ui.colored_label(color, "changed");
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.small(change.at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                                },
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.small(format!(
                                "Blended  ${:.6} → ${:.6}",
                                change.old_blended, change.new_blended
                            ));
                            ui.separator();
                            ui.small(format!(
                                "Input  ${:.6} → ${:.6}",
                                change.old_input, change.new_input
                            ));
                            ui.separator();
                            ui.small(format!(
                                "Output  ${:.6} → ${:.6}",
                                change.old_output, change.new_output
                            ));
                            ui.separator();
                            ui.small(&change.source);
                        });
                    });
                    ui.add_space(5.0);
                }
            });
    }

    fn notification_history(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.heading("Alert activity");
            ui.label(egui::RichText::new("Persistent · newest first · up to 2,000").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clear = ui.small_button("✗ Clear alert activity").clicked();
            });
        });
        ui.horizontal_wrapped(|ui| {
            self.activity_model_filter(ui);
            ui.separator();
            ui.label("Event type");
            egui::ComboBox::from_id_salt("activity_kind_filter")
                .selected_text(
                    self.activity_kind_filter
                        .map(NotificationKind::label)
                        .unwrap_or("All events"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.activity_kind_filter, None, "All events");
                    for kind in NotificationKind::ALL {
                        ui.selectable_value(
                            &mut self.activity_kind_filter,
                            Some(kind),
                            kind.label(),
                        );
                    }
                });
        });
        ui.add_space(4.0);

        if clear && let Ok(mut log) = self.notifications.lock() {
            log.clear();
        }

        let filter = self.activity_model_filter.trim().to_lowercase();
        let kind_filter = self.activity_kind_filter;
        let records = self
            .notifications
            .lock()
            .map(|log| {
                log.records()
                    .filter(|record| kind_filter.is_none_or(|kind| record.kind == kind))
                    .filter(|record| {
                        filter.is_empty()
                            || record
                                .model
                                .as_deref()
                                .is_some_and(|model| model.to_lowercase().contains(&filter))
                            || record.summary.to_lowercase().contains(&filter)
                            || record.body.to_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if records.is_empty() {
            ui.label("No alert activity matches the current filters.");
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("activity_notification_history")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for record in &records {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.strong(&record.summary);
                            ui.label(egui::RichText::new(record.kind.label()).weak());
                            if !record.delivered {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!(
                                        "suppressed: {}",
                                        record.suppressed_reason.as_deref().unwrap_or("policy")
                                    ),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.small(record.at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                                },
                            );
                        });
                        if let Some(model) = &record.model {
                            ui.small(format!("Model: {model}"));
                        }
                        if !record.body.is_empty() {
                            ui.add_space(2.0);
                            ui.small(&record.body);
                        }
                    });
                    ui.add_space(5.0);
                }
            });
    }

    /// New listings: every model the persistent history log first saw inside
    /// the selected window, with pricing, benchmark coverage, and — expanded —
    /// the price-since-listing chart, telemetry, the full consensus grid, and
    /// the market's provider asks.
    fn new_listings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("New listings");
            ui.label(egui::RichText::new("First sighting in the price feed · newest first").weak());
        });
        ui.horizontal(|ui| {
            self.activity_model_filter(ui);
            ui.separator();
            ui.label("Window:");
            for window in [
                NewListingsWindow::Day1,
                NewListingsWindow::Day7,
                NewListingsWindow::Day30,
                NewListingsWindow::AllTime,
            ] {
                if ui
                    .selectable_label(self.new_listings_window == window, window.label())
                    .clicked()
                {
                    self.new_listings_window = window;
                }
            }
        });
        ui.add_space(4.0);

        let now_ts = Utc::now().timestamp() as f64;
        let cutoff = window_cutoff(self.new_listings_window, now_ts);

        // A history log younger than the window has never seen the models
        // that were listed before it started — say so instead of implying
        // the whole catalogue is new.
        let log_start = self
            .history
            .series_list()
            .map(|series| series.added_ts)
            .fold(f64::INFINITY, f64::min);
        if log_start.is_finite() && cutoff.is_some_and(|cutoff| log_start >= cutoff) {
            ui.small(format!(
                "History log begins {} — models listed before that were never recorded as new.",
                format_listing_time(log_start)
            ));
            ui.add_space(4.0);
        }

        let filter = self.activity_model_filter.trim().to_lowercase();
        let (rows, total_matching) = self.new_listing_rows(&filter, cutoff);

        if rows.is_empty() {
            if !filter.is_empty() {
                ui.label("No new listings match this filter.");
            } else if self.new_listings_cache.rows.is_empty() {
                if self.price_snapshot.is_none() {
                    ui.label("Waiting for pricing data — listings appear once the history log records its first poll.");
                } else {
                    ui.label("No models recorded yet.");
                }
            } else {
                ui.label(format!(
                    "No new listings in the last {}.",
                    match self.new_listings_window {
                        NewListingsWindow::Day1 => "24 hours".to_owned(),
                        window => window.label().to_lowercase(),
                    }
                ));
            }
            return;
        }

        ui.horizontal(|ui| {
            ui.strong(format!(
                "{} listing{}",
                rows.len(),
                if rows.len() == 1 { "" } else { "s" }
            ));
            if total_matching > rows.len() {
                ui.label(
                    egui::RichText::new(format!(
                        "of {total_matching} matching — showing the newest {}",
                        rows.len()
                    ))
                    .weak(),
                );
            }
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("activity_new_listings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in &rows {
                    self.new_listing_card(ui, row);
                    ui.add_space(5.0);
                }
            });
    }

    /// Window- and search-filtered listing rows, newest first, with the lazy
    /// benchmark-coverage counts filled in. Returns the rows plus the total
    /// match count before the display cap.
    fn new_listing_rows(&mut self, filter: &str, cutoff: Option<f64>) -> (Vec<NewListing>, usize) {
        let mut rows: Vec<NewListing> = self
            .new_listings_cache
            .rows
            .iter()
            .filter(|row| listed_within(row.added_ts, cutoff))
            .filter(|row| {
                filter.is_empty()
                    || row.slug.to_lowercase().contains(filter)
                    || row.display.to_lowercase().contains(filter)
                    || row.creator.to_lowercase().contains(filter)
            })
            .cloned()
            .collect();
        let total_matching = rows.len();
        rows.truncate(MAX_VISIBLE_NEW_LISTINGS);

        // Board coverage walks every leaderboard row, so the collapsed boards
        // are built once per cache epoch and per-row counts are memoized.
        let sets = &self.benchmark_sets;
        let mode = self.comparison_mode;
        let scaffold = self.common_scaffold.clone();
        let cache = &mut self.new_listings_cache;
        if rows
            .iter()
            .any(|row| !cache.coverage.contains_key(&row.slug))
        {
            let boards = cache
                .boards
                .get_or_insert_with(|| consensus_boards(sets, mode, &scaffold));
            for row in &mut rows {
                row.benchmark_sources = *cache
                    .coverage
                    .entry(row.slug.clone())
                    .or_insert_with(|| listing_benchmark_coverage(&row.quote, boards));
            }
        }
        (rows, total_matching)
    }

    fn new_listing_card(&mut self, ui: &mut egui::Ui, row: &NewListing) {
        let now_ts = Utc::now().timestamp() as f64;
        let expanded = self.new_listings_selected.as_deref() == Some(row.slug.as_str());
        let mut toggle_expand = false;
        let total_sources = BenchmarkSource::consensus_sources().len();

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            ui.horizontal(|ui| {
                if now_ts - row.added_ts < NEW_BADGE_SECS {
                    ui.colored_label(NEW_BADGE_COLOR, egui::RichText::new("NEW").small().strong());
                }
                ui.label(
                    egui::RichText::new("●")
                        .color(creator_color(&row.creator))
                        .size(16.0),
                );
                ui.strong(&row.display);
                if !row.creator.is_empty() {
                    ui.weak(format!("by {}", row.creator));
                }
                if row.quote.vision {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 255), "vision");
                }
                if row.quote.uncensored {
                    ui.colored_label(egui::Color32::from_rgb(240, 170, 60), "uncensored");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small(format_listing_time(row.added_ts));
                    ui.label(egui::RichText::new(format_age(row.added_ts, now_ts)).weak());
                });
            });
            copyable_slug(ui, &row.slug);

            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Blend ${:.4}/1M", row.blended));
                ui.separator();
                ui.small(format!("In ${:.4}", row.quote.input));
                if let Some(cache_read) = row
                    .quote
                    .effective_cache_read(self.settings.cache_read_fallback_pct)
                {
                    ui.small(format!("Cache ${cache_read:.4}"));
                }
                ui.small(format!("Out ${:.4}", row.quote.output));
                ui.separator();
                if row.quote.live_market {
                    ui.strong("live market");
                    if row.quote.free_offer_listed {
                        free_offer_badge(ui);
                    }
                } else {
                    ui.label("catalog price");
                }
                if let Some(discount) = row.quote.discount_pct {
                    ui.separator();
                    ui.colored_label(discount_color(discount), format!("{discount:.0}% off"));
                }
                if let Some(requests) = row.quote.requests_24h {
                    ui.separator();
                    ui.small(format!(
                        "{} req/24h",
                        format_compact_number(requests as f64)
                    ));
                }
                if let Some(volume) = row.quote.volume_24h {
                    ui.separator();
                    ui.small(format!("{} vol/24h", format_compact_usd(volume)));
                }
            });

            ui.horizontal(|ui| {
                if row.benchmark_sources > 0 {
                    ui.colored_label(
                        STATUS_OK,
                        format!(
                            "◆ benchmarked on {} of {} boards",
                            row.benchmark_sources, total_sources
                        ),
                    );
                } else {
                    ui.weak(format!(
                        "◆ no rows on {} benchmark boards yet",
                        total_sources
                    ));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(if expanded {
                            "▾ Hide details"
                        } else {
                            "▸ Benchmarks & history"
                        })
                        .clicked()
                    {
                        toggle_expand = true;
                    }
                });
            });

            if expanded {
                ui.add_space(4.0);
                ui.separator();
                self.new_listing_details(ui, row);
            }
        });

        if toggle_expand {
            self.new_listings_selected = (!expanded).then(|| row.slug.clone());
        }
    }

    /// Expanded card body: price chart since listing, daily telemetry, the
    /// source-by-source benchmark grid, and the market's provider asks.
    fn new_listing_details(&mut self, ui: &mut egui::Ui, row: &NewListing) {
        let now_ts = Utc::now().timestamp() as f64;
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
            self.settings.cache_read_fallback_pct,
        );

        // Owned history extracts: nothing borrowed from self survives into
        // the render closures below.
        let series = self.history.series(&row.slug);
        let blended =
            series.map(|series| blended_series(series, weights.0, weights.1, weights.2, weights.3));
        let low = series.and_then(|series| {
            historical_low(
                series,
                PriceMetric::Blended,
                weights.0,
                weights.1,
                weights.2,
                weights.3,
            )
        });
        let telemetry = series
            .filter(|series| series.telemetry.len() >= 2)
            .map(|series| {
                (
                    series
                        .telemetry
                        .iter()
                        .map(|&(ts, _, volume)| (ts, volume))
                        .collect::<Vec<_>>(),
                    series
                        .telemetry
                        .iter()
                        .map(|&(ts, requests, _)| (ts, requests as f64))
                        .collect::<Vec<_>>(),
                )
            });
        let consensus = self.cached_consensus(&row.quote);

        // --- Blended price since listing ---------------------------------
        ui.horizontal(|ui| {
            ui.strong("Blended price since listing");
            if let Some((low, low_ts)) = low {
                ui.label(
                    egui::RichText::new(format!(
                        "· low {} on {}",
                        format_price_tick(low),
                        DateTime::from_timestamp(low_ts as i64, 0)
                            .map(|dt| dt.format("%d %b").to_string())
                            .unwrap_or_default(),
                    ))
                    .weak(),
                );
            }
            if let Some(changes) = blended.as_ref().map(|pts| pts.len().saturating_sub(1)) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} price change{} recorded",
                            changes,
                            if changes == 1 { "" } else { "s" }
                        ))
                        .weak(),
                    );
                });
            }
        });
        match blended.as_deref().map(|pts| step_points(pts, now_ts)) {
            Some(steps) if !steps.is_empty() => {
                listing_mini_chart(
                    ui,
                    format!("listing_price_{}", row.slug),
                    "blend",
                    &steps,
                    format_price_tick,
                );
            }
            _ => {
                ui.label("No price history recorded yet.");
            }
        }

        // --- Daily telemetry ---------------------------------------------
        if let Some((volume, requests)) = telemetry {
            ui.add_space(6.0);
            let volume_steps = step_points(&volume, now_ts);
            let request_steps = step_points(&requests, now_ts);
            let half = (ui.available_width() / 2.0 - 8.0).max(170.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(half);
                    ui.strong("Daily volume ($)");
                    listing_mini_chart(
                        ui,
                        format!("listing_volume_{}", row.slug),
                        "volume",
                        &volume_steps,
                        format_compact_usd,
                    );
                });
                ui.vertical(|ui| {
                    ui.set_min_width(half);
                    ui.strong("Requests / day");
                    listing_mini_chart(
                        ui,
                        format!("listing_requests_{}", row.slug),
                        "requests",
                        &request_steps,
                        format_compact_number,
                    );
                });
            });
        }

        // --- Benchmarks ----------------------------------------------------
        if let Some(consensus) = &consensus {
            ui.add_space(6.0);
            ui.strong(format!(
                "Benchmark consensus — {} of {} boards",
                consensus.entries.len(),
                BenchmarkSource::consensus_sources().len()
            ));
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("listing_consensus_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Benchmark");
                            ui.strong("Percentile");
                            ui.strong("Rank");
                            ui.strong("Score");
                            ui.strong("Matched row");
                            ui.end_row();
                            for entry in &consensus.entries {
                                ui.label(entry.source.short_label());
                                ui.label(format_percentile(entry.percentile));
                                ui.label(format!("#{}/{}", entry.rank, entry.total));
                                ui.label(format!("{:.1}", entry.score));
                                ui.label(&entry.benchmark_name);
                                ui.end_row();
                            }
                        });
                });
        } else {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "No benchmark board has scored this model yet — it cannot join the Pareto \
                     chart until one does.",
                )
                .weak(),
            );
        }

        // --- Market --------------------------------------------------------
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if row.benchmark_sources > 0
                && ui
                    .small_button("◆ Show on Pareto chart")
                    .on_hover_text(
                        "Opens the Pareto tab with this model selected (when the chart's \
                         current board scores it)",
                    )
                    .clicked()
            {
                self.selected_pareto_model = Some(row.slug.clone());
                self.tab = Tab::Pareto;
            }
            if ui.small_button("📈 Open in History").clicked() {
                self.history_ui.selected = vec![row.slug.clone()];
                self.history_ui.metric = HistoryMetric::Blended;
                self.history_ui.range = HistoryRange::All;
                self.history_ui.reset_zoom = true;
                self.tab = Tab::History;
            }
            if ui.small_button("📌 Pin price widget").clicked() {
                let near = ui
                    .ctx()
                    .input(|i| i.viewport().outer_rect)
                    .map(|root| root.left_top() + egui::vec2(24.0, 24.0))
                    .unwrap_or_else(|| egui::Pos2::new(60.0, 60.0));
                self.widgets.spawn(
                    ui.ctx(),
                    self.price_snapshot.as_ref(),
                    &self.settings,
                    self.liquidity_filter,
                    &row.slug,
                    near,
                );
            }
            ui.menu_button(egui::RichText::new("🔔 Alarm…").size(13.0), |ui| {
                if ui.button("Price threshold").clicked() {
                    self.alert_model = row.slug.clone();
                    self.alert_mode = AlertMode::Threshold;
                    self.alert_metric = PriceMetric::Blended;
                    self.alert_cost_basis = CostBasis::PerMillion;
                    self.alert_direction = AlertDirection::BelowOrEqual;
                    self.alert_threshold = row.blended;
                    self.tab = Tab::Alerts;
                    ui.close();
                }
                if ui.button("Any price change").clicked() {
                    self.alert_model = row.slug.clone();
                    self.alert_mode = AlertMode::AnyChange;
                    self.tab = Tab::Alerts;
                    ui.close();
                }
                if ui.button("All-time price low").clicked() {
                    self.alert_model = row.slug.clone();
                    self.alert_mode = AlertMode::PriceFloor;
                    self.tab = Tab::Alerts;
                    ui.close();
                }
            });
        });

        // The expanded listing detail is one of the on-demand triggers for
        // this model's per-provider route breakdown.
        self.ensure_performance(&row.quote.model, crate::fetch::ON_DEMAND_REFRESH_SECS);
        self.market_quality_section(ui, &row.quote);
    }
}

/// Display cap for the All-time window: rendering thousands of cards starves
/// the frame budget, and newest-first means the cap only ever trims the tail.
const MAX_VISIBLE_NEW_LISTINGS: usize = 300;
/// A listing this young carries the NEW badge regardless of the window size.
const NEW_BADGE_SECS: f64 = 48.0 * 3_600.0;
const NEW_BADGE_COLOR: egui::Color32 = egui::Color32::from_rgb(246, 190, 80);
/// Minimum visible time span (30 minutes) so a just-listed model's flat
/// price line still gets a sensible axis.
const MIN_X_SPAN_SECS: f64 = 1_800.0;

/// Earliest first-seen timestamp that still counts as inside the window.
/// `None` for "All time".
fn window_cutoff(window: NewListingsWindow, now_ts: f64) -> Option<f64> {
    window.seconds().map(|secs| now_ts - secs as f64)
}

fn listed_within(added_ts: f64, cutoff: Option<f64>) -> bool {
    cutoff.is_none_or(|cutoff| added_ts >= cutoff)
}

/// Collapsed rows for every consensus leaderboard, built once per cache
/// epoch — the per-source collapse (sort + dedupe across all rows) is the
/// expensive part of coverage counting and must not repeat per listing.
fn consensus_boards(
    sets: &HashMap<BenchmarkSource, Vec<Benchmark>>,
    mode: ComparisonMode,
    scaffold: &str,
) -> Vec<Vec<Benchmark>> {
    BenchmarkSource::consensus_sources()
        .iter()
        .map(|&source| benchmarks_for_source(source, sets, mode, scaffold))
        .collect()
}

/// Number of collapsed consensus leaderboards with a scored row matching
/// this quote. Mirrors the consensus grid's own matching so the compact
/// badge and the expanded grid can never disagree.
fn listing_benchmark_coverage(quote: &Quote, boards: &[Vec<Benchmark>]) -> usize {
    boards
        .iter()
        .filter(|rows| {
            best_benchmark_match(quote, rows.as_slice())
                .and_then(|matched| matched.agentic_coding)
                .is_some_and(|score| score.is_finite())
        })
        .count()
}

/// Age of a listing in human bands; anything older than a month shows its
/// calendar date instead of an ever-growing day count.
fn format_age(added_ts: f64, now_ts: f64) -> String {
    let secs = (now_ts - added_ts).max(0.0) as i64;
    if secs < 60 {
        "just now".into()
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3_600)
    } else if secs < 30 * 86_400 {
        format!("{}d ago", secs / 86_400)
    } else {
        DateTime::from_timestamp(added_ts as i64, 0)
            .map(|dt| dt.format("listed %d %b %Y").to_string())
            .unwrap_or_default()
    }
}

fn format_listing_time(ts: f64) -> String {
    DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| format!("{ts:.0}"))
}

/// Compact, read-only step chart for a listing card. Interaction is off so
/// the wheel scrolls the listing list instead of zooming a mini chart.
fn listing_mini_chart(
    ui: &mut egui::Ui,
    id: String,
    name: &str,
    steps: &[[f64; 2]],
    format_value: fn(f64) -> String,
) {
    Plot::new(id)
        .height(110.0)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .x_axis_formatter(|mark, range| {
            format_listing_axis(mark.value, range.end() - range.start())
        })
        .y_axis_formatter(move |mark, _| format_value(mark.value))
        .label_formatter(|_| None)
        .show(ui, |plot_ui| {
            if let Some((x0, x1, y0, y1)) = padded_plot_bounds(steps) {
                plot_ui.set_plot_bounds_x(x0..=x1);
                plot_ui.set_plot_bounds_y(y0..=y1);
            }
            plot_ui.line(
                Line::new(name.to_owned(), PlotPoints::from(steps.to_vec()))
                    .color(egui::Color32::from_rgb(96, 165, 250))
                    .width(2.0),
            );
        });
}

/// Explicit padded bounds for a mini chart: auto bounds on a flat or
/// single-point series (the common case for a just-listed model) degenerate,
/// so the y window is widened around the value instead. Returns
/// `(x_min, x_max, y_min, y_max)`.
fn padded_plot_bounds(steps: &[[f64; 2]]) -> Option<(f64, f64, f64, f64)> {
    if steps.is_empty() {
        return None;
    }
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    for &[x, y] in steps {
        x_min = x_min.min(x);
        x_max = x_max.max(x);
        y_min = y_min.min(y);
        y_max = y_max.max(y);
    }
    if !(x_min.is_finite() && x_max.is_finite() && y_min.is_finite() && y_max.is_finite()) {
        return None;
    }
    let x_pad = ((x_max - x_min) * 0.02).max(MIN_X_SPAN_SECS / 2.0);
    let magnitude = y_max.abs().max(y_min.abs());
    let min_span = (magnitude * 0.4).max(0.05);
    let y_span = y_max - y_min;
    let y_pad = if y_span < min_span {
        (min_span - y_span) / 2.0
    } else {
        y_span * 0.08
    };
    Some((x_min - x_pad, x_max + x_pad, y_min - y_pad, y_max + y_pad))
}

/// Time axis labels that adapt granularity to the visible span (seconds).
fn format_listing_axis(value: f64, span_seconds: f64) -> String {
    let Some(dt) = DateTime::from_timestamp(value as i64, 0) else {
        return String::new();
    };
    if span_seconds <= 2.0 * 86_400.0 {
        dt.format("%d %b %H:%M").to_string()
    } else if span_seconds <= 250.0 * 86_400.0 {
        dt.format("%d %b").to_string()
    } else {
        dt.format("%b %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::score_benchmark;
    use crate::testfix::test_quote;
    use crate::types::BenchmarkKind;

    #[test]
    fn coverage_counts_only_scored_matching_boards() {
        let mut sets = HashMap::new();
        // Terminal-Bench 4 carries a scored row for the model…
        sets.insert(
            BenchmarkSource::TerminalBench4,
            vec![score_benchmark(
                "glm-5.3",
                61.0,
                None,
                None,
                BenchmarkKind::Model,
            )],
        );
        // …LiveBench a matching row with no agentic score, which must not
        // count (mirrors the consensus grid, which skips unscored rows).
        let mut unscored = score_benchmark("glm-5.3", 70.0, None, None, BenchmarkKind::Model);
        unscored.agentic_coding = None;
        sets.insert(BenchmarkSource::LiveBench, vec![unscored]);

        let quote = test_quote("glm-5.3", 1.0, true);
        let unrelated = test_quote("totally-unrelated-model-x", 1.0, true);
        let boards = consensus_boards(&sets, ComparisonMode::BestAvailableAgent, "mini-SWE-agent");
        assert_eq!(listing_benchmark_coverage(&quote, &boards), 1);
        assert_eq!(listing_benchmark_coverage(&unrelated, &boards), 0);
    }

    #[test]
    fn window_membership_respects_cutoff() {
        let cutoff = Some(1_000.0);
        assert!(listed_within(1_000.0, cutoff));
        assert!(listed_within(2_000.0, cutoff));
        assert!(!listed_within(999.0, cutoff));
        assert!(listed_within(-5.0, None));
    }

    #[test]
    fn age_bands_and_dates() {
        let now = 1_800_000_000.0;
        assert_eq!(format_age(now - 30.0, now), "just now");
        assert_eq!(format_age(now - 90.0, now), "1m ago");
        assert_eq!(format_age(now - 5.0 * 3_600.0, now), "5h ago");
        assert_eq!(format_age(now - 3.0 * 86_400.0, now), "3d ago");
        let old = format_age(now - 40.0 * 86_400.0, now);
        assert!(
            old.starts_with("listed "),
            "month-old listings show their date: {old}"
        );
    }

    #[test]
    fn flat_single_point_series_gets_readable_bounds() {
        // A just-listed model's single price sample must not sit on a
        // degenerate axis: the y window widens around the value.
        let (x0, x1, y0, y1) =
            padded_plot_bounds(&[[100.0, 0.35], [4_000.0, 0.35]]).expect("bounds");
        assert!(x0 < 100.0 && x1 > 4_000.0);
        assert!(y0 < 0.35 && y1 > 0.35 && (y1 - y0) >= 0.14);
        // A meaningful span keeps proportionate padding.
        let (_, _, y0, y1) = padded_plot_bounds(&[[0.0, 1.0], [10.0, 20.0]]).expect("bounds");
        assert!((y1 - y1) < (y1 - y0)); // non-degenerate
        assert!(y0 < 1.0 && y1 > 20.0);
        assert!(padded_plot_bounds(&[]).is_none());
    }
}
