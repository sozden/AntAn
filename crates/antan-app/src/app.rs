//! AntAn uygulama kabuğu: durum, düzen ve sekmeler.

use eframe::egui::{self, Color32, RichText, Vec2};

use antan_core::farfield::{self, Pattern, PatternSummary};
use antan_core::matching::{self, Reactive};
use antan_core::solver::{self, SolveError, SweepPoint};
use antan_core::templates::{self, Tube};
use antan_core::{Antenna, Material, Solution};

use crate::{editor, shot, view3d, viz};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Pattern3d,
    Cuts,
    Sweep,
    Matching,
}

pub struct AntAnApp {
    pub antenna: Antenna,
    pub selected: usize,
    pub default_tube: (f64, Option<f64>, Material),
    pub z0: f64,
    pub dirty: bool,
    pub status: String,

    pub solution: Option<Result<Solution, SolveError>>,
    pub pattern: Option<Pattern>,
    pub summary: Option<PatternSummary>,
    pub sweep: Option<Vec<SweepPoint>>,

    tab: Tab,
    view_yaw: f32,
    view_pitch: f32,
    sweep_span_pct: f64,

    rod_diameter_mm: f64,
    spacing_mm: f64,

    shotter: shot::Shotter,
    shot_msg: Option<String>,
    central_rect: Option<egui::Rect>,
    autoshot: Option<std::path::PathBuf>,
    autoshot_frame: u32,
}

impl AntAnApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let freq = 144.3e6;
        let tube = Tube::aluminum(0.010, Some(0.001));
        let antenna = templates::yagi(freq, 3, &tube);
        let mut app = Self {
            antenna,
            selected: 1,
            default_tube: (tube.diameter, tube.wall, tube.material),
            z0: 50.0,
            dirty: true,
            status: "Hazır".into(),
            solution: None,
            pattern: None,
            summary: None,
            sweep: None,
            tab: Tab::Pattern3d,
            view_yaw: -0.6,
            view_pitch: 0.5,
            sweep_span_pct: 5.0,
            rod_diameter_mm: 4.0,
            spacing_mm: 30.0,
            shotter: shot::Shotter::default(),
            shot_msg: None,
            central_rect: None,
            autoshot: std::env::var("ANTAN_AUTOSHOT").ok().map(std::path::PathBuf::from),
            autoshot_frame: 0,
        };
        if let Ok(t) = std::env::var("ANTAN_AUTOSHOT_TAB") {
            app.tab = match t.as_str() {
                "cuts" => Tab::Cuts,
                "sweep" => Tab::Sweep,
                "matching" => Tab::Matching,
                _ => Tab::Pattern3d,
            };
        }
        app.recompute();
        app
    }

    fn load_template(&mut self, n: usize) {
        let tube = Tube { diameter: self.default_tube.0, wall: self.default_tube.1, material: self.default_tube.2.clone() };
        self.antenna = if n <= 1 { templates::dipole(self.antenna.frequency, 0.485, &tube) } else { templates::yagi(self.antenna.frequency, n, &tube) };
        self.selected = self.antenna.sources.first().map(|s| s.wire).unwrap_or(0);
        self.dirty = true;
    }

    fn recompute(&mut self) {
        self.solution = Some(solver::solve(&self.antenna, self.antenna.frequency));
        match &self.solution {
            Some(Ok(sol)) => {
                let pat = Pattern::compute(sol, 4.0);
                self.summary = Some(pat.summary(sol));
                self.pattern = Some(pat);
                let span = self.antenna.frequency * self.sweep_span_pct / 100.0;
                self.sweep = solver::sweep(&self.antenna, self.antenna.frequency - span, self.antenna.frequency + span, 25).ok();
                self.status = "Çözüldü".into();
            }
            Some(Err(e)) => {
                self.pattern = None;
                self.summary = None;
                self.sweep = None;
                self.status = e.to_string();
            }
            None => {}
        }
        self.dirty = false;
    }
}

impl eframe::App for AntAnApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.central_rect = Some(ui.max_rect());
        if self.dirty {
            self.recompute();
        }
        if let Some(res) = self.shotter.poll(&ctx) {
            let closing = self.autoshot.is_some();
            self.shot_msg = Some(match res {
                Ok(p) => format!("Kaydedildi: {}", p.display()),
                Err(e) => format!("Hata: {e}"),
            });
            if closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Doğrulama için: ANTAN_AUTOSHOT ortam değişkeni verilmişse, çözüm
        // ve diyagram oturduktan birkaç kare sonra ekran görüntüsü alıp
        // pencereyi otomatik kapatır (etkileşimsiz doğrulama için).
        if let Some(path) = self.autoshot.clone() {
            self.autoshot_frame += 1;
            if self.autoshot_frame == 6 {
                if let Some(rect) = self.central_rect {
                    self.shotter.request(&ctx, rect, path);
                }
            } else {
                ctx.request_repaint();
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("AntAn").strong());
                ui.weak("anten analiz ve hesaplama");
                ui.separator();
                ui.label("Şablon:");
                if ui.button("Dipol").clicked() {
                    self.load_template(1);
                }
                for n in [3usize, 5, 7] {
                    if ui.button(format!("{n} el. Yagi")).clicked() {
                        self.load_template(n);
                    }
                }
                ui.separator();
                if ui.button("🔄 Yeniden hesapla").clicked() {
                    self.dirty = true;
                }
                ui.separator();
                if ui.button("📷 Ekranı kaydet").clicked() {
                    if let Some(rect) = self.central_rect {
                        let path = std::env::temp_dir().join("antan_shot.png");
                        self.shotter.request(&ctx, rect, path);
                    }
                }
                if let Some(m) = &self.shot_msg {
                    ui.weak(m);
                }
            });
            ui.add_space(4.0);
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).small());
                ui.separator();
                if let Some(Ok(sol)) = &self.solution {
                    let z = sol.impedance();
                    ui.label(format!("Z = {:.1} {}{:.1}j Ω", z.re, if z.im >= 0.0 { "+" } else { "" }, z.im));
                    ui.separator();
                    ui.label(format!("SWR = {:.2}", matching::swr(z, self.z0)));
                    ui.separator();
                    ui.label(format!("Verim = {:.1}%", sol.efficiency() * 100.0));
                    if let Some(s) = &self.summary {
                        ui.separator();
                        ui.label(format!("Kazanç = {:.2} dBi, Ö/A = {:.1} dB", s.max_gain_dbi, s.front_to_back_db));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(editor::status_line(&self.antenna)).small().weak());
                });
            });
        });

        egui::Panel::left("explorer").resizable(true).default_size(190.0).size_range(150.0..=320.0).show(ui, |ui| {
            editor::explorer(self, ui);
        });
        egui::Panel::right("props").resizable(true).default_size(320.0).size_range(260.0..=460.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| editor::properties(self, ui));
        });

        let central = egui::CentralPanel::default()
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Pattern3d, "3B Diyagram");
                    ui.selectable_value(&mut self.tab, Tab::Cuts, "Yatay / Düşey kesit");
                    ui.selectable_value(&mut self.tab, Tab::Sweep, "SWR taraması");
                    ui.selectable_value(&mut self.tab, Tab::Matching, "Eşleme");
                });
                ui.separator();

                let sol = match &self.solution {
                    Some(Ok(sol)) => sol.clone(),
                    _ => {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| ui.colored_label(Color32::from_rgb(200, 90, 70), self.status.clone()));
                        return;
                    }
                };
                match self.tab {
                    Tab::Pattern3d => self.pattern3d_tab(ui, &sol),
                    Tab::Cuts => self.cuts_tab(ui, &sol),
                    Tab::Sweep => self.sweep_tab(ui),
                    Tab::Matching => self.matching_tab(ui, &sol),
                }
            });
        let _ = central;
    }
}

impl AntAnApp {
    fn pattern3d_tab(&mut self, ui: &mut egui::Ui, sol: &Solution) {
        let Some(pat) = &self.pattern else { return };
        let Some(s) = &self.summary else { return };
        ui.horizontal_top(|ui| {
            let size = ui.available_size() - Vec2::new(260.0, 0.0);
            view3d::show(ui, Vec2::new(size.x.max(200.0), size.y), pat, s.max_gain_dbi, &mut self.view_yaw, &mut self.view_pitch);
            ui.vertical(|ui| {
                ui.set_width(240.0);
                summary_grid(ui, sol, s);
            });
        });
    }

    fn cuts_tab(&mut self, ui: &mut egui::Ui, sol: &Solution) {
        let Some(s) = &self.summary else { return };
        let az = farfield::azimuth_cut(sol, s.max_theta, 180);
        let el = farfield::elevation_cut(sol, s.max_phi, 180);
        ui.horizontal_top(|ui| {
            let w = (ui.available_width() - 20.0) * 0.5;
            viz::polar_plot(ui, Vec2::new(w, 420.0), "Yatay (azimuth) kesit — ana huzme tepesinden", &az, s.max_gain_dbi, ["İleri", "Sağ", "Geri", "Sol"]);
            viz::polar_plot(ui, Vec2::new(w, 420.0), "Düşey (elevation) kesit — ana huzimeden geçen düzlem", &el, s.max_gain_dbi, ["İleri/Ufuk", "Zenit", "Geri/Ufuk", "Nadir"]);
        });
        ui.add_space(8.0);
        ui.label("Sürülen eleman akım dağılımı (göreli genlik):");
        let drv = sol.feeds[0].basis;
        let drv_wire = sol.basis[drv].wire;
        let mags: Vec<(f64, f64)> = sol.wire_currents(drv_wire).into_iter().map(|(t, i)| (t, i.norm())).collect();
        viz::current_plot(ui, Vec2::new(ui.available_width(), 90.0), &mags);
    }

    fn sweep_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Tarama genişliği (± merkez frekansın %'si):");
            if ui.add(egui::DragValue::new(&mut self.sweep_span_pct).range(0.5..=30.0).suffix("%")).changed() {
                self.dirty = true;
            }
        });
        let Some(sweep) = &self.sweep else { return };
        viz::sweep_chart(ui, Vec2::new(ui.available_width(), 360.0), sweep, self.z0, self.antenna.frequency);
        ui.add_space(6.0);
        egui::Grid::new("sweep_table").striped(true).show(ui, |ui| {
            ui.label(RichText::new("Frekans (MHz)").strong());
            ui.label(RichText::new("Z (Ω)").strong());
            ui.label(RichText::new("SWR").strong());
            ui.end_row();
            for p in sweep.iter().step_by((sweep.len() / 12).max(1)) {
                ui.label(format!("{:.3}", p.frequency / 1.0e6));
                ui.label(format!("{:.1} {}{:.1}j", p.impedance.re, if p.impedance.im >= 0.0 { "+" } else { "" }, p.impedance.im));
                ui.label(format!("{:.2}", matching::swr(p.impedance, self.z0)));
                ui.end_row();
            }
        });
    }

    fn matching_tab(&mut self, ui: &mut egui::Ui, sol: &Solution) {
        let z = sol.impedance();
        ui.label(format!("Besleme empedansı: Z = {:.2} {}{:.2}j Ω  (SWR = {:.2}, {} Ω hatta)", z.re, if z.im >= 0.0 { "+" } else { "" }, z.im, matching::swr(z, self.z0), self.z0));
        ui.separator();

        ui.columns(3, |cols| {
            cols[0].label(RichText::new("L devresi").strong());
            let nets = matching::l_network(z, self.z0, self.antenna.frequency);
            if nets.is_empty() {
                cols[0].weak("Bu empedans için basit L devresi çözümü yok.");
            }
            for (i, n) in nets.iter().enumerate() {
                cols[0].label(format!("Çözüm {}: {}", i + 1, if n.shunt_at_load { "paralel yükte, seri hatta" } else { "seri yükte, paralel hatta" }));
                cols[0].label(format!("  Seri: {}", reactive_str(n.series)));
                cols[0].label(format!("  Paralel: {}", reactive_str(n.shunt)));
                cols[0].add_space(4.0);
            }

            cols[1].label(RichText::new("Hairpin (beta)").strong());
            cols[1].horizontal(|ui| {
                ui.label("Çubuk çapı");
                ui.add(egui::DragValue::new(&mut self.rod_diameter_mm).range(1.0..=20.0).suffix(" mm"));
            });
            cols[1].horizontal(|ui| {
                ui.label("Aralık");
                ui.add(egui::DragValue::new(&mut self.spacing_mm).range(5.0..=150.0).suffix(" mm"));
            });
            match matching::hairpin(z, self.z0, self.antenna.frequency, self.rod_diameter_mm / 1000.0, self.spacing_mm / 1000.0) {
                Some(h) => {
                    cols[1].label(format!("Eleman reaktansı {:.2} Ω değişmeli (Δ = {:+.2} Ω; kısaltın/uzatın)", h.required_reactance, h.reactance_change));
                    cols[1].label(format!("Saplama endüktansı: {:.1} nH ({:.1} Ω)", h.inductance * 1.0e9, h.shunt_reactance));
                    cols[1].label(format!("Hat Z₀ = {:.1} Ω, boy = {:.1} mm", h.line_z0, h.length * 1000.0));
                }
                None => {
                    cols[1].weak("R ≥ Z₀ veya geometri geçersiz — hairpin uygun değil (R < Z₀ gerekir).");
                }
            }

            cols[2].label(RichText::new("Gamma eşleme").strong());
            let w = &self.antenna.wires[sol.basis[sol.feeds[0].basis].wire];
            let p = matching::GammaParams { element_diameter: w.diameter, rod_diameter: self.rod_diameter_mm / 1000.0, spacing: self.spacing_mm / 1000.0 };
            match matching::gamma(z, self.z0, self.antenna.frequency, p) {
                Some(g) => {
                    cols[2].label(format!("Çubuk boyu: {:.1} mm", g.length * 1000.0));
                    cols[2].label(format!("Seri kondansatör: {:.1} pF", g.capacitance * 1.0e12));
                    cols[2].label(format!("Empedans yükseltme: {:.2}×", g.step_up));
                    cols[2].label(format!("Bölüm hat Z₀: {:.1} Ω", g.line_z0));
                }
                None => {
                    cols[2].weak("Bu geometri/empedans için çözüm bulunamadı.");
                }
            }
        });
    }
}

fn reactive_str(r: Reactive) -> String {
    match r {
        Reactive::Inductor(l) => format!("{:.1} nH (endüktör)", l * 1.0e9),
        Reactive::Capacitor(c) => format!("{:.1} pF (kondansatör)", c * 1.0e12),
    }
}

fn summary_grid(ui: &mut egui::Ui, sol: &Solution, s: &PatternSummary) {
    ui.label(RichText::new("Özet").strong());
    ui.separator();
    egui::Grid::new("summary").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
        ui.label("Kazanç (ana huzme)");
        ui.label(format!("{:.2} dBi", s.max_gain_dbi));
        ui.end_row();
        ui.label("Yönlülük");
        ui.label(format!("{:.2} dBi", s.directivity_dbi));
        ui.end_row();
        ui.label("Ön/Arka");
        ui.label(format!("{:.1} dB", s.front_to_back_db));
        ui.end_row();
        ui.label("Huzme genişliği (az.)");
        ui.label(format!("{:.0}°", s.beamwidth_az_deg));
        ui.end_row();
        ui.label("Huzme genişliği (yük.)");
        ui.label(format!("{:.0}°", s.beamwidth_el_deg));
        ui.end_row();
        ui.label("Verim");
        ui.label(format!("{:.2}%", sol.efficiency() * 100.0));
        ui.end_row();
        ui.label("Giriş gücü (1W kaynak)");
        ui.label(format!("{:.3} W", sol.input_power));
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.weak("Fare ile sürükleyerek 3B diyagramı döndürün.");
}
