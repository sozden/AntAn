//! 2B grafikler: kutupsal kesitler, SWR ve R/X taraması, Smith abağı, akım dağılımı.

use eframe::egui::{self, Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use std::f64::consts::PI;

use antan_core::Complex64;
use antan_core::matching::{reflection, swr};
use antan_core::solver::SweepPoint;

use crate::theme;
use antan_core::i18n::t as tl;
use antan_core::tf;


pub fn fmt_z(z: Complex64) -> String {
    format!("{:.1} {} j{:.1} Ω", z.re, if z.im >= 0.0 { "+" } else { "−" }, z.im.abs())
}

fn small(text: impl ToString, color: Color32) -> (String, FontId, Color32) {
    (text.to_string(), FontId::proportional(10.5), color)
}

fn put(p: &egui::Painter, at: Pos2, align: Align2, t: (String, FontId, Color32)) {
    p.text(at, align, t.0, t.1, t.2);
}

/// Fare imleci yanında okuma kutusu.
fn readout(p: &egui::Painter, area: Rect, at: Pos2, lines: &[String]) {
    let font = FontId::proportional(11.5);
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 6.6 + 14.0;
    let h = lines.len() as f32 * 15.0 + 8.0;
    let mut r = Rect::from_min_size(at + Vec2::new(12.0, 12.0), Vec2::new(w, h));
    if r.right() > area.right() {
        r = r.translate(Vec2::new(-w - 24.0, 0.0));
    }
    if r.bottom() > area.bottom() {
        r = r.translate(Vec2::new(0.0, -h - 24.0));
    }
    p.rect(r, 4.0, Color32::from_rgba_unmultiplied(20, 24, 31, 235), Stroke::new(1.0, theme::GRID_STRONG), StrokeKind::Middle);
    for (i, l) in lines.iter().enumerate() {
        p.text(r.left_top() + Vec2::new(7.0, 4.0 + 15.0 * i as f32), Align2::LEFT_TOP, l, font.clone(), Color32::WHITE);
    }
}

// ------------------------------------------------------------ kutupsal

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Polar {
    /// İleri yukarıda, açı saat yönünün tersine (yukarıdan bakış, +Y solda).
    Azimuth,
    /// İleri sağda, zenit yukarıda.
    Elevation,
}

pub struct PolarInfo {
    pub max_gain_dbi: f64,
    pub beamwidth_deg: f64,
    pub fb_db: f64,
}

/// `cut`: (açı rad, doğrusal kazanç), 0..2π. Ölçek: ana huzme tepesine göre dB.
pub fn polar_plot(ui: &mut egui::Ui, size: Vec2, title: &str, cut: &[(f64, f64)], kind: Polar, range_db: f64, info: &PolarInfo) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    p.text(rect.left_top() + Vec2::new(10.0, 8.0), Align2::LEFT_TOP, title, FontId::proportional(13.0), Color32::WHITE);
    let center = rect.center() + Vec2::new(0.0, 10.0);
    let radius = (rect.width().min(rect.height() - 20.0) * 0.5 - 44.0).max(20.0);
    let screen_angle = |a: f64| -> f64 {
        match kind {
            Polar::Azimuth => PI / 2.0 + a,
            Polar::Elevation => a,
        }
    };
    let at = |a: f64, r: f32| {
        let s = screen_angle(a);
        center + Vec2::new(s.cos() as f32, -(s.sin() as f32)) * r
    };
    let r_of = |db: f64| radius * ((db + range_db) / range_db).clamp(0.0, 1.0) as f32;

    // Halkalar.
    let mut ring = 0.0;
    while ring >= -range_db + 1e-9 {
        p.circle_stroke(center, r_of(ring), Stroke::new(1.0, if ring == 0.0 { theme::GRID_STRONG } else { theme::GRID }));
        put(&p, center + Vec2::new(3.0, -r_of(ring) + 1.0), Align2::LEFT_TOP, small(format!("{ring:.0}"), theme::TEXT_DIM));
        ring -= 10.0;
    }
    // −3 dB kesikli halka.
    let r3 = r_of(-3.0);
    for k in 0..72 {
        if k % 2 == 0 {
            let a0 = k as f64 / 72.0 * 2.0 * PI;
            let a1 = (k + 1) as f64 / 72.0 * 2.0 * PI;
            p.line_segment([at(a0, r3), at(a1, r3)], Stroke::new(1.0, theme::WARM.gamma_multiply(0.6)));
        }
    }
    // Işınlar ve açı etiketleri.
    let names: [&str; 4] = match kind {
        Polar::Azimuth => [tl("FRONT", "İLERİ"), tl("LEFT", "SOL"), tl("BACK", "GERİ"), tl("RIGHT", "SAĞ")],
        Polar::Elevation => [tl("FRONT", "İLERİ"), tl("ZENITH", "ZENİT"), tl("BACK", "GERİ"), tl("NADIR", "NADİR")],
    };
    for k in 0..12 {
        let a = k as f64 * PI / 6.0;
        p.line_segment([center, at(a, radius)], Stroke::new(1.0, theme::GRID));
        // Etiket, ışının dış ucuna; yanlarda taşmaması için hizalama yöne göre.
        let s = screen_angle(a);
        let (cx, cy) = (s.cos(), s.sin());
        let align = match (cx.abs() > 0.5, cx > 0.0, cy > 0.0) {
            (true, true, _) => Align2::LEFT_CENTER,
            (true, false, _) => Align2::RIGHT_CENTER,
            (false, _, true) => Align2::CENTER_BOTTOM,
            (false, _, false) => Align2::CENTER_TOP,
        };
        let pos = at(a, radius + 6.0);
        if k % 3 == 0 {
            put(&p, pos, align, (format!("{} {}°", names[k / 3], k * 30), FontId::proportional(10.5), theme::ACCENT));
        } else {
            put(&p, pos, align, small(format!("{}°", k * 30), theme::TEXT_DIM));
        }
    }

    // Eğri: merkezden yelpaze olarak dolgu + çizgi.
    let db_of = |g: f64| 10.0 * g.max(1e-30).log10() - info.max_gain_dbi;
    let pts: Vec<Pos2> = cut.iter().map(|(a, g)| at(*a, r_of(db_of(*g)))).collect();
    let mut mesh = Mesh::default();
    let fill = Color32::from_rgba_unmultiplied(64, 190, 255, 45);
    mesh.colored_vertex(center, fill);
    for q in &pts {
        mesh.colored_vertex(*q, fill);
    }
    for i in 1..pts.len() as u32 {
        mesh.add_triangle(0, i, i + 1);
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::line(pts, Stroke::new(2.0, theme::ACCENT)));

    // Bilgi.
    let lines = [tf!("Max. {:.2} dBi", "Maks. {:.2} dBi", info.max_gain_dbi), format!("−3 dB: {:.0}°", info.beamwidth_deg), tf!("F/B: {:.1} dB", "Ö/A: {:.1} dB", info.fb_db)];
    for (i, l) in lines.iter().enumerate() {
        put(&p, rect.right_top() + Vec2::new(-10.0, 8.0 + 15.0 * i as f32), Align2::RIGHT_TOP, (l.clone(), FontId::proportional(11.5), if i == 0 { Color32::WHITE } else { theme::TEXT_DIM }));
    }

    // Fare okuması.
    if let Some(hp) = resp.hover_pos() {
        let d = hp - center;
        if d.length() <= radius + 4.0 && !cut.is_empty() {
            let s = (-(d.y as f64)).atan2(d.x as f64);
            let a = match kind {
                Polar::Azimuth => s - PI / 2.0,
                Polar::Elevation => s,
            }
            .rem_euclid(2.0 * PI);
            let i = ((a / (2.0 * PI)) * (cut.len() - 1) as f64).round() as usize;
            let (ang, g) = cut[i.min(cut.len() - 1)];
            let db = db_of(g);
            p.line_segment([center, at(ang, radius)], Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.4)));
            p.circle_filled(at(ang, r_of(db)), 4.0, Color32::WHITE);
            readout(&p, rect, hp, &[format!("{:.0}°", ang.to_degrees()), format!("{:.2} dBi", db + info.max_gain_dbi), tf!("{db:+.2} dB (relative)", "{db:+.2} dB (göreli)")]);
        }
    }
}

// ------------------------------------------------------------ tarama

/// SWR'nin `limit` altında kaldığı (merkezi içeren) frekans aralığı.
pub fn swr_bandwidth(sweep: &[SweepPoint], z0: f64, center: f64, limit: f64) -> Option<(f64, f64)> {
    if sweep.len() < 2 {
        return None;
    }
    let s: Vec<f64> = sweep.iter().map(|p| swr(p.impedance, z0)).collect();
    let ci = sweep.iter().enumerate().min_by(|a, b| (a.1.frequency - center).abs().partial_cmp(&(b.1.frequency - center).abs()).unwrap())?.0;
    // Merkezde sağlanmıyorsa en düşük SWR noktasından başla.
    let ci = if s[ci] < limit { ci } else { s.iter().enumerate().min_by(|a, b| a.1.partial_cmp(b.1).unwrap())?.0 };
    if s[ci] >= limit {
        return None;
    }
    let cross = |i: usize, j: usize| {
        let t = (limit - s[i]) / (s[j] - s[i]);
        sweep[i].frequency + t * (sweep[j].frequency - sweep[i].frequency)
    };
    let mut lo = sweep[0].frequency;
    for i in (1..=ci).rev() {
        if s[i - 1] >= limit {
            lo = cross(i, i - 1);
            break;
        }
    }
    let mut hi = sweep[sweep.len() - 1].frequency;
    for i in ci..sweep.len() - 1 {
        if s[i + 1] >= limit {
            hi = cross(i, i + 1);
            break;
        }
    }
    Some((lo, hi))
}

struct Axes {
    plot: Rect,
    f0: f64,
    f1: f64,
    y0: f64,
    y1: f64,
}

impl Axes {
    fn x(&self, f: f64) -> f32 {
        self.plot.left() + self.plot.width() * ((f - self.f0) / (self.f1 - self.f0)) as f32
    }
    fn y(&self, v: f64) -> f32 {
        self.plot.bottom() - self.plot.height() * ((v.clamp(self.y0, self.y1) - self.y0) / (self.y1 - self.y0)) as f32
    }
    fn f_at(&self, x: f32) -> f64 {
        self.f0 + (self.f1 - self.f0) * ((x - self.plot.left()) / self.plot.width()) as f64
    }
}

fn nice_step(range: f64, target: usize) -> f64 {
    let raw = range / target as f64;
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * mag >= raw {
            return m * mag;
        }
    }
    10.0 * mag
}

fn draw_axes(p: &egui::Painter, ax: &Axes, y_ticks: &[f64], y_fmt: impl Fn(f64) -> String) {
    p.rect_stroke(ax.plot, 0.0, Stroke::new(1.0, theme::GRID_STRONG), StrokeKind::Middle);
    for &v in y_ticks {
        let y = ax.y(v);
        p.line_segment([Pos2::new(ax.plot.left(), y), Pos2::new(ax.plot.right(), y)], Stroke::new(1.0, theme::GRID));
        put(p, Pos2::new(ax.plot.left() - 5.0, y), Align2::RIGHT_CENTER, small(y_fmt(v), theme::TEXT_DIM));
    }
    let step = nice_step((ax.f1 - ax.f0) / 1e6, 6) * 1e6;
    let mut f = (ax.f0 / step).ceil() * step;
    while f <= ax.f1 + 1e-6 {
        let x = ax.x(f);
        p.line_segment([Pos2::new(x, ax.plot.top()), Pos2::new(x, ax.plot.bottom())], Stroke::new(1.0, theme::GRID));
        put(p, Pos2::new(x, ax.plot.bottom() + 4.0), Align2::CENTER_TOP, small(format!("{:.2}", f / 1e6), theme::TEXT_DIM));
        f += step;
    }
}

pub fn swr_chart(ui: &mut egui::Ui, size: Vec2, sweep: &[SweepPoint], z0: f64, design_f: f64, band: Option<(f64, f64)>) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, format!("SWR ({z0:.0} Ω)"), FontId::proportional(13.0), Color32::WHITE);
    if sweep.len() < 2 {
        return;
    }
    let s: Vec<f64> = sweep.iter().map(|q| swr(q.impedance, z0).min(99.0)).collect();
    let ymax = s.iter().copied().fold(1.0, f64::max).clamp(3.0, 10.0);
    let ax = Axes {
        plot: Rect::from_min_max(rect.min + Vec2::new(42.0, 26.0), rect.max - Vec2::new(12.0, 22.0)),
        f0: sweep[0].frequency,
        f1: sweep[sweep.len() - 1].frequency,
        y0: 1.0,
        y1: ymax,
    };
    if let Some((lo, hi)) = band {
        let r = Rect::from_min_max(Pos2::new(ax.x(lo), ax.plot.top()), Pos2::new(ax.x(hi), ax.plot.bottom()));
        p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(90, 205, 125, 22));
    }
    let ticks: Vec<f64> = [1.0, 1.5, 2.0, 3.0, 5.0, 10.0].into_iter().filter(|v| *v <= ymax).collect();
    draw_axes(&p, &ax, &ticks, |v| format!("{v:.1}"));
    let y2 = ax.y(2.0);
    p.line_segment([Pos2::new(ax.plot.left(), y2), Pos2::new(ax.plot.right(), y2)], Stroke::new(1.0, theme::BAD.gamma_multiply(0.6)));
    let pts: Vec<Pos2> = sweep.iter().zip(&s).map(|(q, v)| Pos2::new(ax.x(q.frequency), ax.y(*v))).collect();
    p.add(Shape::line(pts, Stroke::new(2.2, theme::ACCENT)));
    let mx = ax.x(design_f);
    p.line_segment([Pos2::new(mx, ax.plot.top()), Pos2::new(mx, ax.plot.bottom())], Stroke::new(1.0, theme::WARM));
    hover_freq(&p, &resp, &ax, rect, sweep, z0);
}

pub fn rx_chart(ui: &mut egui::Ui, size: Vec2, sweep: &[SweepPoint], z0: f64, design_f: f64) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, tl("Impedance", "Empedans"), FontId::proportional(13.0), Color32::WHITE);
    p.text(rect.left_top() + Vec2::new(90.0, 7.0), Align2::LEFT_TOP, "— R", FontId::proportional(11.5), theme::ACCENT);
    p.text(rect.left_top() + Vec2::new(125.0, 7.0), Align2::LEFT_TOP, "— X", FontId::proportional(11.5), theme::SERIES2);
    if sweep.len() < 2 {
        return;
    }
    let lo = sweep.iter().map(|q| q.impedance.re.min(q.impedance.im)).fold(0.0, f64::min);
    let hi = sweep.iter().map(|q| q.impedance.re.max(q.impedance.im)).fold(z0, f64::max);
    let step = nice_step(hi - lo, 5);
    let (y0, y1) = ((lo / step).floor() * step, (hi / step).ceil() * step);
    let ax = Axes {
        plot: Rect::from_min_max(rect.min + Vec2::new(42.0, 26.0), rect.max - Vec2::new(12.0, 22.0)),
        f0: sweep[0].frequency,
        f1: sweep[sweep.len() - 1].frequency,
        y0,
        y1,
    };
    let mut ticks = Vec::new();
    let mut v = y0;
    while v <= y1 + 1e-9 {
        ticks.push(v);
        v += step;
    }
    draw_axes(&p, &ax, &ticks, |v| format!("{v:.0}"));
    let y_zero = ax.y(0.0);
    p.line_segment([Pos2::new(ax.plot.left(), y_zero), Pos2::new(ax.plot.right(), y_zero)], Stroke::new(1.0, theme::GRID_STRONG));
    let r: Vec<Pos2> = sweep.iter().map(|q| Pos2::new(ax.x(q.frequency), ax.y(q.impedance.re))).collect();
    let x: Vec<Pos2> = sweep.iter().map(|q| Pos2::new(ax.x(q.frequency), ax.y(q.impedance.im))).collect();
    p.add(Shape::line(r, Stroke::new(2.0, theme::ACCENT)));
    p.add(Shape::line(x, Stroke::new(2.0, theme::SERIES2)));
    let mx = ax.x(design_f);
    p.line_segment([Pos2::new(mx, ax.plot.top()), Pos2::new(mx, ax.plot.bottom())], Stroke::new(1.0, theme::WARM));
    hover_freq(&p, &resp, &ax, rect, sweep, z0);
}

fn hover_freq(p: &egui::Painter, resp: &egui::Response, ax: &Axes, rect: Rect, sweep: &[SweepPoint], z0: f64) {
    let Some(hp) = resp.hover_pos() else { return };
    if !ax.plot.contains(hp) {
        return;
    }
    let f = ax.f_at(hp.x);
    let q = sweep.iter().min_by(|a, b| (a.frequency - f).abs().partial_cmp(&(b.frequency - f).abs()).unwrap()).unwrap();
    let x = ax.x(q.frequency);
    p.line_segment([Pos2::new(x, ax.plot.top()), Pos2::new(x, ax.plot.bottom())], Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.5)));
    readout(p, rect, hp, &[format!("{:.3} MHz", q.frequency / 1e6), fmt_z(q.impedance), format!("SWR {:.2}", swr(q.impedance, z0))]);
}

// ------------------------------------------------------------ Smith

pub fn smith_chart(ui: &mut egui::Ui, size: Vec2, sweep: &[SweepPoint], z0: f64, design_f: f64) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, tf!("Smith chart ({z0:.0} Ω)", "Smith abağı ({z0:.0} Ω)"), FontId::proportional(13.0), Color32::WHITE);
    let center = rect.center() + Vec2::new(0.0, 8.0);
    let rad = (rect.width().min(rect.height() - 16.0) * 0.5 - 18.0).max(20.0);
    let g2s = |g: Complex64| center + Vec2::new(g.re as f32, -(g.im as f32)) * rad;
    let z2s = |z: Complex64| g2s((z - 1.0) / (z + 1.0));

    p.circle_filled(center, rad, Color32::from_rgb(20, 24, 31));
    // Sabit direnç çemberleri.
    for r in [0.2, 0.5, 1.0, 2.0, 5.0] {
        let pts: Vec<Pos2> = (0..=200)
            .map(|i| {
                let t = (i as f64 / 200.0 - 0.5) * PI * 0.999;
                z2s(Complex64::new(r, 8.0 * t.tan()))
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(if r == 1.0 { 1.3 } else { 0.8 }, if r == 1.0 { theme::GRID_STRONG } else { theme::GRID })));
        put(&p, z2s(Complex64::new(r, 0.0)) + Vec2::new(2.0, 2.0), Align2::LEFT_TOP, small(format!("{r}"), theme::TEXT_DIM));
    }
    // Sabit reaktans yayları.
    for x in [0.2, 0.5, 1.0, 2.0, 5.0] {
        for sx in [x, -x] {
            let pts: Vec<Pos2> = (0..=160).map(|i| z2s(Complex64::new((i as f64 / 160.0 * 1.55).tan() * 4.0, sx))).collect();
            p.add(Shape::line(pts, Stroke::new(0.8, theme::GRID)));
        }
        put(&p, z2s(Complex64::new(0.0, x)) + Vec2::new(0.0, -8.0), Align2::CENTER_CENTER, small(format!("+j{x}"), theme::TEXT_DIM));
        put(&p, z2s(Complex64::new(0.0, -x)) + Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, small(format!("−j{x}"), theme::TEXT_DIM));
    }
    p.line_segment([center - Vec2::new(rad, 0.0), center + Vec2::new(rad, 0.0)], Stroke::new(0.8, theme::GRID));
    p.circle_stroke(center, rad, Stroke::new(1.3, theme::GRID_STRONG));
    // SWR 2 çemberi.
    let r2 = rad / 3.0;
    for k in 0..90 {
        if k % 2 == 0 {
            let a0 = k as f32 / 90.0 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / 90.0 * std::f32::consts::TAU;
            p.line_segment([center + Vec2::angled(a0) * r2, center + Vec2::angled(a1) * r2], Stroke::new(1.0, theme::BAD.gamma_multiply(0.7)));
        }
    }
    put(&p, center + Vec2::new(r2 * 0.72, -r2 * 0.72), Align2::LEFT_BOTTOM, small("SWR 2", theme::BAD));

    if sweep.len() < 2 {
        return;
    }
    let pts: Vec<Pos2> = sweep.iter().map(|q| g2s(reflection(q.impedance, z0))).collect();
    p.add(Shape::line(pts.clone(), Stroke::new(2.2, theme::ACCENT)));
    p.circle_filled(pts[0], 4.0, theme::ACCENT);
    p.circle_stroke(pts[pts.len() - 1], 4.5, Stroke::new(1.5, theme::ACCENT));
    put(&p, pts[0] + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, small(format!("{:.2}", sweep[0].frequency / 1e6), theme::ACCENT));
    put(&p, pts[pts.len() - 1] + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, small(format!("{:.2}", sweep[sweep.len() - 1].frequency / 1e6), theme::ACCENT));
    if let Some(q) = sweep.iter().min_by(|a, b| (a.frequency - design_f).abs().partial_cmp(&(b.frequency - design_f).abs()).unwrap()) {
        p.circle_filled(g2s(reflection(q.impedance, z0)), 5.0, theme::WARM);
    }
    if let Some(hp) = resp.hover_pos() {
        if let Some((i, d)) = pts.iter().enumerate().map(|(i, q)| (i, q.distance(hp))).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            && d < 20.0
        {
            let q = sweep[i];
            p.circle_stroke(pts[i], 6.0, Stroke::new(1.5, Color32::WHITE));
            readout(&p, rect, hp, &[format!("{:.3} MHz", q.frequency / 1e6), fmt_z(q.impedance), format!("SWR {:.2}", swr(q.impedance, z0))]);
        }
    }
}

// ------------------------------------------------------------ akım

/// Tellerin akım genliği dağılımı: (ad, [(t, |I|)]), aynı ölçekte.
pub fn current_plot(ui: &mut egui::Ui, size: Vec2, series: &[(String, Vec<(f64, f64)>)], highlight: Option<usize>) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, tl("Current magnitude (along each element, tip to tip)", "Akım genliği (eleman boyunca, uçtan uca)"), FontId::proportional(13.0), Color32::WHITE);
    let plot = Rect::from_min_max(rect.min + Vec2::new(12.0, 26.0), rect.max - Vec2::new(12.0, 10.0));
    p.rect_stroke(plot, 0.0, Stroke::new(1.0, theme::GRID), StrokeKind::Middle);
    let maxv = series.iter().flat_map(|s| s.1.iter().map(|v| v.1)).fold(1e-15, f64::max);
    for (k, (name, pts)) in series.iter().enumerate() {
        let col = if Some(k) == highlight { theme::WARM } else { theme::colormap(0.15 + 0.8 * k as f32 / series.len().max(1) as f32) };
        let line: Vec<Pos2> = pts.iter().map(|(t, v)| Pos2::new(plot.left() + plot.width() * *t as f32, plot.bottom() - plot.height() * (v / maxv) as f32 * 0.92)).collect();
        p.add(Shape::line(line, Stroke::new(if Some(k) == highlight { 2.6 } else { 1.6 }, col)));
        p.text(plot.right_top() + Vec2::new(-6.0, 4.0 + 13.0 * k as f32), Align2::RIGHT_TOP, name, FontId::proportional(10.0), col);
    }
}
