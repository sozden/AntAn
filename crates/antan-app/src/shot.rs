//! Bir panelin PNG görüntüsünü almak için tam ekran yakalama yardımcıları.
//!
//! egui tüm pencereyi yakalar; bir `egui::Rect` bölgesini kırpıp dosyaya yazar.
//! (Grafik projesindeki aynı desen.)

use eframe::egui;
use std::path::PathBuf;

pub struct PendingShot {
    pub rect: egui::Rect,
    pub path: PathBuf,
    pub after_frame: u64,
    pub sent: bool,
}

#[derive(Default)]
pub struct Shotter {
    pending: Option<PendingShot>,
}

impl Shotter {
    pub fn request(&mut self, ctx: &egui::Context, rect: egui::Rect, path: PathBuf) {
        self.pending = Some(PendingShot { rect, path, after_frame: ctx.cumulative_frame_nr() + 1, sent: false });
        ctx.request_repaint();
    }

    /// Her kare çağrılır; bekleyen yakalama varsa işler ve sonucu döner.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Result<PathBuf, String>> {
        let shot = self.pending.as_mut()?;
        if !shot.sent {
            if ctx.cumulative_frame_nr() > shot.after_frame {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                shot.sent = true;
            }
            ctx.request_repaint();
            return None;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let image = image?;
        let shot = self.pending.take().unwrap();
        let region = image.region(&shot.rect, Some(ctx.pixels_per_point()));
        Some(write_png(&shot.path, &region).map(|_| shot.path))
    }
}

fn write_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), image.size[0] as u32, image.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    w.write_image_data(&bytes).map_err(|e| e.to_string())
}
