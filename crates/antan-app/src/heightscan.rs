//! Yükseklik taraması penceresi: kazanç ve kalkış açısı yüksekliğe göre.
//! Hesap arka planda (paralel) yapılır; grafikte tıklanan yükseklik modele uygulanır.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};

use antan_core::Antenna;
use antan_core::i18n::t as tl;
use antan_core::matching;
use antan_core::scan::{HeightRow, height_scan};
use antan_core::tf;

use crate::theme;
use crate::viz::fmt_z;

pub struct HeightScan {
    pub open: bool,
    /// Tarama aralığı, dalga boyu cinsinden.
    from_wl: f64,
    to_wl: f64,
    points: usize,
    result: Arc<Mutex<Option<Vec<HeightRow>>>>,
    done: Arc<AtomicUsize>,
    total: usize,
    running: bool,
    lambda: f64,
}

impl Default for HeightScan {
    fn default() -> Self {
        Self { open: false, from_wl: 0.1, to_wl: 2.0, points: 39, result: Arc::default(), done: Arc::default(), total: 0, running: false, lambda: 1.0 }
    }
}

impl HeightScan {
    pub fn busy(&self) -> bool {
        self.running && self.result.lock().unwrap().is_none()
    }

    pub fn start(&mut self, ctx: &egui::Context, antenna: Antenna) {
        let lambda = antan_core::C0 / antenna.frequency;
        self.lambda = lambda;
        let n = self.points.max(2);
        let heights: Vec<f64> = (0..n).map(|i| (self.from_wl + (self.to_wl - self.from_wl) * i as f64 / (n - 1) as f64) * lambda).collect();
        self.total = heights.len();
        self.done.store(0, Ordering::Relaxed);
        *self.result.lock().unwrap() = None;
        self.running = true;
        let (result, done, ctx) = (self.result.clone(), self.done.clone(), ctx.clone());
        std::thread::spawn(move || {
            let ctx2 = ctx.clone();
            let rows = height_scan(&antenna, &heights, &|| {
                done.fetch_add(1, Ordering::Relaxed);
                ctx2.request_repaint();
            });
            *result.lock().unwrap() = Some(rows);
            ctx.request_repaint();
        });
    }

    /// Pencereyi çizer. Kullanıcı bir yükseklik seçerse (en alt nokta, m) döner.
    pub fn show(&mut self, ctx: &egui::Context, antenna: &Antenna, z0: f64, current_height: f64) -> Option<f64> {
        if !self.open {
            return None;
        }
        let mut pick = None;
        let mut open = self.open;
        let grounded = !antenna.ground.is_free();
        egui::Window::new(tl("Height scan", "Yükseklik taraması")).open(&mut open).default_size([820.0, 640.0]).resizable(true).show(ctx, |ui| {
            ui.label(RichText::new(tl("Gain, takeoff angle and feed impedance versus the height of the antenna's lowest point (Yagi: boom height) over the selected ground. Click the chart to apply a height.", "Seçili zemin üzerinde, antenin en alt noktasının (Yagi'de bum) yüksekliğine göre kazanç, kalkış açısı ve besleme empedansı. Grafiğe tıklayınca o yükseklik modele uygulanır.")).small().color(theme::TEXT_DIM));
            if !grounded {
                ui.label(RichText::new(tl("Select perfect or real ground first (Ground and height section).", "Önce kusursuz ya da gerçek zemin seçin (Zemin ve yükseklik bölümü).")).color(theme::WARM));
                return;
            }
            let lambda = antan_core::C0 / antenna.frequency;
            ui.horizontal_wrapped(|ui| {
                ui.label(tl("From", "Başlangıç"));
                ui.add(egui::DragValue::new(&mut self.from_wl).range(0.02..=10.0).speed(0.01).suffix(" λ"));
                ui.label(tl("to", "bitiş"));
                ui.add(egui::DragValue::new(&mut self.to_wl).range(0.05..=20.0).speed(0.01).suffix(" λ"));
                ui.label(RichText::new(format!("({:.1} … {:.1} m)", self.from_wl * lambda, self.to_wl * lambda)).color(theme::TEXT_DIM));
                ui.label(tl("points", "nokta"));
                ui.add(egui::DragValue::new(&mut self.points).range(5..=200));
                let busy = self.running && self.result.lock().unwrap().is_none();
                if ui.add_enabled(!busy, egui::Button::new(RichText::new(tl("▶ Scan", "▶ Tara")).strong()).fill(theme::ACCENT_DIM)).clicked() {
                    if self.to_wl <= self.from_wl {
                        self.to_wl = self.from_wl + 0.5;
                    }
                    self.start(ctx, antenna.clone());
                }
                if busy {
                    ui.spinner();
                    ui.label(format!("{}/{}", self.done.load(Ordering::Relaxed), self.total));
                }
            });
            let rows = self.result.lock().unwrap().clone();
            let Some(rows) = rows else { return };
            if rows.is_empty() {
                ui.label(RichText::new(tl("No height could be solved (e.g. a ground-mounted antenna loses its ground connection when lifted).", "Hiçbir yükseklik çözülemedi (ör. zemine bağlı anten kaldırılınca zemin bağlantısını yitirir).")).color(theme::WARM));
                return;
            }
            let best = rows.iter().max_by(|a, b| a.gain_dbi.total_cmp(&b.gain_dbi)).copied();
            if let Some(b) = best {
                ui.label(RichText::new(tf!("Highest gain: {:.2} dBi at {:.2} m ({:.2} λ), takeoff {:.0}°", "En yüksek kazanç: {:.2} dBi, {:.2} m'de ({:.2} λ), kalkış {:.0}°", b.gain_dbi, b.height, b.height / self.lambda, b.takeoff_deg)).color(theme::GOOD));
            }
            let h = (ui.available_height() * 0.55).max(200.0);
            if let Some(hh) = chart(ui, &rows, Vec2::new(ui.available_width(), h), self.lambda, current_height) {
                pick = Some(hh);
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                egui::Grid::new("hs_table").striped(true).num_columns(7).spacing([14.0, 2.0]).show(ui, |ui| {
                    for t in [tl("Height", "Yükseklik"), "λ", tl("Gain", "Kazanç"), tl("Takeoff", "Kalkış"), tl("F/B", "Ö/A"), "Z", "SWR"] {
                        ui.label(RichText::new(t).small().color(theme::TEXT_DIM));
                    }
                    ui.label("");
                    ui.end_row();
                    for r in &rows {
                        let swr = matching::swr(r.impedance, z0);
                        ui.label(format!("{:.2} m", r.height));
                        ui.label(format!("{:.2}", r.height / self.lambda));
                        ui.label(format!("{:.2} dBi", r.gain_dbi));
                        ui.label(format!("{:.0}°", r.takeoff_deg));
                        ui.label(if r.front_to_back_db.abs() < 0.05 { "—".to_string() } else { format!("{:.1} dB", r.front_to_back_db) });
                        ui.label(fmt_z(r.impedance));
                        ui.label(RichText::new(format!("{swr:.2}")).color(theme::swr_color(swr)));
                        if ui.small_button(tl("apply", "uygula")).clicked() {
                            pick = Some(r.height);
                        }
                        ui.end_row();
                    }
                });
            });
        });
        self.open = open;
        pick
    }
}

/// Kazanç (sol eksen, mavi) ve kalkış açısı (sağ eksen, turuncu) — yüksekliğe göre.
fn chart(ui: &mut egui::Ui, rows: &[HeightRow], size: Vec2, lambda: f64, current: f64) -> Option<f64> {
    let (resp, p) = ui.allocate_painter(size, Sense::click());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    let plot = Rect::from_min_max(rect.min + Vec2::new(54.0, 26.0), rect.max - Vec2::new(50.0, 30.0));
    let (h0, h1) = (rows[0].height, rows[rows.len() - 1].height.max(rows[0].height + 1e-9));
    let gmax = rows.iter().map(|r| r.gain_dbi).fold(f64::NEG_INFINITY, f64::max).ceil();
    let gmin = rows.iter().map(|r| r.gain_dbi).fold(f64::INFINITY, f64::min).floor().min(gmax - 1.0);
    let (tmin, tmax) = (0.0, 90.0);
    let x = |h: f64| plot.left() + ((h - h0) / (h1 - h0)) as f32 * plot.width();
    let yg = |g: f64| plot.bottom() - ((g - gmin) / (gmax - gmin)) as f32 * plot.height();
    let yt = |t: f64| plot.bottom() - ((t - tmin) / (tmax - tmin)) as f32 * plot.height();
    // Izgara ve eksen yazıları.
    for k in 0..=5 {
        let g = gmin + (gmax - gmin) * k as f64 / 5.0;
        let y = yg(g);
        p.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0, theme::GRID));
        p.text(Pos2::new(plot.left() - 6.0, y), Align2::RIGHT_CENTER, format!("{g:.1}"), theme::plot_font(10.0), theme::ACCENT);
        let t = tmin + (tmax - tmin) * k as f64 / 5.0;
        p.text(Pos2::new(plot.right() + 6.0, yt(t)), Align2::LEFT_CENTER, format!("{t:.0}°"), theme::plot_font(10.0), theme::WARM);
    }
    // Yükseklik ekseni: λ adımları.
    let step = if (h1 - h0) / lambda > 4.0 { 1.0 } else if (h1 - h0) / lambda > 1.5 { 0.5 } else { 0.25 };
    let mut k = (h0 / lambda / step).ceil() * step;
    while k * lambda <= h1 + 1e-9 {
        let xx = x(k * lambda);
        p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.0, theme::GRID));
        p.text(Pos2::new(xx, plot.bottom() + 4.0), Align2::CENTER_TOP, format!("{k:.2} λ\n{:.1} m", k * lambda), theme::plot_font(9.5), theme::TEXT_DIM);
        k += step;
    }
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, tl("Gain (dBi)", "Kazanç (dBi)"), theme::plot_font(12.0), theme::ACCENT);
    p.text(rect.right_top() + Vec2::new(-10.0, 6.0), Align2::RIGHT_TOP, tl("Takeoff angle", "Kalkış açısı"), theme::plot_font(12.0), theme::WARM);
    p.add(Shape::line(rows.iter().map(|r| Pos2::new(x(r.height), yg(r.gain_dbi))).collect(), Stroke::new(2.0, theme::ACCENT)));
    p.add(Shape::line(rows.iter().map(|r| Pos2::new(x(r.height), yt(r.takeoff_deg))).collect(), Stroke::new(2.0, theme::WARM)));
    if current >= h0 && current <= h1 {
        let xx = x(current);
        p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.5, theme::GOOD));
    }
    // Fareyle okuma ve tıklayınca seçim.
    let hover = resp.hover_pos().filter(|pos| plot.expand(4.0).contains(*pos))?;
    let hh = h0 + ((hover.x - plot.left()) / plot.width()).clamp(0.0, 1.0) as f64 * (h1 - h0);
    let r = rows.iter().min_by(|a, b| (a.height - hh).abs().total_cmp(&(b.height - hh).abs()))?;
    let xx = x(r.height);
    p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.0, Color32::WHITE));
    p.circle_filled(Pos2::new(xx, yg(r.gain_dbi)), 4.0, theme::ACCENT);
    p.circle_filled(Pos2::new(xx, yt(r.takeoff_deg)), 4.0, theme::WARM);
    let txt = tf!("{:.2} m ({:.2} λ)\n{:.2} dBi · {:.0}°", "{:.2} m ({:.2} λ)\n{:.2} dBi · {:.0}°", r.height, r.height / lambda, r.gain_dbi, r.takeoff_deg);
    p.text(Pos2::new(xx + 8.0, plot.top() + 4.0), Align2::LEFT_TOP, txt, theme::plot_font(11.0), Color32::WHITE);
    resp.clicked().then_some(r.height)
}
