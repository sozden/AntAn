#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod editor;
mod shot;
mod view3d;
mod viz;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_title("AntAn").with_inner_size([1480.0, 920.0]).with_drag_and_drop(false),
        ..Default::default()
    };
    eframe::run_native("AntAn", options, Box::new(|cc| Ok(Box::new(app::AntAnApp::new(cc)))))
}
