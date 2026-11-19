//! İstif penceresi: düzen (2/4 anten), aralıklar, DL6WU önerisi ve aralık taraması.
//! İstif projeye kaydedilir; ana görünümler (3B, kesitler, SWR) istifin tamamını gösterir.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};

use antan_core::Antenna;
use antan_core::i18n::t as tl;
use antan_core::matching;
use antan_core::stack::{self, Stack, StackRow};
use antan_core::tf;

use crate::theme;
use crate::viz::fmt_z;

/// Hazır düzenler: (satır, sütun).
const LAYOUTS: [(usize, usize); 7] = [(1, 1), (2, 1), (1, 2), (3, 1), (4, 1), (1, 4), (2, 2)];

fn layout_label(r: usize, c: usize) -> String {
    match (r, c) {
        (1, 1) => tl("Single antenna (no stack)", "Tek anten (istif yok)").into(),
        (_, 1) => tf!("{r} stacked vertically", "{r} anten üst üste (dikey)"),
        (1, _) => tf!("{c} side by side (horizontal)", "{c} anten yan yana (yatay)"),
        _ => tf!("{r} × {c} (H-frame)", "{r} × {c} (H çerçeve)"),
    }
}

pub struct StackWindow {
    pub open: bool,
    from_wl: f64,
    to_wl: f64,
    points: usize,
    result: Arc<Mutex<Option<Vec<StackRow>>>>,
    done: Arc<AtomicUsize>,
    total: usize,
    running: bool,
    /// Önerilen aralıklar (dikey, yatay; m) — hangi tek anten için hesaplandığıyla.
    rec: Option<(Antenna, Option<(f64, f64)>)>,
    /// Taramanın hangi düzen için yapıldığı (dikey mi taranıyor).
    scanned_vertical: bool,
}

impl Default for StackWindow {
    fn default() -> Self {
        Self { open: false, from_wl: 0.5, to_wl: 2.5, points: 29, result: Arc::default(), done: Arc::default(), total: 0, running: false, rec: None, scanned_vertical: true }
    }
}

impl StackWindow {
    pub fn busy(&self) -> bool {
        self.running && self.result.lock().unwrap().is_none()
    }

    pub fn recommended(&mut self, single: &Antenna) -> Option<(f64, f64)> {
        if self.rec.as_ref().map(|(a, _)| a != single).unwrap_or(true) {
            self.rec = Some((single.clone(), stack::recommended_spacing(single)));
        }
        self.rec.as_ref().and_then(|(_, r)| *r)
    }

    pub fn start(&mut self, ctx: &egui::Context, single: Antenna, st: Stack, z0: f64) {
        let lambda = antan_core::C0 / single.frequency;
        let n = self.points.max(2);
        let sp: Vec<f64> = (0..n).map(|i| (self.from_wl + (self.to_wl - self.from_wl) * i as f64 / (n - 1) as f64) * lambda).collect();
        self.total = sp.len();
        self.scanned_vertical = st.rows > 1;
        self.done.store(0, Ordering::Relaxed);
        *self.result.lock().unwrap() = None;
        self.running = true;
        let (result, done, ctx) = (self.result.clone(), self.done.clone(), ctx.clone());
        std::thread::spawn(move || {
            let ctx2 = ctx.clone();
            let rows = stack::spacing_scan(&single, &st, &sp, z0, &|| {
                done.fetch_add(1, Ordering::Relaxed);
                ctx2.request_repaint();
            });
            *result.lock().unwrap() = Some(rows);
            ctx.request_repaint();
        });
    }

    /// Pencereyi çizer; kullanıcı istifi değiştirirse yeni değeri döner.
    pub fn show(&mut self, ctx: &egui::Context, single: &Antenna, current: Stack, single_gain: Option<f64>, z0: f64) -> Option<Stack> {
        if !self.open {
            return None;
        }
        let mut st = current;
        let mut open = self.open;
        let lambda = antan_core::C0 / single.frequency;
        let rec = self.recommended(single);
        egui::Window::new(tl("Stacking", "İstif")).open(&mut open).default_size([860.0, 680.0]).resizable(true).show(ctx, |ui| {
            ui.label(RichText::new(tl("Copies of the antenna are stacked vertically and/or side by side and fed in phase with equal power (equal-length cables and an ideal power divider). Mutual coupling between the antennas is included. All views (3D, cuts, SWR) then show the whole stack; the impedance shown is at the divider input.", "Antenin kopyaları üst üste ve/veya yan yana dizilir, eşit güçle aynı fazda beslenir (eşit boylu kablolar ve ideal güç bölücü). Antenler arası karşılıklı etki hesaba girer. Bütün görünümler (3B, kesitler, SWR) istifin tamamını gösterir; gösterilen empedans güç bölücü girişindedir.")).small().color(theme::TEXT_DIM));
            ui.add_space(4.0);
            egui::Grid::new("stack_grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tl("Layout", "Düzen"));
                egui::ComboBox::from_id_salt("stack_layout").selected_text(layout_label(st.rows.max(1), st.cols.max(1))).width(260.0).show_ui(ui, |ui| {
                    for (r, c) in LAYOUTS {
                        if ui.selectable_label(st.rows == r && st.cols == c, layout_label(r, c)).clicked() {
                            st.rows = r;
                            st.cols = c;
                            // İlk seçimde önerilen aralıkları uygula.
                            if let Some((v, h)) = rec {
                                if st.v_spacing <= 0.0 {
                                    st.v_spacing = v;
                                }
                                if st.h_spacing <= 0.0 {
                                    st.h_spacing = h;
                                }
                            }
                        }
                    }
                });
                ui.label("");
                ui.label("");
                ui.end_row();
                let row = |ui: &mut egui::Ui, label: &str, val: &mut f64, recv: Option<f64>, enabled: bool| {
                    ui.add_enabled_ui(enabled, |ui| ui.label(label));
                    let mut wl = *val / lambda;
                    ui.add_enabled_ui(enabled, |ui| {
                        if ui.add(egui::DragValue::new(&mut wl).range(0.1..=10.0).speed(0.005).suffix(" λ").max_decimals(3)).changed() {
                            *val = wl * lambda;
                        }
                        ui.label(RichText::new(format!("= {:.3} m", *val)).color(theme::TEXT_DIM));
                    });
                    match recv {
                        Some(r) => {
                            ui.add_enabled_ui(enabled, |ui| {
                                if ui.button(tf!("DL6WU: {:.2} λ ({:.2} m) — use", "DL6WU: {:.2} λ ({:.2} m) — kullan", r / lambda, r)).on_hover_text(tl("Optimum spacing from the −3 dB beamwidth in the stacking plane: D = λ / (2·sin(θ/2))", "İstif düzlemindeki −3 dB huzme genişliğinden en uygun aralık: D = λ / (2·sin(θ/2))")).clicked() {
                                    *val = r;
                                }
                            });
                        }
                        None => {
                            ui.label("");
                        }
                    }
                    ui.label("");
                    ui.end_row();
                };
                row(ui, tl("Vertical spacing", "Dikey aralık"), &mut st.v_spacing, rec.map(|r| r.0), st.rows > 1);
                row(ui, tl("Horizontal spacing", "Yatay aralık"), &mut st.h_spacing, rec.map(|r| r.1), st.cols > 1);
            });
            if st.cols > 1 {
                let span = single.bounds();
                let width = span.1[1] - span.0[1];
                if st.h_spacing < width * 1.05 {
                    ui.label(RichText::new(tf!("⚠ Horizontal spacing is shorter than the antenna width ({:.2} m): the element tips would touch.", "⚠ Yatay aralık anten genişliğinden ({:.2} m) kısa: eleman uçları birbirine değer.", width)).color(theme::WARM));
                }
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label(RichText::new(tl("Spacing scan", "Aralık taraması")).strong().color(theme::ACCENT));
            if st.is_single() {
                ui.label(RichText::new(tl("Choose a layout with two or more antennas first.", "Önce iki ya da daha çok antenli bir düzen seçin.")).color(theme::TEXT_DIM));
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(if st.rows > 1 { tl("Vertical spacing from", "Dikey aralık") } else { tl("Horizontal spacing from", "Yatay aralık") });
                ui.add(egui::DragValue::new(&mut self.from_wl).range(0.2..=10.0).speed(0.01).suffix(" λ"));
                ui.label(tl("to", "–"));
                ui.add(egui::DragValue::new(&mut self.to_wl).range(0.3..=12.0).speed(0.01).suffix(" λ"));
                ui.label(tl("points", "nokta"));
                ui.add(egui::DragValue::new(&mut self.points).range(5..=120));
                let busy = self.busy();
                if ui.add_enabled(!busy, egui::Button::new(RichText::new(tl("▶ Scan", "▶ Tara")).strong()).fill(theme::ACCENT_DIM)).clicked() {
                    if self.to_wl <= self.from_wl {
                        self.to_wl = self.from_wl + 1.0;
                    }
                    self.start(ctx, single.clone(), st, z0);
                }
                if busy {
                    ui.spinner();
                    ui.label(format!("{}/{}", self.done.load(Ordering::Relaxed), self.total));
                }
            });
            if st.rows > 1 && st.cols > 1 {
                ui.label(RichText::new(tl("In the H-frame the horizontal spacing changes in the same ratio as the vertical one.", "H çerçevede yatay aralık dikeyle aynı oranda değişir.")).small().color(theme::TEXT_DIM));
            }
            let rows = self.result.lock().unwrap().clone();
            let Some(rows) = rows else { return };
            if rows.is_empty() {
                ui.label(RichText::new(tl("No spacing could be solved.", "Hiçbir aralık çözülemedi.")).color(theme::WARM));
                return;
            }
            if let Some(b) = rows.iter().max_by(|a, b| a.gain_dbi.total_cmp(&b.gain_dbi)) {
                let gain = match single_gain {
                    Some(g) => tf!(" (+{:.2} dB over one antenna)", " (tek antene göre +{:.2} dB)", b.gain_dbi - g),
                    None => String::new(),
                };
                ui.label(RichText::new(tf!("Highest gain: {:.2} dBi{} at {:.2} λ ({:.2} m)", "En yüksek kazanç: {:.2} dBi{}, {:.2} λ ({:.2} m) aralıkta", b.gain_dbi, gain, b.spacing / lambda, b.spacing)).color(theme::GOOD));
            }
            let cur = if self.scanned_vertical { st.v_spacing } else { st.h_spacing };
            let h = (ui.available_height() * 0.55).max(200.0);
            if let Some(d) = chart(ui, &rows, Vec2::new(ui.available_width(), h), lambda, cur) {
                self.pick(&mut st, d);
            }
            ui.add_space(4.0);
            let mut pick = None;
            egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                egui::Grid::new("stack_table").striped(true).num_columns(8).spacing([14.0, 2.0]).show(ui, |ui| {
                    for t in [tl("Spacing", "Aralık"), "m", tl("Gain", "Kazanç"), tl("F/B", "Ö/A"), tl("Side lobe", "Yan lob"), tl("Z (divider)", "Z (bölücü)"), "SWR", tl("ΔR between feeds", "Beslemeler arası ΔR")] {
                        ui.label(RichText::new(t).small().color(theme::TEXT_DIM));
                    }
                    ui.label("");
                    ui.end_row();
                    for r in &rows {
                        let swr = matching::swr(r.impedance, z0);
                        ui.label(format!("{:.2} λ", r.spacing / lambda));
                        ui.label(format!("{:.2}", r.spacing));
                        ui.label(format!("{:.2} dBi", r.gain_dbi));
                        ui.label(format!("{:.1} dB", r.front_to_back_db));
                        ui.label(format!("{:.1} dB", r.sidelobe_db));
                        ui.label(fmt_z(r.impedance));
                        ui.label(RichText::new(format!("{swr:.2}")).color(theme::swr_color(swr)));
                        ui.label(format!("{:.1} Ω", r.feed_spread));
                        if ui.small_button(tl("apply", "uygula")).clicked() {
                            pick = Some(r.spacing);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(d) = pick {
                self.pick(&mut st, d);
            }
        });
        self.open = open;
        (st != current).then_some(st)
    }

    fn pick(&self, st: &mut Stack, d: f64) {
        if self.scanned_vertical {
            if st.v_spacing > 0.0 && st.cols > 1 {
                st.h_spacing *= d / st.v_spacing;
            }
            st.v_spacing = d;
        } else {
            st.h_spacing = d;
        }
    }
}

/// Kazanç (sol eksen, mavi) ve yan lob seviyesi (sağ eksen, turuncu) — aralığa göre.
fn chart(ui: &mut egui::Ui, rows: &[StackRow], size: Vec2, lambda: f64, current: f64) -> Option<f64> {
    let (resp, p) = ui.allocate_painter(size, Sense::click());
    let rect = resp.rect;
    p.rect_filled(rect, 6.0, theme::PLOT_BG);
    let plot = Rect::from_min_max(rect.min + Vec2::new(54.0, 26.0), rect.max - Vec2::new(54.0, 30.0));
    let (s0, s1) = (rows[0].spacing, rows[rows.len() - 1].spacing.max(rows[0].spacing + 1e-9));
    let gmax = rows.iter().map(|r| r.gain_dbi).fold(f64::NEG_INFINITY, f64::max).ceil();
    let gmin = rows.iter().map(|r| r.gain_dbi).fold(f64::INFINITY, f64::min).floor().min(gmax - 1.0);
    let (lmin, lmax) = (-40.0, 0.0);
    let x = |s: f64| plot.left() + ((s - s0) / (s1 - s0)) as f32 * plot.width();
    let yg = |g: f64| plot.bottom() - ((g - gmin) / (gmax - gmin)) as f32 * plot.height();
    let yl = |l: f64| plot.bottom() - ((l.clamp(lmin, lmax) - lmin) / (lmax - lmin)) as f32 * plot.height();
    for k in 0..=5 {
        let g = gmin + (gmax - gmin) * k as f64 / 5.0;
        let y = yg(g);
        p.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0, theme::GRID));
        p.text(Pos2::new(plot.left() - 6.0, y), Align2::RIGHT_CENTER, format!("{g:.1}"), theme::plot_font(10.0), theme::ACCENT);
        let l = lmin + (lmax - lmin) * k as f64 / 5.0;
        p.text(Pos2::new(plot.right() + 6.0, yl(l)), Align2::LEFT_CENTER, format!("{l:.0} dB"), theme::plot_font(10.0), theme::WARM);
    }
    let step = if (s1 - s0) / lambda > 4.0 { 1.0 } else if (s1 - s0) / lambda > 1.5 { 0.5 } else { 0.25 };
    let mut k = (s0 / lambda / step).ceil() * step;
    while k * lambda <= s1 + 1e-9 {
        let xx = x(k * lambda);
        p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.0, theme::GRID));
        p.text(Pos2::new(xx, plot.bottom() + 4.0), Align2::CENTER_TOP, format!("{k:.2} λ\n{:.2} m", k * lambda), theme::plot_font(9.5), theme::TEXT_DIM);
        k += step;
    }
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, tl("Gain (dBi)", "Kazanç (dBi)"), theme::plot_font(12.0), theme::ACCENT);
    p.text(rect.right_top() + Vec2::new(-10.0, 6.0), Align2::RIGHT_TOP, tl("Highest side lobe", "En yüksek yan lob"), theme::plot_font(12.0), theme::WARM);
    p.add(Shape::line(rows.iter().map(|r| Pos2::new(x(r.spacing), yg(r.gain_dbi))).collect(), Stroke::new(2.0, theme::ACCENT)));
    p.add(Shape::line(rows.iter().map(|r| Pos2::new(x(r.spacing), yl(r.sidelobe_db))).collect(), Stroke::new(2.0, theme::WARM)));
    if current >= s0 && current <= s1 {
        let xx = x(current);
        p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.5, theme::GOOD));
    }
    let hover = resp.hover_pos().filter(|pos| plot.expand(4.0).contains(*pos))?;
    let ss = s0 + ((hover.x - plot.left()) / plot.width()).clamp(0.0, 1.0) as f64 * (s1 - s0);
    let r = rows.iter().min_by(|a, b| (a.spacing - ss).abs().total_cmp(&(b.spacing - ss).abs()))?;
    let xx = x(r.spacing);
    p.line_segment([Pos2::new(xx, plot.top()), Pos2::new(xx, plot.bottom())], Stroke::new(1.0, Color32::WHITE));
    p.circle_filled(Pos2::new(xx, yg(r.gain_dbi)), 4.0, theme::ACCENT);
    p.circle_filled(Pos2::new(xx, yl(r.sidelobe_db)), 4.0, theme::WARM);
    let txt = tf!("{:.2} λ ({:.2} m)\n{:.2} dBi · side lobe {:.1} dB", "{:.2} λ ({:.2} m)\n{:.2} dBi · yan lob {:.1} dB", r.spacing / lambda, r.spacing, r.gain_dbi, r.sidelobe_db);
    p.text(Pos2::new(xx + 8.0, plot.top() + 4.0), Align2::LEFT_TOP, txt, theme::plot_font(11.0), Color32::WHITE);
    resp.clicked().then_some(r.spacing)
}
