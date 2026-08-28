//! Settings tab: the actual settings, plus a compact feed-status block.

use eframe::egui;

use crate::settings_store::config_path;
use crate::theme::{STATUS_BAD, STATUS_OK};
use crate::types::BenchmarkSource;
use crate::widgets::{PINNED_FONT_MAX, PINNED_FONT_MIN};
use crate::worker::WorkerCommand;

use super::ParetoWatchApp;

impl ParetoWatchApp {
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Polling");
        ui.horizontal(|ui| {
            ui.label("Refresh every");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.poll_seconds)
                        .speed(10)
                        .range(30..=3600),
                )
                .changed()
            {
                self.settings_dirty = true;
            }
            ui.label("seconds");
        });

        ui.add_space(10.0);
        ui.heading("Pinned prices window");
        ui.horizontal(|ui| {
            ui.label("Font size");
            if ui
                .add(
                    egui::Slider::new(
                        &mut self.settings.pinned_price_font_size,
                        PINNED_FONT_MIN..=PINNED_FONT_MAX,
                    )
                    .step_by(0.5),
                )
                .changed()
            {
                self.settings_dirty = true;
            }
        });

        ui.add_space(10.0);
        ui.heading("Blended price workload");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Agentic 15/80/5").clicked() {
                self.settings.input_weight = 15.0;
                self.settings.cache_read_weight = 80.0;
                self.settings.output_weight = 5.0;
                self.settings_dirty = true;
            }
            if ui.button("No-cache 95/0/5").clicked() {
                self.settings.input_weight = 95.0;
                self.settings.cache_read_weight = 0.0;
                self.settings.output_weight = 5.0;
                self.settings_dirty = true;
            }
            if ui.button("Balanced 50/0/50").clicked() {
                self.settings.input_weight = 50.0;
                self.settings.cache_read_weight = 0.0;
                self.settings.output_weight = 50.0;
                self.settings_dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Fresh input");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.input_weight)
                        .speed(0.1)
                        .range(0.0..=100.0),
                )
                .changed()
            {
                self.settings_dirty = true;
            }
            ui.label("Cache read");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.cache_read_weight)
                        .speed(0.1)
                        .range(0.0..=100.0),
                )
                .changed()
            {
                self.settings_dirty = true;
            }
            ui.label("Output");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.output_weight)
                        .speed(0.1)
                        .range(0.0..=100.0),
                )
                .changed()
            {
                self.settings_dirty = true;
            }
        });
        let total = self.settings.input_weight.max(0.0)
            + self.settings.cache_read_weight.max(0.0)
            + self.settings.output_weight.max(0.0);
        if total > f64::EPSILON {
            ui.small(format!(
                "Current mix: {:.0}% fresh input · {:.0}% cache read · {:.0}% output",
                self.settings.input_weight.max(0.0) / total * 100.0,
                self.settings.cache_read_weight.max(0.0) / total * 100.0,
                self.settings.output_weight.max(0.0) / total * 100.0,
            ));
        }
        if ui
            .checkbox(
                &mut self.settings.include_cache_read_in_discount,
                "Include cache read in discount (workload-weighted vs list prices)",
            )
            .changed()
        {
            self.settings_dirty = true;
        }

        ui.add_space(10.0);
        ui.heading("Alert delivery");
        if ui
            .checkbox(
                &mut self.settings.quiet_hours_enabled,
                "Suppress desktop notifications and audio during quiet hours",
            )
            .changed()
        {
            self.settings_dirty = true;
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.settings.quiet_hours_enabled, |ui| {
                ui.label("Quiet from");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.settings.quiet_hours_start)
                            .range(0..=23u8)
                            .suffix(":00"),
                    )
                    .changed()
                {
                    self.settings_dirty = true;
                }
                ui.label("until");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.settings.quiet_hours_end)
                            .range(0..=23u8)
                            .suffix(":00"),
                    )
                    .changed()
                {
                    self.settings_dirty = true;
                }
                ui.label("local time");
            });
        });
        ui.small("Alert events are still written to Activity while muted or in quiet hours.");
        let muted_until = self
            .notifications
            .lock()
            .ok()
            .and_then(|log| log.muted_until());
        if let Some(until) = muted_until {
            ui.horizontal(|ui| {
                ui.small(format!(
                    "Temporarily muted until {} local time.",
                    until.with_timezone(&chrono::Local).format("%H:%M")
                ));
                if ui.small_button("🔊 Unmute now").clicked()
                    && let Ok(mut log) = self.notifications.lock()
                {
                    log.unmute();
                }
            });
        }

        ui.add_space(10.0);
        ui.heading("Feed status");
        if let Some(snapshot) = &self.price_snapshot {
            ui.horizontal(|ui| {
                ui.colored_label(
                    if self.last_price_error.is_some() {
                        STATUS_BAD
                    } else {
                        STATUS_OK
                    },
                    "●",
                );
                ui.label(format!(
                    "Base feed: {} · {} priced models",
                    snapshot.base_source,
                    snapshot.quotes.len()
                ));
            });
            if snapshot.market_overlay_count > 0 {
                ui.horizontal(|ui| {
                    ui.colored_label(STATUS_OK, "●");
                    ui.label(format!(
                        "Live market: {} quotes",
                        snapshot.market_overlay_count
                    ));
                });
            } else if let Some(err) = &snapshot.market_error {
                ui.horizontal(|ui| {
                    ui.colored_label(STATUS_BAD, "●");
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Live market unavailable: {err}"),
                    );
                });
            }
        } else {
            ui.horizontal(|ui| {
                ui.colored_label(ui.visuals().weak_text_color(), "○");
                ui.label("Waiting for pricing data…");
            });
        }
        if let Some(err) = &self.last_price_error {
            ui.horizontal(|ui| {
                ui.colored_label(STATUS_BAD, "●");
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Base feed error: {err}"),
                );
            });
        }
        for source in BenchmarkSource::display_sources() {
            ui.horizontal(|ui| {
                // ● green with rows loaded, ● red on fetch error, ○ not
                // fetched yet.
                if self.benchmark_errors.get(&source).is_some() {
                    ui.colored_label(STATUS_BAD, "●");
                } else if self.benchmark_sets.get(&source).is_some() {
                    ui.colored_label(STATUS_OK, "●");
                } else {
                    ui.colored_label(ui.visuals().weak_text_color(), "○");
                }
                ui.hyperlink_to(source.label(), source.source_url());
                if let Some(rows) = self.benchmark_sets.get(&source) {
                    ui.label(format!("· {} rows", rows.len()));
                }
                if let Some(err) = self.benchmark_errors.get(&source) {
                    ui.colored_label(ui.visuals().error_fg_color, format!("· {err}"));
                }
            });
        }

        ui.add_space(10.0);
        if ui
            .add_enabled(self.settings_dirty, egui::Button::new("Save settings"))
            .clicked()
        {
            self.save_and_push_settings();
            let _ = self.worker_tx.send(WorkerCommand::Refresh);
        }
        ui.label(format!("Config: {}", config_path().display()));
    }
}
