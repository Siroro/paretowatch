//! Persistent activity: observed price moves and alert events across restarts.

use eframe::egui;

use crate::notifications::NotificationKind;
use crate::theme::{PRICE_DOWN, PRICE_UP};

use super::{ActivityTab, ParetoWatchApp};

impl ParetoWatchApp {
    pub(super) fn activity_tab(&mut self, ui: &mut egui::Ui) {
        let (price_count, notification_count) = self
            .notifications
            .lock()
            .map(|log| (log.price_moves().count(), log.records().count()))
            .unwrap_or_default();

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
        });
        ui.separator();

        match self.activity_tab {
            ActivityTab::PriceMoves => self.recent_price_moves(ui),
            ActivityTab::Notifications => self.notification_history(ui),
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
}
