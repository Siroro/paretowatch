//! Settings tab: polling, benchmark feed status, pricing diagnostics, and
//! the blended-price workload mix.

use eframe::egui;

use crate::artificial_analysis_snapshot::{
    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE, ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
};
use crate::bench::{benchmarks_for_source, best_benchmark_match};
use crate::settings_store::config_path;
use crate::types::BenchmarkSource;
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
        ui.label("The live Surplus market feed currently advertises a 30-second cache; the comparison/catalog feeds update more slowly.");

        ui.add_space(14.0);
        ui.heading("Benchmarks");
        ui.label("All remote benchmark feeds are public and require no API key. Each remote feed refreshes independently every six hours.");
        ui.label(format!("Artificial Analysis is bundled as a static {} snapshot from {}; it never requires an API key and stays in the composite until we manually bump it.", ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION, ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE));
        ui.label("Default composite is a robust precision-weighted posterior. Board weights: 20% SWE-rebench + 15% Terminal-Bench 3.0 + 15% DeepSWE + 10% LiveBench Agentic + 5% Revelo Code Index (deployment flavor demotes model-level boards, capability flavor demotes harness-specific ones). Each board's weight is scaled by n/(n+2) over its anchored population — percentiles from tiny boards barely count. A small AA prior (10%) enters on the same anchored scale, ranked within the models the boards actually evaluate. Two Huber passes symmetrically downweight boards far from the consensus instead of trimming the lowest. Boards that have evaluated other models but not this one add 25%-weight pseudo-evidence at the model's prior, so a model topping two boards cannot outrank broad top-tier coverage; models no board has evaluated yet enter at 75% of their AA standing. Execution-mode SKUs (e.g. GPT-5.6 Sol Pro) inherit their base variant's rows.");
        ui.label("Comparison modes: Model capability prefers model-only rows, but automatically falls back to the best model+harness row for agent-only leaderboards; Best available agent always picks the strongest observed model+harness row per source; Same/common scaffold keeps only rows matching the selected harness.");
        for source in BenchmarkSource::display_sources() {
            let mapped_models = self.price_snapshot.as_ref().map(|snapshot| {
                let active = benchmarks_for_source(
                    source,
                    &self.benchmark_sets,
                    self.comparison_mode,
                    &self.common_scaffold,
                );
                snapshot
                    .quotes
                    .iter()
                    .filter(|quote| best_benchmark_match(quote, &active).is_some())
                    .count()
            });
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to(source.label(), source.source_url());
                if let Some(rows) = self.benchmark_sets.get(&source) {
                    ui.label(format!("· {} source rows", rows.len()));
                    if let Some(mapped) = mapped_models {
                        ui.label(format!("· {mapped} Surplus text models mapped"));
                    }
                } else {
                    ui.label("· waiting");
                }
                if let Some(err) = self.benchmark_errors.get(&source) {
                    ui.colored_label(ui.visuals().error_fg_color, format!("· {err}"));
                }
            });
        }

        ui.add_space(14.0);
        ui.heading("Pricing diagnostics");
        if let Some(snapshot) = &self.price_snapshot {
            ui.label(format!(
                "Base feed: {} · {} priced text-output models",
                snapshot.base_source,
                snapshot.quotes.len()
            ));
            ui.small("Model universe is gated by /v1/models architecture metadata: multimodal input is allowed, but output must be text-only. Image/video/audio/embedding/rerank products are excluded.");
            if snapshot.market_overlay_count > 0 {
                ui.label(format!(
                    "Live marketplace overlay: {} quotes",
                    snapshot.market_overlay_count
                ));
            } else if let Some(err) = &snapshot.market_error {
                ui.collapsing("Live marketplace overlay unavailable", |ui| {
                    ui.label(err);
                    ui.label("The app continues using Surplus comparison/catalog prices, so charts and alerts still work.");
                });
            } else {
                ui.label("Live marketplace overlay: no quotes returned");
            }
        } else {
            ui.label("Waiting for pricing data…");
        }
        if let Some(err) = &self.last_price_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Last base pricing error: {err}"),
            );
        }

        ui.add_space(14.0);
        ui.heading("Blended price workload");
        ui.label("Blended price is a normalized $/1M-token rate, not the cost of a fixed-size request. Agentic coding is input-heavy, and repeated context is often served from prompt cache, so cached reads get their own weight.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Agentic coding 15/80/5").clicked() {
                self.settings.input_weight = 15.0;
                self.settings.cache_read_weight = 80.0;
                self.settings.output_weight = 5.0;
                self.settings_dirty = true;
            }
            if ui.button("Input-heavy no-cache 95/0/5").clicked() {
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
                "Current mix: {:.1}% fresh input · {:.1}% cache read · {:.1}% output",
                self.settings.input_weight.max(0.0) / total * 100.0,
                self.settings.cache_read_weight.max(0.0) / total * 100.0,
                self.settings.output_weight.max(0.0) / total * 100.0,
            ));
        }
        ui.small("If a model/source does not publish cache-read pricing, ParetoWatch conservatively prices cache-read tokens at the normal input rate.");

        ui.add_space(14.0);
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
