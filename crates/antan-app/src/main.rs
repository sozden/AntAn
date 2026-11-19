#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod compute;
mod editor;
mod heightscan;
mod stackwin;
mod matchjob;
mod optim_ui;
mod project;
mod shot;
mod suggest;
mod theme;
mod view3d;
mod viz;

fn main() -> eframe::Result {
    // Doğrulama çekimleri için pencere boyutu: ANTAN_AUTOSHOT_SIZE=1560x1500
    let size = std::env::var("ANTAN_AUTOSHOT_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
        .unwrap_or([1560.0, 940.0]);
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("AntAn").with_app_id("antan").with_inner_size(size).with_min_inner_size([1100.0, 700.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native("AntAn", options, Box::new(|cc| Ok(Box::new(app::AntAnApp::new(cc)))))
}

/// Pencere/görev çubuğu simgesi: assets/icon-256.png (tools/make-icons.ps1 üretir).
fn app_icon() -> Option<eframe::egui::IconData> {
    let bytes: &[u8] = include_bytes!("../../../assets/icon-256.png");
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then(|| eframe::egui::IconData { rgba: buf, width: info.width, height: info.height })
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_icon_decodes() {
        let icon = super::app_icon().expect("simge çözülemedi");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Köşe saydam, orta opak.
        assert_eq!(icon.rgba[3], 0);
        assert_eq!(icon.rgba[(128 * 256 + 128) * 4 + 3], 255);
    }
}
