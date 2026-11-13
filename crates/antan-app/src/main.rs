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
    // Doğrulama çekimleri için pencere boyutu: ANTAN_AUTOSHOT_SIZE=1560x1500
    let size = std::env::var("ANTAN_AUTOSHOT_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
        .unwrap_or([1560.0, 940.0]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_title("AntAn").with_inner_size(size).with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native("AntAn", options, Box::new(|cc| Ok(Box::new(app::AntAnApp::new(cc)))))
}
