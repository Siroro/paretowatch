//! Session activity: observed price moves and notifications that fired.

use eframe::egui;

use super::{ActivityTab, ParetoWatchApp};

impl ParetoWatchApp {
    pub(super) fn activity_tab(&mut self, ui: &mut egui::Ui) {
        let notification_count = self
            .notifications
            .lock()
            .map(|log| log.records().count())
            .unwrap_or_default();

        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.activity_tab,
                ActivityTab::PriceMoves,
                format!("Recent price moves ({})", self.recent_changes.len()),
            );
            ui.selectable_value(
                &mut self.activity_tab,
                ActivityTab::Notifications,
                format!("Notification history ({notification_count})"),
            );
        });
        ui.separator();

        match self.activity_tab {
            ActivityTab::PriceMoves => self.recent_price_moves(ui),
            ActivityTab::Notifications => self.notification_history(ui),
        }
    }

    fn recent_price_moves(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Recent price moves");
            if !self.recent_changes.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "Newest first · {} retained this session",
                        self.recent_changes.len()
                    ))
                    .weak(),
                );
            }
        });
        ui.add_space(4.0);

        if self.recent_changes.is_empty() {
            ui.label("No price changes observed since the app started.");
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("activity_recent_price_moves")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for change in &self.recent_changes {
                    let delta = change.delta();
                    let (arrow, color) = if delta < 0.0 {
                        ("↓", egui::Color32::from_rgb(54, 179, 126))
                    } else if delta > 0.0 {
                        ("↑", egui::Color32::from_rgb(224, 86, 86))
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
                                    ui.small(change.at.format("%H:%M:%S UTC").to_string());
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
                            ui.small(change.source);
                        });
                    });
                    ui.add_space(5.0);
                }
            });
    }

    fn notification_history(&self, ui: &mut egui::Ui) {
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.heading("Notification history");
            ui.label(egui::RichText::new("Session only · newest first").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clear = ui.small_button("Clear history").clicked();
            });
        });
        ui.add_space(4.0);

        if clear && let Ok(mut log) = self.notifications.lock() {
            log.clear();
        }

        let records = self
            .notifications
            .lock()
            .map(|log| log.records().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        if records.is_empty() {
            ui.label("No alerts have fired this session.");
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
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.small(record.at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                                },
                            );
                        });
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
