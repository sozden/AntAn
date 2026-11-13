//! AntAn uygulama kabuğu: durum, menüler, düzen ve sekmeler.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, Vec2};

use antan_core::farfield::{self, Pattern, PatternSummary};
use antan_core::matching::{self, Reactive};
use antan_core::solver::{self, SolveError, SweepPoint};
use antan_core::templates::Kind;
use antan_core::{Antenna, C0, Solution, nec};

use crate::compute::SweepJob;
use crate::optim_ui::Optim;
use crate::project::{self, Model, Project};
use crate::view3d::{self, View3d};
use crate::viz::{self, Polar, PolarInfo, fmt_z};
use crate::{editor, shot, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Pattern3d,
    Cuts,
    Sweep,
    Matching,
    Optimize,
}

/// Çözümü yeniden hesaplamayı gerektiren girdiler.
#[derive(PartialEq, Clone)]
struct SolveKey {
    model: Model,
    span: f64,
    points: usize,
}

pub struct AntAnApp {
    pub project: Project,
    pub selected: usize,
    pub status: String,
    path: Option<PathBuf>,
    saved: Project,
    committed: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,

    antenna: Antenna,
    wire_map: Vec<Range<usize>>,
    solution: Option<Result<Solution, SolveError>>,
    pattern: Option<Pattern>,
    summary: Option<PatternSummary>,
    sweep: Option<Vec<SweepPoint>>,
    sweep_job: SweepJob,
    solved: Option<SolveKey>,
    solve_ms: f64,

    tab: Tab,
    view: View3d,
    cut_range_db: f64,
    scale_with_freq: bool,
    optim: Optim,
    tune_msg: Option<String>,
    show_about: bool,

    shotter: shot::Shotter,
    window_rect: Option<egui::Rect>,
    autoshot: Option<PathBuf>,
    autoshot_frame: u32,
}

impl AntAnApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let project = match std::env::var("ANTAN_AUTOSHOT_TEMPLATE").ok().as_deref() {
            Some("moxon") => Project::template(Kind::Moxon, 144.3e6),
            Some("quad") => Project::template(Kind::Quad2, 144.3e6),
            Some("gp") => Project::template(Kind::GroundPlane, 145.0e6),
            Some("vertical") => Project::template(Kind::GroundVertical, 14.2e6),
            Some("trap") => Project::template(Kind::TrapDipole, 3.65e6),
            Some("loaded") => Project::template(Kind::LoadedVertical, 14.2e6),
            Some("yagi_ground") => {
                let mut p = Project::yagi(144.3e6, 5);
                if let Model::Yagi(d) = &mut p.model {
                    d.ground = antan_core::geometry::Ground::Real { permittivity: 13.0, conductivity: 0.005 };
                    d.height = 2.0 * C0 / d.frequency;
                }
                p
            }
            Some("yagi7") => tapered_demo(),
            _ => Project::yagi(144.3e6, 5),
        };
        let mut app = Self {
            saved: project.clone(),
            committed: project.clone(),
            project,
            selected: 1,
            status: "Hazır".into(),
            path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            antenna: Antenna::new("", 1.0, vec![], vec![]),
            wire_map: Vec::new(),
            solution: None,
            pattern: None,
            summary: None,
            sweep: None,
            sweep_job: SweepJob::default(),
            solved: None,
            solve_ms: 0.0,
            tab: Tab::Pattern3d,
            view: View3d::default(),
            cut_range_db: 30.0,
            scale_with_freq: false,
            optim: Optim::default(),
            tune_msg: None,
            show_about: false,
            shotter: shot::Shotter::default(),
            window_rect: None,
            autoshot: std::env::var("ANTAN_AUTOSHOT").ok().map(PathBuf::from),
            autoshot_frame: 0,
        };
        app.tab = match std::env::var("ANTAN_AUTOSHOT_TAB").ok().as_deref() {
            Some("cuts") => Tab::Cuts,
            Some("sweep") => Tab::Sweep,
            Some("matching") => Tab::Matching,
            Some("optim") => Tab::Optimize,
            _ => Tab::Pattern3d,
        };
        app
    }

    // ------------------------------------------------------------ durum

    fn dirty(&self) -> bool {
        self.project != self.saved
    }

    fn replace_project(&mut self, p: Project) {
        self.undo.push(self.project.clone());
        self.redo.clear();
        self.project = p;
        self.committed = self.project.clone();
        self.selected = match &self.project.model {
            Model::Yagi(d) => d.driven,
            Model::Wires(a) => a.sources.first().map(|s| s.wire).unwrap_or(0),
        };
    }

    /// Düzenleme bittiğinde (fare bırakılınca) geri alma adımı kaydeder.
    fn track_undo(&mut self, ctx: &egui::Context) {
        let idle = !ctx.input(|i| i.pointer.any_down());
        if idle && self.project != self.committed {
            self.undo.push(std::mem::replace(&mut self.committed, self.project.clone()));
            self.redo.clear();
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
    }

    fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
        }
    }
    fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
        }
    }

    fn recompute_if_needed(&mut self, ctx: &egui::Context) {
        let key = SolveKey { model: self.project.model.clone(), span: self.project.sweep_span_pct, points: self.project.sweep_points };
        if self.solved.as_ref() == Some(&key) {
            return;
        }
        let t0 = Instant::now();
        (self.antenna, self.wire_map) = match &self.project.model {
            Model::Yagi(d) => d.build_with_map(),
            Model::Wires(a) => (a.clone(), (0..a.wires.len()).map(|i| i..i + 1).collect()),
        };
        let f = self.antenna.frequency;
        let sol = solver::solve(&self.antenna, f);
        match &sol {
            Ok(s) => {
                let pat = Pattern::compute(s, 3.0);
                self.summary = Some(pat.summary(s));
                self.pattern = Some(pat);
                let span = f * self.project.sweep_span_pct / 100.0;
                self.sweep_job.start(ctx, self.antenna.clone(), f - span, f + span, self.project.sweep_points);
                self.status = "Çözüldü".into();
            }
            Err(e) => {
                self.pattern = None;
                self.summary = None;
                self.sweep = None;
                self.status = e.to_string();
            }
        }
        self.solution = Some(sol);
        self.solve_ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.solved = Some(key);
    }

    fn sol(&self) -> Option<&Solution> {
        match &self.solution {
            Some(Ok(s)) => Some(s),
            _ => None,
        }
    }

    fn highlight(&self) -> Vec<usize> {
        self.wire_map.get(self.selected).map(|r| r.clone().collect()).unwrap_or_default()
    }

    fn band(&self) -> Option<(f64, f64)> {
        viz::swr_bandwidth(self.sweep.as_deref()?, self.project.z0, self.project.model.frequency(), 2.0)
    }

    // ------------------------------------------------------------ dosya

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("AntAn projesi", &[project::EXT]).pick_file() {
            match Project::load(&path) {
                Ok(p) => {
                    self.replace_project(p);
                    self.saved = self.project.clone();
                    self.status = format!("Açıldı: {}", path.display());
                    self.path = Some(path);
                }
                Err(e) => self.status = e,
            }
        }
    }

    fn save(&mut self, ask: bool) {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("AntAn projesi", &[project::EXT]).set_file_name(format!("{}.{}", self.project.model.name(), project::EXT)).save_file(),
        };
        let Some(path) = path else { return };
        match self.project.save(&path) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.status = format!("Kaydedildi: {}", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = format!("Kaydedilemedi: {e}"),
        }
    }

    fn export_nec(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("NEC kart dosyası", &["nec"]).set_file_name(format!("{}.nec", self.project.model.name())).save_file() else { return };
        let f = self.antenna.frequency;
        let span = f * self.project.sweep_span_pct / 100.0;
        let text = nec::export(&self.antenna, Some((f - span, f + span, self.project.sweep_points)));
        self.status = match std::fs::write(&path, text) {
            Ok(()) => format!("NEC dosyası yazıldı: {}", path.display()),
            Err(e) => format!("Yazılamadı: {e}"),
        };
    }

    fn save_png(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.window_rect else { return };
        if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("antan.png").save_file() {
            self.shotter.request(ctx, rect, path);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (o, s, z, y) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
            )
        });
        if o {
            self.open_dialog();
        }
        if s {
            self.save(false);
        }
        if z {
            self.undo();
        }
        if y {
            self.redo();
        }
    }

    // ------------------------------------------------------------ menüler

    fn new_menu(&mut self, ui: &mut egui::Ui) {
        let f = self.project.model.frequency();
        ui.label(RichText::new("Yagi-Uda (eleman bazlı)").small().color(theme::TEXT_DIM));
        for n in [2usize, 3, 4, 5, 6, 7, 9, 11] {
            if ui.button(format!("{n} elemanlı Yagi")).clicked() {
                let mut p = Project::yagi(f, n);
                p.z0 = self.project.z0;
                self.replace_project(p);
                ui.close();
            }
        }
        if ui.button("7 elemanlı kademeli borulu Yagi (örnek)").clicked() {
            self.replace_project(tapered_demo());
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Tel modelleri").small().color(theme::TEXT_DIM));
        for k in Kind::ALL {
            if ui.button(k.label()).clicked() {
                self.replace_project(Project::template(k, f));
                ui.close();
            }
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Dosya", |ui| {
                ui.menu_button("Yeni", |ui| self.new_menu(ui));
                if ui.button("Aç…  (Ctrl+O)").clicked() {
                    ui.close();
                    self.open_dialog();
                }
                if ui.button("Kaydet  (Ctrl+S)").clicked() {
                    ui.close();
                    self.save(false);
                }
                if ui.button("Farklı kaydet…").clicked() {
                    ui.close();
                    self.save(true);
                }
                ui.separator();
                if ui.button("NEC-2 dosyası olarak dışa aktar…").clicked() {
                    ui.close();
                    self.export_nec();
                }
                if ui.button("Pencereyi PNG olarak kaydet…").clicked() {
                    ui.close();
                    self.save_png(&ctx);
                }
                ui.separator();
                if ui.button("Çıkış").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Düzen", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Geri al  (Ctrl+Z)")).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Yinele  (Ctrl+Y)")).clicked() {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if let Model::Yagi(d) = &self.project.model {
                    if ui.button("Tel modeline dönüştür").on_hover_text("Yagi tasarımını serbestçe düzenlenebilir tellere çevirir").clicked() {
                        let mut p = self.project.clone();
                        p.model = Model::Wires(d.build());
                        self.replace_project(p);
                        ui.close();
                    }
                }
            });
            ui.menu_button("Görünüm", |ui| {
                for (t, l) in [(Tab::Pattern3d, "3B diyagram"), (Tab::Cuts, "Kesitler ve akımlar"), (Tab::Sweep, "SWR / empedans / Smith"), (Tab::Matching, "Eşleme"), (Tab::Optimize, "İyileştirme")] {
                    if ui.selectable_label(self.tab == t, l).clicked() {
                        self.tab = t;
                        ui.close();
                    }
                }
            });
            ui.menu_button("Yardım", |ui| {
                if ui.button("AntAn hakkında").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("AntAn").size(20.0).strong().color(theme::ACCENT));
            ui.label(RichText::new("anten analiz ve hesaplama").color(theme::TEXT_DIM));
            ui.separator();
            ui.menu_button("➕ Yeni", |ui| self.new_menu(ui));
            if ui.button("📂 Aç").clicked() {
                self.open_dialog();
            }
            if ui.button("💾 Kaydet").clicked() {
                self.save(false);
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶")).on_hover_text("Geri al").clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text("Yinele").clicked() {
                self.redo();
            }
            ui.separator();
            ui.label("Bant:");
            for (label, f) in [("10 m", 28.5e6), ("6 m", 50.15e6), ("2 m", 144.3e6), ("70 cm", 432.2e6), ("23 cm", 1296.2e6)] {
                if ui.small_button(label).on_hover_text(if self.scale_with_freq { "Frekansı değiştir ve geometriyi ölçekle" } else { "Yalnız analiz frekansını değiştir" }).clicked() {
                    let old = self.project.model.frequency();
                    self.project.model.set_frequency(f);
                    if self.scale_with_freq {
                        self.project.model.scale(old / f);
                    }
                }
            }
            ui.checkbox(&mut self.scale_with_freq, "ölçekle").on_hover_text("İşaretliyse bant değişince tüm boyutlar yeni dalga boyuna ölçeklenir");
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ok = matches!(self.solution, Some(Ok(_)));
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, if ok { theme::GOOD } else { theme::BAD });
            ui.label(RichText::new(&self.status).small());
            if let Some(s) = self.sol() {
                let z = s.impedance();
                let swr = matching::swr(z, self.project.z0);
                ui.separator();
                ui.label(format!("Z = {}", fmt_z(z)));
                ui.separator();
                ui.label(RichText::new(format!("SWR {swr:.2}")).color(theme::swr_color(swr)));
                if let Some(sm) = &self.summary {
                    ui.separator();
                    ui.label(format!("G {:.2} dBi", sm.max_gain_dbi));
                    ui.separator();
                    ui.label(format!("Ö/A {:.1} dB", sm.front_to_back_db));
                }
                ui.separator();
                ui.label(format!("Verim {:.1}%", s.efficiency() * 100.0));
                ui.separator();
                ui.label(RichText::new(format!("{} bilinmeyen · {:.0} ms", s.basis.len(), self.solve_ms)).small().color(theme::TEXT_DIM));
            }
            if self.sweep_job.pending() {
                ui.separator();
                ui.spinner();
                ui.label(RichText::new("tarama…").small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "kaydedilmedi".into());
                ui.label(RichText::new(name).small().color(theme::TEXT_DIM));
                ui.separator();
                ui.label(RichText::new(self.project.model.ground().label()).small().color(theme::TEXT_DIM));
            });
        });
    }

    fn about(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("AntAn hakkında").open(&mut open).collapsible(false).resizable(false).default_width(460.0).show(ctx, |ui| {
            ui.label(RichText::new("AntAn — anten analiz ve hesaplama").heading().color(theme::ACCENT));
            ui.label(format!("Sürüm {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(6.0);
            ui.label("Çözücü: serbest uzayda ince tel moment yöntemi (Harrington, karışık potansiyel, darbe tabanlı / nokta eşlemeli). Tel birleşimleri, iletken kaybı (deri etkisi, boru et kalınlığı) ve kademeli elemanlar desteklenir.");
            ui.add_space(4.0);
            ui.label("Doğrulama: yarım dalga dipol (≈73 + j42 Ω, 2.15 dBi), enerji korunumu (kazanç = yönlülük), Kirchhoff akım yasası, bölünmüş/tek parça eşdeğerliği.");
            ui.add_space(4.0);
            ui.label(RichText::new("Sınırlar: toprak etkisi henüz yok (serbest uzay); NEC ile birkaç yüzdelik farklar normaldir.").color(theme::TEXT_DIM));
        });
        self.show_about = open;
    }

    // ------------------------------------------------------------ sekmeler

    fn kpi(ui: &mut egui::Ui, title: &str, value: String, sub: String, color: Color32) {
        egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).small().color(theme::TEXT_DIM));
            ui.label(RichText::new(value).size(20.0).strong().color(color));
            if !sub.is_empty() {
                ui.label(RichText::new(sub).small().color(theme::TEXT_DIM));
            }
        });
    }

    fn summary_cards(&self, ui: &mut egui::Ui) {
        let (Some(sol), Some(s)) = (self.sol(), &self.summary) else { return };
        let z = sol.impedance();
        let swr = matching::swr(z, self.project.z0);
        let lambda = C0 / self.antenna.frequency;
        Self::kpi(ui, "Kazanç", format!("{:.2} dBi", s.max_gain_dbi), format!("{:.2} dBd · yönlülük {:.2} dBi", s.max_gain_dbi - 2.15, s.directivity_dbi), Color32::WHITE);
        Self::kpi(ui, "Ön / arka", format!("{:.1} dB", s.front_to_back_db), String::new(), theme::ACCENT);
        let ground = self.antenna.ground;
        if !ground.is_free() {
            let (lo, hi) = self.antenna.bounds();
            Self::kpi(
                ui,
                "Kalkış açısı",
                format!("{:.0}°", 90.0 - s.max_theta.to_degrees()),
                format!("{} · yükseklik {:.2}–{:.2} m", ground.label(), lo[2].max(0.0), hi[2]),
                theme::WARM,
            );
        }
        Self::kpi(ui, "Besleme empedansı", fmt_z(z), format!("|Γ| = {:.3} · RL {:.1} dB", matching::reflection(z, self.project.z0).norm(), matching::return_loss_db(z, self.project.z0)), Color32::WHITE);
        Self::kpi(ui, &format!("SWR ({:.0} Ω)", self.project.z0), format!("{swr:.2}"), match self.band() {
            Some((lo, hi)) => format!("2:1 bant: {:.2}–{:.2} MHz ({:.0} kHz)", lo / 1e6, hi / 1e6, (hi - lo) / 1e3),
            None if self.sweep_job.pending() => "bant hesaplanıyor…".into(),
            None => "2:1 bant yok".into(),
        }, theme::swr_color(swr));
        Self::kpi(ui, "Huzme genişliği (−3 dB)", format!("{:.0}° / {:.0}°", s.beamwidth_az_deg, s.beamwidth_el_deg), "yatay / düşey".into(), Color32::WHITE);
        let (lo, hi) = self.antenna.bounds();
        Self::kpi(
            ui,
            "Verim ve boyut",
            format!("{:.2} %", sol.efficiency() * 100.0),
            format!(
                "kayıp {:.3} dB{} · {:.0} × {:.0} × {:.0} mm (X·Y·Z), en büyük {:.2} λ",
                -10.0 * sol.efficiency().max(1e-9).log10(),
                if sol.loads.is_empty() { String::new() } else { format!(" (yüklerde %{:.1})", sol.loads.iter().map(|l| l.power).sum::<f64>() / sol.input_power * 100.0) },
                (hi[0] - lo[0]) * 1e3,
                (hi[1] - lo[1]) * 1e3,
                (hi[2] - lo[2]) * 1e3,
                (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f64::max) / lambda
            ),
            Color32::WHITE,
        );
    }

    fn pattern_tab(&mut self, ui: &mut egui::Ui) {
        view3d::toolbar(ui, &mut self.view);
        ui.add_space(4.0);
        let avail = ui.available_size();
        let highlight = self.highlight();
        ui.horizontal_top(|ui| {
            let w = (avail.x - 250.0).max(200.0);
            let sol = match &self.solution {
                Some(Ok(s)) => Some(s),
                _ => None,
            };
            let max_db = self.summary.map(|s| s.max_gain_dbi).unwrap_or(0.0);
            let boom = match &self.project.model {
                Model::Yagi(d) => {
                    let (lo, hi) = d.elements.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), e| (a.min(e.x), b.max(e.x)));
                    let z = self.antenna.wires.first().map(|w| w.start[2]).unwrap_or(0.0);
                    Some(([lo - d.boom.overhang, 0.0, z], [hi + d.boom.overhang, 0.0, z]))
                }
                Model::Wires(_) => None,
            };
            view3d::show(ui, Vec2::new(w, avail.y - 4.0), self.pattern.as_ref(), max_db, &self.antenna, sol, &highlight, boom, &mut self.view);
            ui.vertical(|ui| {
                ui.set_width(240.0);
                self.summary_cards(ui);
            });
        });
    }

    fn cuts_tab(&mut self, ui: &mut egui::Ui) {
        let (Some(sol), Some(s)) = (self.sol(), self.summary) else { return };
        let az = farfield::azimuth_cut(sol, s.max_theta, 360);
        let el = farfield::elevation_cut(sol, s.max_phi, 360);
        let mut range = self.cut_range_db;
        ui.horizontal(|ui| {
            ui.label("Ölçek aralığı");
            ui.add(egui::Slider::new(&mut range, 10.0..=60.0).suffix(" dB"));
            ui.label(RichText::new(format!("Yatay kesit θ = {:.0}° (yükseliş {:.0}°), düşey kesit φ = {:.0}°", s.max_theta.to_degrees(), 90.0 - s.max_theta.to_degrees(), s.max_phi.to_degrees())).color(theme::TEXT_DIM));
        });
        let avail = ui.available_size();
        let h = ((avail.y - 190.0).max(260.0)).min(avail.x * 0.5 - 6.0);
        let w = (avail.x - 8.0) * 0.5;
        ui.horizontal_top(|ui| {
            viz::polar_plot(ui, Vec2::new(w, h), "Yatay (azimut) kesit", &az, Polar::Azimuth, range, &PolarInfo { max_gain_dbi: s.max_gain_dbi, beamwidth_deg: s.beamwidth_az_deg, fb_db: s.front_to_back_db });
            viz::polar_plot(ui, Vec2::new(w, h), "Düşey (yükseliş) kesit", &el, Polar::Elevation, range, &PolarInfo { max_gain_dbi: s.max_gain_dbi, beamwidth_deg: s.beamwidth_el_deg, fb_db: s.front_to_back_db });
        });
        ui.add_space(6.0);
        // Elemanların akım dağılımı (Yagi: her elemanın orta teli + bölümleri birleşik).
        let mut series = Vec::new();
        match &self.project.model {
            Model::Yagi(d) => {
                for (i, e) in d.elements.iter().enumerate() {
                    let range = &self.wire_map[i];
                    // Elemanı uçtan uca örnekle: y koordinatına göre tüm parçalardaki akımlar.
                    let half = 0.5 * e.length;
                    let mut pts: Vec<(f64, f64)> = Vec::new();
                    for wi in range.clone() {
                        let w = &self.antenna.wires[wi];
                        for (t, cur) in sol.wire_currents(wi) {
                            let y = w.start[1] + (w.end[1] - w.start[1]) * t;
                            pts.push(((y + half) / (2.0 * half), cur.norm()));
                        }
                    }
                    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    series.push((e.name.clone(), pts));
                }
            }
            Model::Wires(a) => {
                for (i, w) in a.wires.iter().enumerate().take(12) {
                    series.push((w.name.clone(), sol.wire_currents(i).into_iter().map(|(t, c)| (t, c.norm())).collect()));
                }
            }
        }
        viz::current_plot(ui, Vec2::new(ui.available_width(), ui.available_height().max(120.0) - 4.0), &series, Some(self.selected));
        self.cut_range_db = range;
    }

    fn sweep_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Tarama: ±");
            ui.add(egui::DragValue::new(&mut self.project.sweep_span_pct).range(0.2..=40.0).speed(0.1).suffix(" %"));
            ui.label("nokta");
            ui.add(egui::DragValue::new(&mut self.project.sweep_points).range(5..=401));
            if self.sweep_job.pending() {
                ui.spinner();
                ui.label("hesaplanıyor (tüm çekirdekler)…");
            }
            if let Some((lo, hi)) = self.band() {
                ui.separator();
                ui.label(RichText::new(format!("SWR < 2 bandı: {:.3} – {:.3} MHz  ({:.0} kHz, %{:.2})", lo / 1e6, hi / 1e6, (hi - lo) / 1e3, (hi - lo) / self.antenna.frequency * 100.0)).color(theme::GOOD));
            }
        });
        let Some(sweep) = self.sweep.clone() else {
            ui.weak("Tarama sonucu bekleniyor…");
            return;
        };
        let avail = ui.available_size();
        let smith = (avail.y - 6.0).min(avail.x * 0.45);
        let left_w = avail.x - smith - 8.0;
        let band = self.band();
        let f = self.antenna.frequency;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let h = (avail.y - 10.0) * 0.5;
                viz::swr_chart(ui, Vec2::new(left_w, h), &sweep, self.project.z0, f, band);
                viz::rx_chart(ui, Vec2::new(left_w, h), &sweep, self.project.z0, f);
            });
            ui.vertical(|ui| {
                viz::smith_chart(ui, Vec2::new(smith, smith.min(avail.y * 0.62)), &sweep, self.project.z0, f);
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("sweep_table").show(ui, |ui| {
                    egui::Grid::new("sweep_tbl").num_columns(5).striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
                        for h in ["MHz", "R (Ω)", "X (Ω)", "SWR", "RL (dB)"] {
                            ui.label(RichText::new(h).small().color(theme::TEXT_DIM));
                        }
                        ui.end_row();
                        let step = (sweep.len() / 20).max(1);
                        for q in sweep.iter().step_by(step) {
                            let s = matching::swr(q.impedance, self.project.z0);
                            ui.label(format!("{:.3}", q.frequency / 1e6));
                            ui.label(format!("{:.1}", q.impedance.re));
                            ui.label(format!("{:+.1}", q.impedance.im));
                            ui.label(RichText::new(format!("{s:.2}")).color(theme::swr_color(s)));
                            ui.label(format!("{:.1}", matching::return_loss_db(q.impedance, self.project.z0)));
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }

    fn matching_tab(&mut self, ui: &mut egui::Ui) {
        let Some(sol) = self.sol() else { return };
        let z = sol.impedance();
        let z0 = self.project.z0;
        let f = self.antenna.frequency;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Besleme: {}", fmt_z(z))).size(16.0).strong());
            let swr = matching::swr(z, z0);
            ui.label(RichText::new(format!("SWR {swr:.2} ({z0:.0} Ω)")).size(16.0).color(theme::swr_color(swr)));
        });
        ui.add_space(4.0);
        let card = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(RichText::new(title).strong().color(theme::ACCENT));
                    ui.separator();
                    add(ui);
                });
            });
        };
        let mut apply: Option<Project> = None;
        let driven_d = self.antenna.wires.get(self.antenna.sources[0].wire).map(|w| w.diameter).unwrap_or(0.01);
        let (mut rod, mut spacing) = (self.project.match_rod, self.project.match_spacing);
        ui.columns(3, |cols| {
            card(&mut cols[0], "L devresi (toplu elemanlar)", &mut |ui| {
                let nets = matching::l_network(z, z0, f);
                if nets.is_empty() {
                    ui.weak("Bu empedans için L devresi gerekmez ya da çözüm yok.");
                }
                for (i, n) in nets.iter().enumerate() {
                    ui.label(RichText::new(format!("Çözüm {}", i + 1)).strong());
                    ui.label(RichText::new(if n.shunt_at_load { "Yüke paralel eleman, hat tarafına seri eleman" } else { "Yüke seri eleman, hat tarafına paralel eleman" }).color(theme::TEXT_DIM));
                    ui.label(format!("  Seri:    {}", reactive_str(n.series)));
                    ui.label(format!("  Paralel: {}", reactive_str(n.shunt)));
                    let check = matching::l_network_input(z, n, f);
                    ui.label(RichText::new(format!("  Kontrol: {}", fmt_z(check))).small().color(theme::TEXT_DIM));
                    ui.add_space(4.0);
                }
            });
            card(&mut cols[1], "Hairpin (beta) eşleme", &mut |ui| {
                egui::Grid::new("hp").num_columns(2).show(ui, |ui| {
                    ui.label("Çubuk çapı");
                    let mut v = rod * 1e3;
                    if ui.add(egui::DragValue::new(&mut v).range(0.5..=30.0).speed(0.1).suffix(" mm")).changed() {
                        rod = v / 1e3;
                    }
                    ui.end_row();
                    ui.label("Çubuk aralığı (merkez)");
                    let mut v = spacing * 1e3;
                    if ui.add(egui::DragValue::new(&mut v).range(2.0..=300.0).speed(0.5).suffix(" mm")).changed() {
                        spacing = v / 1e3;
                    }
                    ui.end_row();
                });
                match matching::hairpin(z, z0, f, rod, spacing) {
                    Some(h) => {
                        ui.label(format!("Gerekli eleman reaktansı: {:.1} Ω", h.required_reactance));
                        let change = if h.reactance_change.abs() < 0.5 { "uygun ✔".to_string() } else if h.reactance_change < 0.0 { format!("{:.1} Ω daha kapasitif → elemanı kısaltın", -h.reactance_change) } else { format!("{:.1} Ω daha endüktif → elemanı uzatın", h.reactance_change) };
                        ui.label(RichText::new(format!("Şu anki durum: {change}")).color(if h.reactance_change.abs() < 0.5 { theme::GOOD } else { theme::WARM }));
                        ui.label(format!("Hairpin endüktansı: {:.1} nH ({:.1} Ω)", h.inductance * 1e9, h.shunt_reactance));
                        ui.label(RichText::new(format!("Hairpin boyu: {:.1} mm", h.length * 1e3)).strong());
                        ui.label(RichText::new(format!("(çift hat Z₀ = {:.0} Ω, kısa devre ucu)", h.line_z0)).small().color(theme::TEXT_DIM));
                    }
                    None => {
                        ui.label(RichText::new("Hairpin için besleme direnci Z₀'dan küçük olmalı.").color(theme::WARM));
                    }
                }
            });
            card(&mut cols[2], "Gamma eşleme", &mut |ui| {
                ui.label(RichText::new(format!("Eleman çapı {:.1} mm; çubuk/aralık hairpin ile ortak.", driven_d * 1e3)).small().color(theme::TEXT_DIM));
                let p = matching::GammaParams { element_diameter: driven_d, rod_diameter: rod, spacing };
                match matching::gamma(z, z0, f, p) {
                    Some(g) => {
                        ui.label(RichText::new(format!("Gamma çubuğu boyu: {:.1} mm", g.length * 1e3)).strong());
                        ui.label(RichText::new(format!("Seri kondansatör: {:.1} pF", g.capacitance * 1e12)).strong());
                        ui.label(format!("Empedans yükseltme oranı: {:.2}×", g.step_up));
                        ui.label(format!("Gamma bölümü hat Z₀: {:.0} Ω", g.line_z0));
                        ui.label(RichText::new("Yaklaşık model (Healey). Rezonanstaki elemanla en iyi sonuç verir.").small().color(theme::TEXT_DIM));
                    }
                    None => {
                        ui.label(RichText::new("Bu geometri için çözüm bulunamadı (çubuk boyu 0.2 λ'yı aşıyor).").color(theme::WARM));
                    }
                }
            });
        });
        self.project.match_rod = rod;
        self.project.match_spacing = spacing;

        if let Model::Yagi(d) = &self.project.model {
            ui.add_space(8.0);
            egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Sürülen elemanı otomatik ayarla").strong().color(theme::ACCENT));
                ui.label(RichText::new("Sürülen elemanın boyu, gerçek çözücüyle yinelenerek hedef besleme reaktansına getirilir.").small().color(theme::TEXT_DIM));
                ui.horizontal(|ui| {
                    if ui.button("Rezonansa getir (X = 0, gamma/doğrudan besleme)").clicked() {
                        match d.tune_driven(|_| Some(0.0)) {
                            Some((nd, nz)) => {
                                let mut p = self.project.clone();
                                p.model = Model::Yagi(nd.clone());
                                apply = Some(p);
                                self.tune_msg = Some(format!("Sürülen eleman {:.1} → {:.1} mm, Z = {}", d.elements[d.driven].length * 1e3, nd.elements[d.driven].length * 1e3, fmt_z(nz)));
                            }
                            None => self.tune_msg = Some("Ayar yakınsamadı.".into()),
                        }
                    }
                    if ui.button("Hairpin için kapasitif yap (X = −√(R(Z₀−R)))").clicked() {
                        match d.tune_driven(|zz| (zz.re < z0).then(|| -(zz.re * (z0 - zz.re)).sqrt())) {
                            Some((nd, nz)) => {
                                let mut p = self.project.clone();
                                p.model = Model::Yagi(nd.clone());
                                apply = Some(p);
                                let hl = matching::hairpin(nz, z0, f, rod, spacing).map(|h| format!(", hairpin {:.1} mm", h.length * 1e3)).unwrap_or_default();
                                self.tune_msg = Some(format!("Sürülen eleman {:.1} → {:.1} mm, Z = {}{hl}", d.elements[d.driven].length * 1e3, nd.elements[d.driven].length * 1e3, fmt_z(nz)));
                            }
                            None => self.tune_msg = Some("Ayar yakınsamadı (R ≥ Z₀ olabilir).".into()),
                        }
                    }
                });
                if let Some(m) = &self.tune_msg {
                    ui.label(RichText::new(m).color(theme::GOOD));
                }
            });
        }
        if let Some(p) = apply {
            self.replace_project(p);
        }
    }

    fn optimize_tab(&mut self, ui: &mut egui::Ui) {
        let design = match &self.project.model {
            Model::Yagi(d) => Some(d.clone()),
            _ => None,
        };
        let mut goals = self.project.goals.clone();
        goals.z0 = if goals.z0 > 0.0 { goals.z0 } else { self.project.z0 };
        if let Some(best) = self.optim.ui(ui, design.as_ref(), &mut goals) {
            let mut p = self.project.clone();
            p.model = Model::Yagi(best);
            p.goals = goals.clone();
            self.replace_project(p);
            self.status = "İyileştirme sonucu uygulandı (Ctrl+Z ile geri alınabilir)".into();
        }
        self.project.goals = goals;
    }
}

fn reactive_str(r: Reactive) -> String {
    match r {
        Reactive::Inductor(l) => format!("{:.1} nH  (L)", l * 1e9),
        Reactive::Capacitor(c) => format!("{:.1} pF  (C)", c * 1e12),
    }
}

/// Örnek: 7 elemanlı, ortası 16 mm, sonra 12 mm, uçları 10 mm borulu 2 m Yagi.
fn tapered_demo() -> Project {
    use antan_core::templates::Tube;
    use antan_core::yagi::{Profile, TaperSection, YagiDesign};
    let profile = Profile {
        sections: vec![
            TaperSection { half_length: 0.10, tube: Tube::aluminum(0.016, Some(0.0015)) },
            TaperSection { half_length: 0.08, tube: Tube::aluminum(0.012, Some(0.001)) },
        ],
        tip: Tube::aluminum(0.010, Some(0.001)),
    };
    let mut d = YagiDesign::standard(144.3e6, 7, profile);
    d.name = "7 el. kademeli Yagi".into();
    Project::with_model(Model::Yagi(d))
}

impl eframe::App for AntAnApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.window_rect = Some(ui.max_rect());
        self.shortcuts(&ctx);
        self.recompute_if_needed(&ctx);
        if let Some(r) = self.sweep_job.poll() {
            match r {
                Ok(s) => self.sweep = Some(s),
                Err(e) => {
                    self.sweep = None;
                    self.status = format!("Tarama: {e}");
                }
            }
        }
        if let Some(res) = self.shotter.poll(&ctx) {
            self.status = match res {
                Ok(p) => format!("Görüntü kaydedildi: {}", p.display()),
                Err(e) => format!("Görüntü kaydedilemedi: {e}"),
            };
            if self.autoshot.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Etkileşimsiz doğrulama: tarama (ve istenirse iyileştirme) bitince
        // ekran görüntüsü alıp kapan.
        if let Some(path) = self.autoshot.clone() {
            self.autoshot_frame += 1;
            if self.autoshot_frame == 2
                && let (Ok(n), Model::Yagi(d)) = (std::env::var("ANTAN_AUTOSHOT_OPTIM"), &self.project.model)
            {
                self.optim.start(&ctx, d.clone(), self.project.goals.clone(), n.parse().unwrap_or(150));
            }
            if self.autoshot_frame > 8 && !self.sweep_job.pending() && !self.optim.busy() && self.autoshot_frame < 1000 {
                if let Some(rect) = self.window_rect {
                    self.shotter.request(&ctx, rect, path);
                    self.autoshot_frame = 1000;
                }
            } else {
                ctx.request_repaint();
            }
        }

        let title = format!(
            "AntAn — {}{}",
            self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.project.model.name().to_string()),
            if self.dirty() { " •" } else { "" }
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(3.0);
            self.toolbar(ui);
            ui.add_space(3.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("tree").resizable(true).default_size(215.0).size_range(170.0..=360.0).show(ui, |ui| editor::model_tree(self, ui));
        egui::Panel::right("props").resizable(true).default_size(400.0).size_range(320.0..=620.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(2.0);
                editor::properties(self, ui);
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (t, l) in [(Tab::Pattern3d, "🌐 3B diyagram"), (Tab::Cuts, "◔ Kesitler ve akımlar"), (Tab::Sweep, "📈 SWR / Smith"), (Tab::Matching, "🔌 Eşleme"), (Tab::Optimize, "⚙ İyileştirme")] {
                    let sel = self.tab == t;
                    if ui.add(egui::Button::selectable(sel, RichText::new(l).size(14.0))).clicked() {
                        self.tab = t;
                    }
                }
            });
            ui.separator();
            if self.sol().is_none() && self.tab != Tab::Optimize {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Çözüm yok").heading().color(theme::BAD));
                    ui.label(self.status.clone());
                });
                return;
            }
            match self.tab {
                Tab::Pattern3d => self.pattern_tab(ui),
                Tab::Cuts => self.cuts_tab(ui),
                Tab::Sweep => self.sweep_tab(ui),
                Tab::Matching => self.matching_tab(ui),
                Tab::Optimize => self.optimize_tab(ui),
            }
        });
        self.about(&ctx);
        let n = match &self.project.model {
            Model::Yagi(d) => d.elements.len(),
            Model::Wires(a) => a.wires.len(),
        };
        self.selected = self.selected.min(n.saturating_sub(1));
        self.track_undo(&ctx);
    }
}
