//! İyileştirme sekmesi: Nelder-Mead'i arka planda çalıştırır, ilerlemeyi gösterir.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Vec2};

use antan_core::optimize::{self, Goals, Score};
use antan_core::yagi::YagiDesign;

use crate::theme;

#[derive(Default)]
struct Shared {
    iter: usize,
    best: Option<(YagiDesign, Score)>,
    history: Vec<f64>,
    done: bool,
}

pub struct Optim {
    pub iterations: usize,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    running: bool,
    start: Option<(YagiDesign, Score)>,
}

impl Default for Optim {
    fn default() -> Self {
        Self { iterations: 400, shared: Arc::default(), stop: Arc::new(AtomicBool::new(false)), running: false, start: None }
    }
}

impl Optim {
    /// Arka planda iş sürüyor mu?
    pub fn busy(&self) -> bool {
        self.running && !self.shared.lock().unwrap().done
    }

    /// Doğrulama için dışarıdan başlatma.
    pub fn start(&mut self, ctx: &egui::Context, design: YagiDesign, goals: Goals, iterations: usize) {
        self.iterations = iterations;
        self.launch(ctx, design, goals);
    }

    fn launch(&mut self, ctx: &egui::Context, design: YagiDesign, goals: Goals) {
        self.stop.store(false, Ordering::Relaxed);
        *self.shared.lock().unwrap() = Shared::default();
        self.start = optimize::evaluate(&design, &goals).map(|s| (design.clone(), s));
        self.running = true;
        let (shared, stop, ctx, iters) = (self.shared.clone(), self.stop.clone(), ctx.clone(), self.iterations);
        std::thread::spawn(move || {
            let (best, score) = optimize::optimize(&design, &goals, iters, |i, s, d| {
                let mut sh = shared.lock().unwrap();
                sh.iter = i;
                sh.history.push(s.cost);
                sh.best = Some((d.clone(), *s));
                drop(sh);
                ctx.request_repaint();
                !stop.load(Ordering::Relaxed)
            });
            let mut sh = shared.lock().unwrap();
            sh.best = Some((best, score));
            sh.done = true;
            ctx.request_repaint();
        });
    }

    /// Uygulanacak tasarım dönerse çağıran onu projeye yazar.
    pub fn ui(&mut self, ui: &mut egui::Ui, design: Option<&YagiDesign>, goals: &mut Goals) -> Option<YagiDesign> {
        let Some(design) = design else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("İyileştirme şimdilik Yagi tasarımları içindir.").heading());
                ui.label("Dosya ▸ Yeni ▸ Yagi ile bir Yagi tasarımı başlatın.");
            });
            return None;
        };
        if self.running && self.shared.lock().unwrap().done {
            self.running = false;
        }
        let mut apply = None;

        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("Hedefler").strong().color(Color32::WHITE));
            ui.add_enabled_ui(!self.running, |ui| {
                egui::Grid::new("goals").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Kazanç ağırlığı");
                    ui.add(egui::Slider::new(&mut goals.w_gain, 0.0..=3.0));
                    ui.end_row();
                    ui.label("Ön/arka ağırlığı");
                    ui.add(egui::Slider::new(&mut goals.w_fb, 0.0..=2.0));
                    ui.end_row();
                    ui.label("Ön/arka tavanı");
                    ui.add(egui::Slider::new(&mut goals.fb_cap_db, 10.0..=40.0).suffix(" dB")).on_hover_text("Bu değerin üstündeki Ö/A artık ödüllendirilmez");
                    ui.end_row();
                    ui.label("SWR ağırlığı");
                    ui.add(egui::Slider::new(&mut goals.w_swr, 0.0..=10.0));
                    ui.end_row();
                    ui.label("Hedef Z₀");
                    ui.add(egui::DragValue::new(&mut goals.z0).range(5.0..=600.0).suffix(" Ω"));
                    ui.end_row();
                    ui.label("Bant (±)");
                    let mut pct = goals.half_band * 100.0;
                    if ui.add(egui::Slider::new(&mut pct, 0.0..=3.0).suffix(" %")).on_hover_text("0 = yalnız merkez frekans; aksi halde bant kenarları da değerlendirilir").changed() {
                        goals.half_band = pct / 100.0;
                    }
                    ui.end_row();
                    ui.label("Kısıtlar");
                    ui.vertical(|ui| {
                        ui.checkbox(&mut goals.move_elements, "Eleman konumları değişebilir");
                        ui.add_enabled(goals.move_elements, egui::Checkbox::new(&mut goals.keep_boom, "Bum boyunu koru"));
                    });
                    ui.end_row();
                    ui.label("En çok adım");
                    ui.add(egui::DragValue::new(&mut self.iterations).range(20..=5000));
                    ui.end_row();
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if !self.running {
                    if ui.add(egui::Button::new(RichText::new("▶ Başlat").strong()).fill(theme::ACCENT_DIM)).clicked() {
                        self.launch(ui.ctx(), design.clone(), goals.clone());
                    }
                } else if ui.add(egui::Button::new("⏹ Durdur").fill(Color32::from_rgb(110, 40, 35))).clicked() {
                    self.stop.store(true, Ordering::Relaxed);
                }
                if self.running {
                    ui.spinner();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Yöntem: Nelder-Mead simpleks. Her adımda tasarım tam MoM çözümüyle değerlendirilir; boylar ve (izin verilirse) konumlar dalga boyu cinsinden ayarlanır. Yansıtıcı x = 0'da sabittir.").small().color(theme::TEXT_DIM));

            let ui = &mut cols[1];
            let sh = self.shared.lock().unwrap();
            ui.label(RichText::new("İlerleme").strong().color(Color32::WHITE));
            ui.label(format!("Adım: {}", sh.iter));
            let row = |ui: &mut egui::Ui, name: &str, s: &Score| {
                ui.label(name);
                ui.label(format!("{:.2} dBi", s.gain_dbi));
                ui.label(format!("{:.1} dB", s.fb_db));
                ui.label(RichText::new(format!("{:.2}", s.swr)).color(theme::swr_color(s.swr)));
                ui.label(format!("{:.2}", s.cost));
                ui.end_row();
            };
            egui::Grid::new("opt_scores").num_columns(5).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for h in ["", "Kazanç", "Ö/A", "SWR", "Maliyet"] {
                    ui.label(RichText::new(h).small());
                }
                ui.end_row();
                if let Some((_, s)) = &self.start {
                    row(ui, "Başlangıç", s);
                }
                if let Some((_, s)) = &sh.best {
                    row(ui, "En iyi", s);
                }
            });
            // Maliyet geçmişi.
            let (resp, p) = ui.allocate_painter(Vec2::new(ui.available_width(), 90.0), Sense::hover());
            let r = resp.rect;
            p.rect_filled(r, 4.0, theme::PLOT_BG);
            if sh.history.len() > 1 {
                let (lo, hi) = sh.history.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v.min(1e3))));
                let span = (hi - lo).max(1e-9);
                let pts: Vec<Pos2> = sh
                    .history
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Pos2::new(r.left() + r.width() * i as f32 / (sh.history.len() - 1) as f32, r.bottom() - 6.0 - (r.height() - 12.0) * ((hi - v.min(1e3)) / span) as f32))
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.8, theme::GOOD)));
            }
            p.text(r.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, "en iyi maliyet (yukarı = daha iyi)", FontId::proportional(10.0), theme::TEXT_DIM);

            if let Some((best, _)) = &sh.best {
                ui.add_space(6.0);
                egui::Grid::new("opt_design").num_columns(4).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for h in ["Eleman", "Konum", "Boy", "Δ boy"] {
                        ui.label(RichText::new(h).small());
                    }
                    ui.end_row();
                    for (e, e0) in best.elements.iter().zip(&design.elements) {
                        ui.label(&e.name);
                        ui.label(format!("{:.1} mm", e.x * 1000.0));
                        ui.label(format!("{:.1} mm", e.length * 1000.0));
                        let dl = (e.length - e0.length) * 1000.0;
                        ui.label(RichText::new(format!("{dl:+.1} mm")).color(if dl.abs() < 0.05 { theme::TEXT_DIM } else { theme::WARM }));
                        ui.end_row();
                    }
                });
                if !self.running && ui.add(egui::Button::new(RichText::new("✔ Sonucu tasarıma uygula").strong()).fill(Color32::from_rgb(35, 90, 55))).clicked() {
                    apply = Some(best.clone());
                }
            }
        });
        apply
    }
}
