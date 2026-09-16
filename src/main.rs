#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod network;
mod theme;
mod ui_foundation;
mod vault;

use anyhow::Context;
use eframe::egui;

fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nodus")
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Nodus",
        options,
        Box::new(|cc| Ok(Box::new(app::NodusApp::new(cc)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
    .context("não foi possível iniciar a interface")
}
