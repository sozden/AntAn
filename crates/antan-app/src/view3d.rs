//! Basit döndürülebilir 3B ışıma diyagramı (ortografik tel kafes görünümü).

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

use antan_core::farfield::{Pattern, direction};

const FLOOR_DB: f64 = -30.0;

fn project(p: [f64; 3], yaw: f32, pitch: f32) -> (f32, f32, f32) {
    let (x, y, z) = (p[0] as f32, p[1] as f32, p[2] as f32);
    // Önce yaw (Z ekseni etrafında, bum X'ten görünür), sonra pitch (Y ekseni).
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (x1, y1) = (x * cy - y * sy, x * sy + y * cy);
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (x2, z2) = (x1 * cp - z * sp, x1 * sp + z * cp);
    (x2, y1, z2) // ekrana: sağ = x2, yukarı = z2 (y1 derinlik: görünürlük sırası için)
}

/// Kazanç örüntüsünü yarıçap olarak kullanan tel kafes küre çizer.
/// Döndürme için fare sürüklemesini destekler; `yaw`/`pitch` kalıcı durumdur.
pub fn show(ui: &mut egui::Ui, size: Vec2, pattern: &Pattern, max_gain_db: f64, yaw: &mut f32, pitch: &mut f32) {
    let (resp, painter) = ui.allocate_painter(size, Sense::drag());
    if resp.dragged() {
        *yaw += resp.drag_delta().x * 0.01;
        *pitch = (*pitch - resp.drag_delta().y * 0.01).clamp(-1.55, 1.55);
    }
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let center = rect.center();
    let scale = rect.width().min(rect.height()) * 0.42;

    let r_of = |db: f64| ((db - FLOOR_DB) / (0.0 - FLOOR_DB)).clamp(0.0, 1.0);
    let point = |it: usize, ip: usize| -> ([f64; 3], f32) {
        let g_db = 10.0 * pattern.at(it, ip).max(1e-30).log10() - max_gain_db;
        let r = r_of(g_db);
        let d = direction(pattern.theta(it), pattern.phi(ip));
        ([d[0] * r, d[1] * r, d[2] * r], *yaw)
    };
    let to_screen = |p: [f64; 3]| -> (Pos2, f32) {
        let (x, y, z) = project(p, *yaw, *pitch);
        (center + Vec2::new(x, -z) * scale, y)
    };

    let accent = ui.visuals().selection.bg_fill;
    let axis_col = ui.visuals().weak_text_color();
    // Eksenler: X (bum/ileri, kırmızımsı), Y (eleman), Z (yukarı).
    for (dir, col, label) in [([1.0, 0.0, 0.0], Color32::from_rgb(210, 90, 70), "İleri (X)"), ([0.0, 1.0, 0.0], axis_col, "Eleman (Y)"), ([0.0, 0.0, 1.0], axis_col, "Yukarı (Z)")] {
        let (p0, _) = to_screen([0.0, 0.0, 0.0]);
        let (p1, _) = to_screen(scale3(dir, 1.15));
        painter.line_segment([p0, p1], Stroke::new(1.2, col));
        painter.text(p1, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(10.0), col);
    }

    // φ-çemberleri (sabit θ) ve θ-çemberleri (sabit φ) olarak tel kafes.
    let mut quads: Vec<(f32, [Pos2; 2])> = Vec::new();
    for it in 0..pattern.n_theta {
        for ip in 0..pattern.n_phi {
            let (p_a, _) = point(it, ip);
            let (p_b, _) = point(it, (ip + 1) % pattern.n_phi);
            let (sa, da) = to_screen(p_a);
            let (sb, db) = to_screen(p_b);
            quads.push(((da + db) * 0.5, [sa, sb]));
        }
    }
    for it in 0..pattern.n_theta.saturating_sub(1) {
        for ip in 0..pattern.n_phi {
            let (p_a, _) = point(it, ip);
            let (p_b, _) = point(it + 1, ip);
            let (sa, da) = to_screen(p_a);
            let (sb, db) = to_screen(p_b);
            quads.push(((da + db) * 0.5, [sa, sb]));
        }
    }
    quads.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (depth, seg) in &quads {
        let shade = ((*depth + scale) / (2.0 * scale)).clamp(0.15, 1.0);
        painter.line_segment(*seg, Stroke::new(1.0, accent.gamma_multiply(shade)));
    }

    painter.text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, "Sürükle: döndür", egui::FontId::proportional(10.0), ui.visuals().weak_text_color());
}

fn scale3(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

#[allow(dead_code)]
fn clip(rect: Rect, p: Pos2) -> Pos2 {
    p.clamp(rect.min, rect.max)
}
