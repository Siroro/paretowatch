#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alerts;
mod app;
mod artificial_analysis_snapshot;
mod bench;
mod fetch;
mod format;
mod history;
mod notifications;
mod pareto;
mod settings_store;
#[cfg(test)]
mod testfix;
mod theme;
mod tray;
mod types;
mod widgets;
mod worker;

use app::ParetoWatchApp;
use eframe::egui;

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    tray::spawn_linux_tray();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ParetoWatch")
            .with_inner_size([1050.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_taskbar(true)
            .with_visible(false),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "ParetoWatch",
        native_options,
        Box::new(|cc| Ok(Box::new(ParetoWatchApp::new(cc)))),
    )
}
