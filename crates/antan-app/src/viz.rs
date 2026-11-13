//! Kutupsal ışıma kesitleri ve SWR taraması için çizim yardımcıları.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};

use antan_core::matching::swr;
use antan_core::solver::SweepPoint;

const RINGS_DB: [f64; 4] = [-20.0, -10.0, -3.0, 0.0];
const FLOOR_DB: f64 = -30.0;

/// `cut`: (açı rad, doğrusal kazanç). `angle_label` 0° konumunun anlamı.
pub fn polar_plot(ui: &mut egui::Ui, size: Vec2, title: &str, cut: &[(f64, f64)], max_gain_db: f64, labels: [&str; 4]) {
    ui.vertical(|ui| {
        ui.label(title);
        let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
        let rect = resp.rect;
        let center = rect.center();
        let radius = rect.width().min(rect.height()) * 0.5 - 28.0;
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

        let r_of = |db: f64| radius * ((db - FLOOR_DB) / (0.0 - FLOOR_DB)).clamp(0.0, 1.0) as f32;
        for ring in RINGS_DB {
            painter.circle_stroke(center, r_of(ring), Stroke::new(1.0, ui.visuals().weak_text_color()));
            painter.text(
                center + Vec2::new(2.0, -r_of(ring) - 10.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{ring:.0}"),
                egui::FontId::proportional(9.0),
                ui.visuals().weak_text_color(),
            );
        }
        for (deg, label) in [(0.0, labels[0]), (90.0, labels[1]), (180.0, labels[2]), (270.0, labels[3])] {
            let a = (deg - 90.0f64).to_radians();
            let p = center + Vec2::new(a.cos() as f32, a.sin() as f32) * radius;
            painter.line_segment([center, p], Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.6)));
            let lp = center + Vec2::new(a.cos() as f32, a.sin() as f32) * (radius + 14.0);
            painter.text(lp, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), ui.visuals().text_color());
        }

        let accent = ui.visuals().selection.bg_fill;
        let pts: Vec<Pos2> = cut
            .iter()
            .map(|(ang, g)| {
                let db = 10.0 * g.max(1e-30).log10() - max_gain_db;
                let a = (*ang - std::f64::consts::FRAC_PI_2) as f32;
                center + Vec2::new(a.cos(), a.sin()) * r_of(db)
            })
            .collect();
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.8, accent));
        }
    });
}

pub fn sweep_chart(ui: &mut egui::Ui, size: Vec2, sweep: &[SweepPoint], z0: f64, marker_freq: f64) {
    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    if sweep.len() < 2 {
        return;
    }
    let (f0, f1) = (sweep[0].frequency, sweep[sweep.len() - 1].frequency);
    let swrs: Vec<f64> = sweep.iter().map(|p| swr(p.impedance, z0)).collect();
    let max_swr = swrs.iter().copied().filter(|v| v.is_finite()).fold(1.0, f64::max).max(3.0).min(10.0);

    let pad = Rect::from_min_max(rect.min + Vec2::new(44.0, 8.0), rect.max - Vec2::new(8.0, 22.0));
    painter.rect_stroke(pad, 0.0, Stroke::new(1.0, ui.visuals().weak_text_color()), StrokeKind::Middle);
    let x_of = |f: f64| pad.left() + (pad.width() * ((f - f0) / (f1 - f0)) as f32);
    let y_of = |s: f64| pad.bottom() - (pad.height() * (s.min(max_swr) / max_swr) as f32);

    for level in [1.0, 1.5, 2.0, 3.0] {
        if level > max_swr {
            continue;
        }
        let y = y_of(level);
        let col = if (level - 2.0).abs() < 1e-9 { Color32::from_rgb(214, 90, 70) } else { ui.visuals().weak_text_color() };
        painter.line_segment([Pos2::new(pad.left(), y), Pos2::new(pad.right(), y)], Stroke::new(1.0, col.gamma_multiply(0.7)));
        painter.text(Pos2::new(pad.left() - 4.0, y), egui::Align2::RIGHT_CENTER, format!("{level:.1}"), egui::FontId::proportional(10.0), ui.visuals().text_color());
    }
    for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let f = f0 + (f1 - f0) * frac;
        let x = x_of(f);
        painter.text(Pos2::new(x, pad.bottom() + 4.0), egui::Align2::CENTER_TOP, format!("{:.1}", f / 1.0e6), egui::FontId::proportional(10.0), ui.visuals().text_color());
    }

    let accent = ui.visuals().selection.bg_fill;
    let pts: Vec<Pos2> = sweep.iter().zip(&swrs).map(|(p, s)| Pos2::new(x_of(p.frequency), y_of(*s))).collect();
    for w in pts.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(2.0, accent));
    }
    let mx = x_of(marker_freq);
    painter.line_segment([Pos2::new(mx, pad.top()), Pos2::new(mx, pad.bottom())], Stroke::new(1.0, Color32::from_rgb(230, 180, 60)));
}

/// Tel üzerindeki akım genliği (A) dağılımı, 0..1 normalize edilmiş.
pub fn current_plot(ui: &mut egui::Ui, size: Vec2, dist: &[(f64, f64)]) {
    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    if dist.is_empty() {
        return;
    }
    let maxv = dist.iter().map(|(_, v)| *v).fold(1e-12, f64::max);
    let pts: Vec<Pos2> = dist
        .iter()
        .map(|(t, v)| Pos2::new(rect.left() + rect.width() * *t as f32, rect.bottom() - rect.height() * (v / maxv) as f32 * 0.9 - 4.0))
        .collect();
    painter.line_segment([rect.left_bottom() - Vec2::new(0.0, 0.0), rect.right_bottom()], Stroke::new(1.0, ui.visuals().weak_text_color()));
    let accent = ui.visuals().selection.bg_fill;
    for w in pts.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(2.0, accent));
    }
}
