//! Döndürülebilir 3B ışıma diyagramı: aydınlatmalı, kazanca göre renklendirilmiş
//! dolu yüzey + ortada anten geometrisi (akım genliğine göre renkli).
//!
//! Kamera: önce Z ekseni etrafında `yaw`, sonra ekran yatay ekseni etrafında
//! `pitch` (pozitif = yukarıdan bakış). Kamera uzayı (sağ, derinlik, yukarı).

use eframe::egui::{self, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use antan_core::farfield::{Pattern, direction};
use antan_core::{Antenna, Solution};

use crate::theme;
use antan_core::i18n::t as tl;


pub struct View3d {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub range_db: f64,
    pub show_pattern: bool,
    pub show_antenna: bool,
    pub show_grid: bool,
    pub show_currents: bool,
}

impl Default for View3d {
    fn default() -> Self {
        Self { yaw: -0.65, pitch: 0.45, zoom: 1.0, range_db: 30.0, show_pattern: true, show_antenna: true, show_grid: true, show_currents: true }
    }
}

type V3 = [f32; 3];

fn cam(p: [f64; 3], yaw: f32, pitch: f32) -> V3 {
    let (x, y, z) = (p[0] as f32, p[1] as f32, p[2] as f32);
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (x1, y1) = (x * cy - y * sy, x * sy + y * cy);
    let (cp, sp) = (pitch.cos(), pitch.sin());
    [x1, y1 * cp - z * sp, z * cp + y1 * sp]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: V3) -> V3 {
    let n = dot(a, a).sqrt().max(1e-12);
    [a[0] / n, a[1] / n, a[2] / n]
}

pub fn toolbar(ui: &mut egui::Ui, v: &mut View3d) {
    ui.horizontal(|ui| {
        ui.toggle_value(&mut v.show_pattern, tl("Pattern", "Diyagram"));
        ui.toggle_value(&mut v.show_antenna, tl("Antenna", "Anten"));
        ui.toggle_value(&mut v.show_currents, tl("Current colors", "Akım renkleri"));
        ui.toggle_value(&mut v.show_grid, tl("Grid", "Izgara"));
        ui.separator();
        ui.label(tl("View:", "Bakış:"));
        for (label, yaw, pitch) in [(tl("3D", "3B"), -0.65, 0.45), (tl("Top", "Üst"), 0.0, 1.5707), (tl("Side", "Yan"), 0.0, 0.0), (tl("Front", "Ön"), -1.5708, 0.0)] {
            if ui.small_button(label).clicked() {
                v.yaw = yaw;
                v.pitch = pitch;
            }
        }
        ui.separator();
        ui.label(tl("Range", "Aralık"));
        ui.add(egui::DragValue::new(&mut v.range_db).range(10.0..=60.0).speed(0.5).suffix(" dB"));
    });
}

/// `boom`: varsa bumun iki ucu (m), çizim için.
#[allow(clippy::too_many_arguments)]
pub fn show(ui: &mut egui::Ui, size: Vec2, pattern: Option<&Pattern>, max_gain_dbi: f64, antenna: &Antenna, sol: Option<&Solution>, highlight: &[usize], boom: Option<([f64; 3], [f64; 3])>, v: &mut View3d) {
    let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    if resp.dragged() {
        v.yaw += resp.drag_delta().x * 0.01;
        v.pitch = (v.pitch + resp.drag_delta().y * 0.01).clamp(-1.5708, 1.5708);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            v.zoom = (v.zoom * (1.0 + scroll * 0.002)).clamp(0.3, 4.0);
        }
    }
    if resp.double_clicked() {
        let range = v.range_db;
        *v = View3d { range_db: range, ..Default::default() };
    }
    let rect = resp.rect;
    painter.rect_filled(rect, 6.0, theme::PLOT_BG);
    let plot = Rect::from_min_max(rect.min, rect.max - Vec2::new(70.0, 0.0));
    let center = plot.center();
    let scale = plot.width().min(plot.height()) * 0.40 * v.zoom;
    let to_screen = |c: V3| center + Vec2::new(c[0], -c[2]) * scale;
    let (yaw, pitch) = (v.yaw, v.pitch);

    // Zemin ızgarası (XY düzlemi, birim çember halkaları).
    if v.show_grid {
        for ring in [0.333f64, 0.667, 1.0] {
            let pts: Vec<Pos2> = (0..=72)
                .map(|i| {
                    let a = i as f64 / 72.0 * std::f64::consts::TAU;
                    to_screen(cam([ring * a.cos(), ring * a.sin(), 0.0], yaw, pitch))
                })
                .collect();
            painter.add(Shape::line(pts, Stroke::new(1.0, theme::GRID)));
        }
        for k in 0..12 {
            let a = k as f64 / 12.0 * std::f64::consts::TAU;
            painter.line_segment([to_screen(cam([0.0; 3], yaw, pitch)), to_screen(cam([a.cos(), a.sin(), 0.0], yaw, pitch))], Stroke::new(1.0, theme::GRID));
        }
    }

    if !antenna.ground.is_free() {
        // Yarı saydam zemin diski (z = 0).
        let mut mesh = Mesh::default();
        let col = Color32::from_rgba_unmultiplied(120, 92, 60, 90);
        mesh.colored_vertex(to_screen(cam([0.0; 3], yaw, pitch)), col);
        for i in 0..=72 {
            let a = i as f64 / 72.0 * std::f64::consts::TAU;
            mesh.colored_vertex(to_screen(cam([1.15 * a.cos(), 1.15 * a.sin(), 0.0], yaw, pitch)), col);
            if i > 0 {
                mesh.add_triangle(0, i as u32, i as u32 + 1);
            }
        }
        painter.add(Shape::mesh(mesh));
    }

    if let (true, Some(pat)) = (v.show_pattern, pattern) {
        draw_surface(&painter, pat, max_gain_dbi, v, &to_screen, v.show_antenna);
    }

    // Eksenler.
    for (d, col, label) in [([1.25, 0.0, 0.0], theme::BAD, tl("X front", "X ileri")), ([0.0, 1.25, 0.0], theme::GOOD, "Y"), ([0.0, 0.0, 1.25], theme::ACCENT, "Z")] {
        let p0 = to_screen(cam([0.0; 3], yaw, pitch));
        let p1 = to_screen(cam(d, yaw, pitch));
        painter.line_segment([p0, p1], Stroke::new(1.0, col.gamma_multiply(0.7)));
        painter.text(p1, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), col);
    }

    if v.show_antenna {
        draw_antenna(&painter, antenna, sol, highlight, boom, v, &to_screen);
    }

    // Köşe eksen göstergesi.
    let g0 = rect.left_bottom() + Vec2::new(36.0, -36.0);
    for (d, col, l) in [([1.0, 0.0, 0.0], theme::BAD, "X"), ([0.0, 1.0, 0.0], theme::GOOD, "Y"), ([0.0, 0.0, 1.0], theme::ACCENT, "Z")] {
        let c = cam(d, yaw, pitch);
        let p = g0 + Vec2::new(c[0], -c[2]) * 24.0;
        painter.line_segment([g0, p], Stroke::new(2.0, col));
        painter.text(p + Vec2::new(c[0], -c[2]) * 8.0, egui::Align2::CENTER_CENTER, l, egui::FontId::proportional(10.0), col);
    }

    if v.show_pattern && pattern.is_some() {
        color_bar(&painter, Rect::from_min_max(rect.right_top() + Vec2::new(-56.0, 30.0), rect.right_bottom() + Vec2::new(-42.0, -30.0)), max_gain_dbi, v.range_db);
    }
    painter.text(
        rect.left_top() + Vec2::new(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        tl("Drag: rotate · Wheel: zoom · Double-click: reset", "Sürükle: döndür · Tekerlek: yakınlaştır · Çift tık: sıfırla"),
        egui::FontId::proportional(11.0),
        theme::TEXT_DIM,
    );
}

fn draw_surface(painter: &egui::Painter, pat: &Pattern, max_gain_dbi: f64, v: &View3d, to_screen: &dyn Fn(V3) -> Pos2, translucent: bool) {
    let (nt, np) = (pat.n_theta, pat.n_phi);
    let range = v.range_db;
    // Köşe noktaları: kamera uzayı ve normalize yarıçap.
    let mut pts = Vec::with_capacity(nt * np);
    let mut rs = Vec::with_capacity(nt * np);
    for it in 0..nt {
        for ip in 0..np {
            let db = 10.0 * pat.at(it, ip).max(1e-30).log10() - max_gain_dbi;
            let r = ((db + range) / range).clamp(0.0, 1.0);
            let d = direction(pat.theta(it), pat.phi(ip));
            pts.push(cam([d[0] * r, d[1] * r, d[2] * r], v.yaw, v.pitch));
            rs.push(r as f32);
        }
    }
    let idx = |it: usize, ip: usize| it * np + ip % np;
    let light = normalize([-0.35, -0.75, 0.55]);
    let mut quads: Vec<(f32, [usize; 4], f32)> = Vec::with_capacity((nt - 1) * np);
    for it in 0..nt - 1 {
        for ip in 0..np {
            let q = [idx(it, ip), idx(it, ip + 1), idx(it + 1, ip + 1), idx(it + 1, ip)];
            let (a, b, c, d) = (pts[q[0]], pts[q[1]], pts[q[2]], pts[q[3]]);
            let depth = (a[1] + b[1] + c[1] + d[1]) * 0.25;
            let mut n = normalize(cross(sub(c, a), sub(d, b)));
            let centroid = [(a[0] + c[0]) * 0.5, (a[1] + c[1]) * 0.5, (a[2] + c[2]) * 0.5];
            if dot(n, centroid) < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            let diffuse = dot(n, light).max(0.0);
            let spec = dot(n, normalize([light[0], light[1] - 1.0, light[2]])).max(0.0).powi(24);
            quads.push((depth, q, 0.38 + 0.62 * diffuse + 0.35 * spec));
        }
    }
    // Ressam algoritması: uzaktan yakına.
    quads.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let alpha = if translucent { 215 } else { 255 };
    let mut mesh = Mesh::default();
    for (_, q, light) in &quads {
        let base = mesh.vertices.len() as u32;
        for &k in q {
            let c = theme::shade(theme::colormap(rs[k]), *light);
            mesh.colored_vertex(to_screen(pts[k]), Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha));
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(Shape::mesh(mesh));

    if v.show_grid {
        // Seyrek tel kafes çizgileri (her 30° φ, her 30° θ).
        let stroke = Stroke::new(0.6, Color32::from_rgba_unmultiplied(0, 0, 0, 70));
        let step_p = (np / 12).max(1);
        let step_t = ((nt - 1) / 6).max(1);
        for ip in (0..np).step_by(step_p) {
            let line: Vec<Pos2> = (0..nt).map(|it| to_screen(pts[idx(it, ip)])).collect();
            painter.add(Shape::line(line, stroke));
        }
        for it in (step_t..nt - 1).step_by(step_t) {
            let line: Vec<Pos2> = (0..=np).map(|ip| to_screen(pts[idx(it, ip)])).collect();
            painter.add(Shape::line(line, stroke));
        }
    }
}

fn draw_antenna(painter: &egui::Painter, ant: &Antenna, sol: Option<&Solution>, highlight: &[usize], boom: Option<([f64; 3], [f64; 3])>, v: &View3d, to_screen: &dyn Fn(V3) -> Pos2) {
    if ant.wires.is_empty() {
        return;
    }
    let (mut lo, hi) = ant.bounds();
    let grounded = !ant.ground.is_free();
    if grounded {
        lo[2] = 0.0;
    }
    // Zemin varken z = 0 zemin diskinde kalır; anten gerçek yüksekliğinde çizilir.
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, if grounded { 0.0 } else { (lo[2] + hi[2]) * 0.5 }];
    let ext = (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f64::max).max(1e-6);
    let s = 0.9 / ext;
    let map = |p: [f64; 3]| to_screen(cam([(p[0] - c[0]) * s, (p[1] - c[1]) * s, (p[2] - c[2]) * s], v.yaw, v.pitch));
    if let Some((b0, b1)) = boom {
        painter.line_segment([map(b0), map(b1)], Stroke::new(5.0, Color32::from_rgb(120, 128, 140)));
    }
    let max_i = sol.map(|s| s.max_current()).unwrap_or(1.0).max(1e-15);

    for (wi, w) in ant.wires.iter().enumerate() {
        let hl = highlight.contains(&wi);
        let width = if hl { 4.0 } else { 2.6 };
        match (v.show_currents, sol) {
            (true, Some(sol)) if sol.wire_count == ant.wires.len() => {
                let cur = sol.wire_currents(wi);
                for pair in cur.windows(2) {
                    let (t0, i0) = pair[0];
                    let (t1, i1) = pair[1];
                    let p = |t: f64| [w.start[0] + (w.end[0] - w.start[0]) * t, w.start[1] + (w.end[1] - w.start[1]) * t, w.start[2] + (w.end[2] - w.start[2]) * t];
                    let mag = (0.5 * (i0.norm() + i1.norm()) / max_i) as f32;
                    let col = if hl { theme::WARM } else { theme::colormap(0.15 + 0.85 * mag) };
                    painter.line_segment([map(p(t0)), map(p(t1))], Stroke::new(width, col));
                }
            }
            _ => {
                let col = if hl { theme::WARM } else { Color32::from_rgb(220, 226, 235) };
                painter.line_segment([map(w.start), map(w.end)], Stroke::new(width, col));
            }
        }
    }
    if let Some(sol) = sol {
        // Toplu yükler: mor kare (bobin/tuzak).
        for l in &sol.loads {
            let p = map(l.pos);
            painter.rect_filled(egui::Rect::from_center_size(p, Vec2::splat(9.0)), 2.0, Color32::from_rgb(190, 120, 255));
            painter.rect_stroke(egui::Rect::from_center_size(p, Vec2::splat(12.0)), 2.0, Stroke::new(1.2, Color32::WHITE), egui::StrokeKind::Middle);
        }
        for f in &sol.feeds {
            let p = map(f.pos);
            painter.circle_filled(p, 5.0, theme::WARM);
            painter.circle_stroke(p, 7.5, Stroke::new(1.5, theme::WARM));
        }
    }
}

fn color_bar(painter: &egui::Painter, r: Rect, max_gain_dbi: f64, range: f64) {
    let mut mesh = Mesh::default();
    let n = 32;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let y = r.bottom() - r.height() * t;
        mesh.colored_vertex(Pos2::new(r.left(), y), theme::colormap(t));
        mesh.colored_vertex(Pos2::new(r.right(), y), theme::colormap(t));
        if i > 0 {
            let b = (2 * i) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::GRID_STRONG), egui::StrokeKind::Middle);
    let mut ticks = vec![0.0, -3.0, -10.0, -20.0, -30.0, -40.0, -50.0];
    ticks.retain(|d| -d <= range);
    for db in ticks {
        let t = ((db + range) / range) as f32;
        let y = r.bottom() - r.height() * t;
        painter.line_segment([Pos2::new(r.right(), y), Pos2::new(r.right() + 4.0, y)], Stroke::new(1.0, theme::TEXT_DIM));
        painter.text(Pos2::new(r.right() + 6.0, y), egui::Align2::LEFT_CENTER, format!("{db:.0}"), egui::FontId::proportional(10.0), theme::TEXT_DIM);
    }
    painter.text(r.center_top() - Vec2::new(0.0, 6.0), egui::Align2::CENTER_BOTTOM, format!("{max_gain_dbi:.2} dBi"), egui::FontId::proportional(11.0), Color32::WHITE);
    painter.text(r.center_bottom() + Vec2::new(0.0, 6.0), egui::Align2::CENTER_TOP, "dB", egui::FontId::proportional(10.0), theme::TEXT_DIM);
}
