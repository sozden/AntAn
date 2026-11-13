#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod compute;
mod editor;
mod optim_ui;
mod project;
mod shot;
mod theme;
mod view3d;
mod viz;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_title("AntAn").with_inner_size([1560.0, 940.0]).with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native("AntAn", options, Box::new(|cc| Ok(Box::new(app::AntAnApp::new(cc)))))
}
