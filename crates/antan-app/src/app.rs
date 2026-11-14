//! AntAn uygulama kabuğu: durum, menüler, düzen ve sekmeler.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, Vec2};
use antan_core::i18n::{Language, language, set_language, t as tl};
use antan_core::tf;


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
    /// Kayıtlı arayüz ölçeği; Ctrl +/− ile değişince ayarlara yazılır.
    text_scale: f32,
    /// Değer kartları alanının yüksekliği (önceki kareden), 3B görünüm boyu için.
    cards_h: f32,
    pub heightscan: crate::heightscan::HeightScan,
    suggest: crate::suggest::Suggest,

    shotter: shot::Shotter,
    window_rect: Option<egui::Rect>,
    autoshot: Option<PathBuf>,
    autoshot_frame: u32,
}

impl AntAnApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        // Dil: kayıtlı ayar; ANTAN_LANG=tr|en (doğrulama için) onu geçersiz kılar.
        let lang = match std::env::var("ANTAN_LANG").ok().as_deref() {
            Some("tr") => Language::Turkish,
            Some("en") => Language::English,
            _ => settings.language,
        };
        // Yazı boyutu: kayıtlı ayar; ANTAN_SCALE=1.3 gibi (doğrulama için) onu geçersiz kılar.
        let text_scale = std::env::var("ANTAN_SCALE").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(settings.text_scale).clamp(0.5, 3.0);
        cc.egui_ctx.set_zoom_factor(text_scale);
        set_language(lang);
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
            Some("lfa") => Project::lfa(144.3e6, 7),
            Some("loaded_dipole") => Project::template(Kind::LoadedDipole, 7.1e6),
            Some("dipole_ground") => {
                let mut p = Project::template(Kind::Dipole, 14.175e6);
                if let Model::Wires(a) = &mut p.model {
                    a.ground = antan_core::geometry::Ground::Real { permittivity: 13.0, conductivity: 0.005 };
                    *a = antan_core::scan::at_height(a, 10.0);
                }
                p
            }
            _ => Project::yagi(144.3e6, 5),
        };
        let mut project = project;
        if let Some(mhz) = std::env::var("ANTAN_AUTOSHOT_FREQ").ok().and_then(|v| v.parse::<f64>().ok()) {
            project.model.set_frequency(mhz * 1e6);
        }
        let mut app = Self {
            saved: project.clone(),
            committed: project.clone(),
            project,
            selected: 1,
            status: tl("Ready", "Hazır").into(),
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
            text_scale,
            cards_h: 90.0,
            heightscan: Default::default(),
            suggest: Default::default(),
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
                self.status = tl("Solved", "Çözüldü").into();
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
        viz::swr_bandwidth(&self.fed_sweep()?, self.project.z0, self.project.model.frequency(), 2.0)
    }

    /// Antenin en alt noktasını (Yagi: bumu) `h` metre yüksekliğe taşır.
    pub fn set_height(&mut self, h: f64) {
        match &mut self.project.model {
            Model::Yagi(d) => d.height = h,
            Model::Wires(a) => *a = antan_core::scan::at_height(a, h),
        }
        self.status = tf!("Height set to {:.2} m", "Yükseklik {:.2} m yapıldı", h);
    }

    /// Besleme noktasında hat tarafından görülen empedans: hairpin takılıysa
    /// antene paralel endüktans (ωL frekansla değişir).
    fn fed(&self, z: antan_core::Complex64, f: f64) -> antan_core::Complex64 {
        crate::project::with_hairpin(z, f, self.project.hairpin_l)
    }

    /// Hairpin dahil tarama sonucu.
    fn fed_sweep(&self) -> Option<Vec<SweepPoint>> {
        Some(self.sweep.as_ref()?.iter().map(|p| SweepPoint { frequency: p.frequency, impedance: self.fed(p.impedance, p.frequency) }).collect())
    }

    // ------------------------------------------------------------ dosya

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter(tl("AntAn project", "AntAn projesi"), &[project::EXT]).pick_file() {
            match Project::load(&path) {
                Ok(p) => {
                    self.replace_project(p);
                    self.saved = self.project.clone();
                    self.status = tf!("Opened: {}", "Açıldı: {}", path.display());
                    self.path = Some(path);
                }
                Err(e) => self.status = e,
            }
        }
    }

    fn save(&mut self, ask: bool) {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter(tl("AntAn project", "AntAn projesi"), &[project::EXT]).set_file_name(format!("{}.{}", self.project.model.name(), project::EXT)).save_file(),
        };
        let Some(path) = path else { return };
        match self.project.save(&path) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.status = tf!("Saved: {}", "Kaydedildi: {}", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = tf!("Could not save: {e}", "Kaydedilemedi: {e}"),
        }
    }

    fn export_nec(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter(tl("NEC card file", "NEC kart dosyası"), &["nec"]).set_file_name(format!("{}.nec", self.project.model.name())).save_file() else { return };
        let f = self.antenna.frequency;
        let span = f * self.project.sweep_span_pct / 100.0;
        let text = nec::export(&self.antenna, Some((f - span, f + span, self.project.sweep_points)));
        self.status = match std::fs::write(&path, text) {
            Ok(()) => tf!("NEC file written: {}", "NEC dosyası yazıldı: {}", path.display()),
            Err(e) => tf!("Could not write: {e}", "Yazılamadı: {e}"),
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
        if ui.button(tl("💡 Suggestions for a frequency…", "💡 Bir frekans için öneriler…")).clicked() {
            self.suggest.open = true;
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new(tl("Yagi-Uda (element based)", "Yagi-Uda (eleman bazlı)")).small().color(theme::TEXT_DIM));
        for n in [2usize, 3, 4, 5, 6, 7, 9, 11] {
            if ui.button(tf!("{n}-element Yagi", "{n} elemanlı Yagi")).clicked() {
                let mut p = Project::yagi(f, n);
                p.z0 = self.project.z0;
                self.replace_project(p);
                ui.close();
            }
        }
        if ui.button(tl("7-element tapered-tube Yagi (example)", "7 elemanlı kademeli borulu Yagi (örnek)")).clicked() {
            self.replace_project(tapered_demo());
            ui.close();
        }
        ui.menu_button(tl("LFA Yagi (loop fed array)", "LFA Yagi (halka beslemeli)"), |ui| {
            for n in [3usize, 4, 5, 6, 7, 9, 11] {
                if ui.button(tf!("{n}-element LFA", "{n} elemanlı LFA")).clicked() {
                    let mut p = Project::lfa(f, n);
                    p.z0 = 50.0;
                    self.replace_project(p);
                    ui.close();
                }
            }
        });
        ui.separator();
        ui.label(RichText::new(tl("Wire models", "Tel modelleri")).small().color(theme::TEXT_DIM));
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
            ui.menu_button(tl("File", "Dosya"), |ui| {
                ui.menu_button(tl("New", "Yeni"), |ui| self.new_menu(ui));
                if ui.button(tl("Open…  (Ctrl+O)", "Aç…  (Ctrl+O)")).clicked() {
                    ui.close();
                    self.open_dialog();
                }
                if ui.button(tl("Save  (Ctrl+S)", "Kaydet  (Ctrl+S)")).clicked() {
                    ui.close();
                    self.save(false);
                }
                if ui.button(tl("Save as…", "Farklı kaydet…")).clicked() {
                    ui.close();
                    self.save(true);
                }
                ui.separator();
                if ui.button(tl("Export as NEC-2 file…", "NEC-2 dosyası olarak dışa aktar…")).clicked() {
                    ui.close();
                    self.export_nec();
                }
                if ui.button(tl("Save window as PNG…", "Pencereyi PNG olarak kaydet…")).clicked() {
                    ui.close();
                    self.save_png(&ctx);
                }
                ui.separator();
                if ui.button(tl("Quit", "Çıkış")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(tl("Edit", "Düzen"), |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(tl("Undo  (Ctrl+Z)", "Geri al  (Ctrl+Z)"))).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new(tl("Redo  (Ctrl+Y)", "Yinele  (Ctrl+Y)"))).clicked() {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if let Model::Yagi(d) = &self.project.model {
                    if ui.button(tl("Convert to wire model", "Tel modeline dönüştür")).on_hover_text(tl("Turns the Yagi design into freely editable wires", "Yagi tasarımını serbestçe düzenlenebilir tellere çevirir")).clicked() {
                        let mut p = self.project.clone();
                        p.model = Model::Wires(d.build());
                        self.replace_project(p);
                        ui.close();
                    }
                }
            });
            ui.menu_button(tl("View", "Görünüm"), |ui| {
                for (t, l) in [(Tab::Pattern3d, tl("3D pattern", "3B diyagram")), (Tab::Cuts, tl("Cuts and currents", "Kesitler ve akımlar")), (Tab::Sweep, tl("SWR / impedance / Smith", "SWR / empedans / Smith")), (Tab::Matching, tl("Matching", "Eşleme")), (Tab::Optimize, tl("Optimization", "İyileştirme"))] {
                    if ui.selectable_label(self.tab == t, l).clicked() {
                        self.tab = t;
                        ui.close();
                    }
                }
                ui.separator();
                ui.menu_button(tl("Text size", "Yazı boyutu"), |ui| {
                    for s in theme::TEXT_SCALES {
                        if ui.selectable_label((self.text_scale - s).abs() < 0.01, format!("{:.0} %", s * 100.0)).clicked() {
                            ctx.set_zoom_factor(s);
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new(tl("Ctrl + / Ctrl − / Ctrl 0", "Ctrl + / Ctrl − / Ctrl 0")).color(theme::TEXT_DIM));
                });
            });
            // Dil menüsü her iki dilde de okunabilsin diye iki dilli başlıklı.
            ui.menu_button("🌐 Language / Dil", |ui| {
                for (l, name) in [(Language::English, "English"), (Language::Turkish, "Türkçe")] {
                    if ui.selectable_label(language() == l, name).clicked() {
                        set_language(l);
                        Settings::current(self.text_scale).save();
                        self.status = tl("Language changed. New templates use the selected language.", "Dil değişti. Yeni şablonlar seçilen dili kullanır.").into();
                        ui.close();
                    }
                }
            });
            ui.menu_button(tl("Help", "Yardım"), |ui| {
                if ui.button(tl("About AntAn", "AntAn hakkında")).clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("AntAn").size(20.0).strong().color(theme::ACCENT));
            ui.label(RichText::new(tl("antenna analysis and design", "anten analiz ve hesaplama")).color(theme::TEXT_DIM));
            ui.separator();
            ui.menu_button(tl("➕ New", "➕ Yeni"), |ui| self.new_menu(ui));
            if ui.button(tl("💡 Suggest", "💡 Öner")).on_hover_text(tl("Suggest and compare antennas for a frequency", "Bir frekans için anten öner ve karşılaştır")).clicked() {
                self.suggest.open = true;
            }
            if ui.button(tl("📂 Open", "📂 Aç")).clicked() {
                self.open_dialog();
            }
            if ui.button(tl("💾 Save", "💾 Kaydet")).clicked() {
                self.save(false);
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶")).on_hover_text(tl("Undo", "Geri al")).clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text(tl("Redo", "Yinele")).clicked() {
                self.redo();
            }
            ui.separator();
            ui.label(tl("Band:", "Bant:"));
            let cur = self.project.model.frequency();
            let current = antan_core::bands::band_of(cur).map(|b| b.name).unwrap_or("—");
            let hint = if self.scale_with_freq { tl("Change frequency and scale the geometry", "Frekansı değiştir ve geometriyi ölçekle") } else { tl("Change the analysis frequency only", "Yalnız analiz frekansını değiştir") };
            let mut pick = None;
            egui::ComboBox::from_id_salt("band").selected_text(current).width(80.0).height(600.0).show_ui(ui, |ui| {
                for (k, b) in antan_core::bands::BANDS.iter().enumerate() {
                    if k == 10 {
                        ui.separator();
                    }
                    if ui.selectable_label(current == b.name, format!("{:<6} {:>9.3} MHz", b.name, b.center / 1e6)).on_hover_text(hint).clicked() {
                        pick = Some(b.center);
                    }
                }
            })
            .response
            .on_hover_text(hint);
            if let Some(f) = pick {
                self.project.change_frequency(f, self.scale_with_freq);
            }
            ui.checkbox(&mut self.scale_with_freq, tl("scale", "ölçekle")).on_hover_text(tl("When checked, all dimensions are scaled to the new wavelength on band change", "İşaretliyse bant değişince tüm boyutlar yeni dalga boyuna ölçeklenir"));
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ok = matches!(self.solution, Some(Ok(_)));
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, if ok { theme::GOOD } else { theme::BAD });
            ui.label(RichText::new(&self.status).small());
            if let Some(s) = self.sol() {
                let z = self.fed(s.impedance(), self.antenna.frequency);
                let swr = matching::swr(z, self.project.z0);
                ui.separator();
                ui.label(format!("Z = {}", fmt_z(z)));
                ui.separator();
                ui.label(RichText::new(format!("SWR {swr:.2}")).color(theme::swr_color(swr)));
                if let Some(sm) = &self.summary {
                    ui.separator();
                    ui.label(format!("G {:.2} dBi", sm.max_gain_dbi));
                    ui.separator();
                    ui.label(tf!("F/B {:.1} dB", "Ö/A {:.1} dB", sm.front_to_back_db));
                }
                ui.separator();
                ui.label(tf!("Efficiency {:.1}%", "Verim {:.1}%", s.efficiency() * 100.0));
                ui.separator();
                ui.label(RichText::new(tf!("{} unknowns · {:.0} ms", "{} bilinmeyen · {:.0} ms", s.basis.len(), self.solve_ms)).small().color(theme::TEXT_DIM));
            }
            if self.sweep_job.pending() {
                ui.separator();
                ui.spinner();
                ui.label(RichText::new(tl("sweep…", "tarama…")).small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tl("not saved", "kaydedilmedi").into());
                ui.label(RichText::new(name).small().color(theme::TEXT_DIM));
                ui.separator();
                ui.label(RichText::new(self.project.model.ground().label()).small().color(theme::TEXT_DIM));
            });
        });
    }

    fn about(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new(tl("About AntAn", "AntAn hakkında")).open(&mut open).collapsible(false).resizable(false).default_width(460.0).show(ctx, |ui| {
            ui.label(RichText::new(tl("AntAn — antenna analysis and design", "AntAn — anten analiz ve hesaplama")).heading().color(theme::ACCENT));
            ui.label(tf!("Version {}", "Sürüm {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(6.0);
            ui.label(tl("Solver: thin-wire method of moments (Harrington, mixed potential, pulse basis / point matching). Supports wire junctions, ground (perfect and real), lumped loads, conductor loss (skin effect, tube wall thickness) and tapered elements.", "Çözücü: ince tel moment yöntemi (Harrington, karışık potansiyel, darbe tabanlı / nokta eşlemeli). Tel birleşimleri, zemin (kusursuz ve gerçek), toplu yükler, iletken kaybı (deri etkisi, boru et kalınlığı) ve kademeli elemanlar desteklenir."));
            ui.add_space(4.0);
            ui.label(tl("Validation: half-wave dipole (≈73 + j42 Ω, 2.15 dBi), energy conservation (gain = directivity), Kirchhoff's current law, split/single-wire equivalence, explicit image equivalence over ground.", "Doğrulama: yarım dalga dipol (≈73 + j42 Ω, 2.15 dBi), enerji korunumu (kazanç = yönlülük), Kirchhoff akım yasası, bölünmüş/tek parça eşdeğerliği."));
            ui.add_space(4.0);
            ui.label(RichText::new(tl("Limitations: over real ground, soil loss does not enter the feed impedance; differences of a few percent from NEC are normal.", "Sınırlar: gerçek zeminde toprak kaybı besleme empedansına girmez; NEC ile birkaç yüzdelik farklar normaldir.")).color(theme::TEXT_DIM));
            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new(tl("Thanks", "Teşekkürler")).strong().color(theme::ACCENT));
            ui.label(tl("For testing and feedback:", "Test edip geri bildirimde bulundukları için:"));
            for (call, name, what) in CONTRIBUTORS {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(*call).strong().monospace());
                    ui.label(*name);
                    ui.label(RichText::new(*what).small().color(theme::TEXT_DIM));
                });
            }
        });
        self.show_about = open;
    }

    // ------------------------------------------------------------ sekmeler

    fn kpi(cards: &mut Vec<Card>, title: &str, value: String, sub: String, color: Color32) {
        cards.push(Card { title: title.to_string(), value, sub, color });
    }

    /// Kartları satırlara yerleştirir: sığdığı kadar yan yana, eşit genişlikte.
    fn draw_cards(ui: &mut egui::Ui, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        let gap = 6.0;
        let avail = ui.available_width();
        let per_row = (((avail + gap) / (150.0 + gap)).floor() as usize).clamp(1, cards.len());
        let rows = cards.len().div_ceil(per_row);
        let per_row = cards.len().div_ceil(rows);
        let w = (avail - gap * (per_row as f32 - 1.0)) / per_row as f32;
        for row in cards.chunks(per_row) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for c in row {
                    egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(9, 5)).show(ui, |ui| {
                        ui.set_width(w - 18.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&c.title).small().color(theme::TEXT_DIM));
                            ui.label(RichText::new(&c.value).size(19.0).strong().color(c.color));
                            if !c.sub.is_empty() {
                                ui.add(egui::Label::new(RichText::new(&c.sub).small().color(theme::TEXT_DIM)).wrap());
                            }
                        });
                    });
                }
            });
            ui.add_space(gap - 4.0);
        }
    }

    fn summary_cards(&self) -> Vec<Card> {
        let mut cards = Vec::new();
        let (Some(sol), Some(s)) = (self.sol(), &self.summary) else { return cards };
        let z = self.fed(sol.impedance(), self.antenna.frequency);
        let swr = matching::swr(z, self.project.z0);
        let lambda = C0 / self.antenna.frequency;
        Self::kpi(&mut cards, tl("Gain", "Kazanç"), format!("{:.2} dBi", s.max_gain_dbi), tf!("{:.2} dBd · directivity {:.2} dBi", "{:.2} dBd · yönlülük {:.2} dBi", s.max_gain_dbi - 2.15, s.directivity_dbi), Color32::WHITE);
        Self::kpi(&mut cards, tl("Front / back", "Ön / arka"), format!("{:.1} dB", s.front_to_back_db), String::new(), theme::ACCENT);
        let ground = self.antenna.ground;
        if !ground.is_free() {
            let (lo, hi) = self.antenna.bounds();
            Self::kpi(
                &mut cards,
                tl("Takeoff angle", "Kalkış açısı"),
                format!("{:.0}°", 90.0 - s.max_theta.to_degrees()),
                tf!("{} · height {:.2}–{:.2} m", "{} · yükseklik {:.2}–{:.2} m", ground.label(), lo[2].max(0.0), hi[2]),
                theme::WARM,
            );
        }
        Self::kpi(&mut cards, if self.project.hairpin_l > 0.0 { tl("Feed impedance (with hairpin)", "Besleme empedansı (hairpin dahil)") } else { tl("Feed impedance", "Besleme empedansı") }, fmt_z(z), format!("|Γ| = {:.3} · RL {:.1} dB", matching::reflection(z, self.project.z0).norm(), matching::return_loss_db(z, self.project.z0)), Color32::WHITE);
        Self::kpi(&mut cards, &format!("SWR ({:.0} Ω)", self.project.z0), format!("{swr:.2}"), match self.band() {
            Some((lo, hi)) => tf!("2:1 band: {:.2}–{:.2} MHz ({:.0} kHz)", "2:1 bant: {:.2}–{:.2} MHz ({:.0} kHz)", lo / 1e6, hi / 1e6, (hi - lo) / 1e3),
            None if self.sweep_job.pending() => tl("computing bandwidth…", "bant hesaplanıyor…").into(),
            None => tl("no 2:1 band", "2:1 bant yok").into(),
        }, theme::swr_color(swr));
        Self::kpi(&mut cards, tl("Beamwidth (−3 dB)", "Huzme genişliği (−3 dB)"), format!("{:.0}° / {:.0}°", s.beamwidth_az_deg, s.beamwidth_el_deg), tl("horizontal / vertical", "yatay / düşey").into(), Color32::WHITE);
        let (lo, hi) = self.antenna.bounds();
        let loss_db = -10.0 * sol.efficiency().max(1e-9).log10();
        let in_loads = if sol.loads.is_empty() { String::new() } else { tf!(" · {:.1}% in loads", " · yüklerde %{:.1}", sol.loads.iter().map(|l| l.power).sum::<f64>() / sol.input_power * 100.0) };
        Self::kpi(&mut cards, tl("Efficiency", "Verim"), format!("{:.2} %", sol.efficiency() * 100.0), tf!("loss {:.3} dB{}", "kayıp {:.3} dB{}", loss_db, in_loads), Color32::WHITE);
        let dims = (0..3).map(|i| hi[i] - lo[i]).collect::<Vec<_>>();
        Self::kpi(
            &mut cards,
            tl("Size (X · Y · Z)", "Boyut (X · Y · Z)"),
            format!("{:.2} λ", dims.iter().cloned().fold(0.0, f64::max) / lambda),
            format!("{:.0} × {:.0} × {:.0} mm", dims[0] * 1e3, dims[1] * 1e3, dims[2] * 1e3),
            Color32::WHITE,
        );
        cards
    }

    fn pattern_tab(&mut self, ui: &mut egui::Ui) {
        view3d::toolbar(ui, &mut self.view);
        ui.add_space(4.0);
        let highlight = self.highlight();
        // Değer kartları 3B görünümün altında, yan yana; yükseklik bir önceki kareden.
        let cards = self.summary_cards();
        let avail = ui.available_size();
        let h3d = (avail.y - self.cards_h - 8.0).max(220.0);
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
        view3d::show(ui, Vec2::new(avail.x, h3d), self.pattern.as_ref(), max_db, &self.antenna, sol, &highlight, boom, &mut self.view);
        ui.add_space(6.0);
        let top = ui.cursor().top();
        Self::draw_cards(ui, &cards);
        self.cards_h = (ui.cursor().top() - top).max(40.0);
    }

    fn cuts_tab(&mut self, ui: &mut egui::Ui) {
        let (Some(sol), Some(s)) = (self.sol(), self.summary) else { return };
        let az = farfield::azimuth_cut(sol, s.max_theta, 360);
        let el = farfield::elevation_cut(sol, s.max_phi, 360);
        let mut range = self.cut_range_db;
        ui.horizontal_wrapped(|ui| {
            ui.label(tl("Scale range", "Ölçek aralığı"));
            ui.add(egui::Slider::new(&mut range, 10.0..=60.0).suffix(" dB"));
            ui.label(RichText::new(tf!("Horizontal cut θ = {:.0}° (elevation {:.0}°), vertical cut φ = {:.0}°", "Yatay kesit θ = {:.0}° (yükseliş {:.0}°), düşey kesit φ = {:.0}°", s.max_theta.to_degrees(), 90.0 - s.max_theta.to_degrees(), s.max_phi.to_degrees())).color(theme::TEXT_DIM));
        });
        let avail = ui.available_size();
        let h = ((avail.y - 190.0).max(260.0)).min(avail.x * 0.5 - 6.0);
        let w = (avail.x - 8.0) * 0.5;
        ui.horizontal_top(|ui| {
            viz::polar_plot(ui, Vec2::new(w, h), tl("Horizontal (azimuth) cut", "Yatay (azimut) kesit"), &az, Polar::Azimuth, range, &PolarInfo { max_gain_dbi: s.max_gain_dbi, beamwidth_deg: s.beamwidth_az_deg, fb_db: s.front_to_back_db });
            viz::polar_plot(ui, Vec2::new(w, h), tl("Vertical (elevation) cut", "Düşey (yükseliş) kesit"), &el, Polar::Elevation, range, &PolarInfo { max_gain_dbi: s.max_gain_dbi, beamwidth_deg: s.beamwidth_el_deg, fb_db: s.front_to_back_db });
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
        ui.horizontal_wrapped(|ui| {
            ui.label(tl("Sweep: ±", "Tarama: ±"));
            ui.add(egui::DragValue::new(&mut self.project.sweep_span_pct).range(0.2..=40.0).speed(0.1).suffix(" %"));
            ui.label(tl("points", "nokta"));
            ui.add(egui::DragValue::new(&mut self.project.sweep_points).range(5..=401));
            if self.sweep_job.pending() {
                ui.spinner();
                ui.label(tl("computing (all cores)…", "hesaplanıyor (tüm çekirdekler)…"));
            }
            if let Some((lo, hi)) = self.band() {
                ui.separator();
                ui.label(RichText::new(tf!("SWR < 2 band: {:.3} – {:.3} MHz  ({:.0} kHz, {:.2}%)", "SWR < 2 bandı: {:.3} – {:.3} MHz  ({:.0} kHz, %{:.2})", lo / 1e6, hi / 1e6, (hi - lo) / 1e3, (hi - lo) / self.antenna.frequency * 100.0)).color(theme::GOOD));
            }
        });
        let Some(sweep) = self.fed_sweep() else {
            ui.weak(tl("Waiting for sweep result…", "Tarama sonucu bekleniyor…"));
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
        let hairpin_l = self.project.hairpin_l;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tf!("Feed: {}", "Besleme: {}", fmt_z(z))).size(17.5).strong());
            let swr = matching::swr(z, z0);
            ui.label(RichText::new(format!("SWR {swr:.2} ({z0:.0} Ω)")).size(17.5).color(theme::swr_color(swr)));
            if hairpin_l > 0.0 {
                let zf = self.fed(z, f);
                let swr = matching::swr(zf, z0);
                ui.separator();
                ui.label(RichText::new(tf!("with hairpin: {}", "hairpin ile: {}", fmt_z(zf))).size(17.5).strong().color(theme::ACCENT));
                ui.label(RichText::new(format!("SWR {swr:.2}")).size(17.5).color(theme::swr_color(swr)));
            }
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
        let mut install: Option<f64> = None;
        ui.columns(3, |cols| {
            card(&mut cols[0], tl("L network (lumped components)", "L devresi (toplu elemanlar)"), &mut |ui| {
                let nets = matching::l_network(z, z0, f);
                if nets.is_empty() {
                    ui.weak(tl("No L network is needed or possible for this impedance.", "Bu empedans için L devresi gerekmez ya da çözüm yok."));
                }
                for (i, n) in nets.iter().enumerate() {
                    ui.label(RichText::new(tf!("Solution {}", "Çözüm {}", i + 1)).strong());
                    ui.label(RichText::new(if n.shunt_at_load { tl("Shunt element at the load, series element towards the line", "Yüke paralel eleman, hat tarafına seri eleman") } else { tl("Series element at the load, shunt element towards the line", "Yüke seri eleman, hat tarafına paralel eleman") }).color(theme::TEXT_DIM));
                    ui.label(tf!("  Series:  {}", "  Seri:    {}", reactive_str(n.series)));
                    ui.label(tf!("  Shunt:   {}", "  Paralel: {}", reactive_str(n.shunt)));
                    let check = matching::l_network_input(z, n, f);
                    ui.label(RichText::new(tf!("  Check:   {}", "  Kontrol: {}", fmt_z(check))).small().color(theme::TEXT_DIM));
                    ui.add_space(4.0);
                }
            });
            card(&mut cols[1], tl("Hairpin (beta) match", "Hairpin (beta) eşleme"), &mut |ui| {
                egui::Grid::new("hp").num_columns(2).show(ui, |ui| {
                    ui.label(tl("Rod diameter", "Çubuk çapı"));
                    let mut v = rod * 1e3;
                    if ui.add(egui::DragValue::new(&mut v).range(0.5..=30.0).speed(0.1).suffix(" mm")).changed() {
                        rod = v / 1e3;
                    }
                    ui.end_row();
                    ui.label(tl("Rod spacing (center)", "Çubuk aralığı (merkez)"));
                    let mut v = spacing * 1e3;
                    if ui.add(egui::DragValue::new(&mut v).range(2.0..=300.0).speed(0.5).suffix(" mm")).changed() {
                        spacing = v / 1e3;
                    }
                    ui.end_row();
                });
                match matching::hairpin(z, z0, f, rod, spacing) {
                    Some(h) => {
                        ui.label(tf!("Required element reactance: {:.1} Ω", "Gerekli eleman reaktansı: {:.1} Ω", h.required_reactance));
                        let change = if h.reactance_change.abs() < 0.5 { tl("OK ✔", "uygun ✔").to_string() } else if h.reactance_change < 0.0 { tf!("{:.1} Ω more capacitive → shorten the element", "{:.1} Ω daha kapasitif → elemanı kısaltın", -h.reactance_change) } else { tf!("{:.1} Ω more inductive → lengthen the element", "{:.1} Ω daha endüktif → elemanı uzatın", h.reactance_change) };
                        ui.label(RichText::new(tf!("Current state: {change}", "Şu anki durum: {change}")).color(if h.reactance_change.abs() < 0.5 { theme::GOOD } else { theme::WARM }));
                        ui.label(tf!("Hairpin inductance: {:.1} nH ({:.1} Ω)", "Hairpin endüktansı: {:.1} nH ({:.1} Ω)", h.inductance * 1e9, h.shunt_reactance));
                        ui.label(RichText::new(tf!("Hairpin length: {:.1} mm", "Hairpin boyu: {:.1} mm", h.length * 1e3)).strong());
                        ui.label(RichText::new(tf!("(two-wire line Z₀ = {:.0} Ω, shorted end)", "(çift hat Z₀ = {:.0} Ω, kısa devre ucu)", h.line_z0)).small().color(theme::TEXT_DIM));
                        ui.add_space(4.0);
                        if ui
                            .button(tl("➕ Install this hairpin on the model", "➕ Bu hairpin'i modele tak"))
                            .on_hover_text(tl("SWR, impedance, Smith chart and sweep are then shown on the line side, including the hairpin (its reactance changes with frequency).", "SWR, empedans, Smith abağı ve tarama bundan sonra hat tarafından, hairpin dahil gösterilir (reaktansı frekansla değişir)."))
                            .clicked()
                        {
                            install = Some(h.inductance);
                        }
                    }
                    None => {
                        ui.label(RichText::new(tl("For a hairpin the feed resistance must be below Z₀.", "Hairpin için besleme direnci Z₀'dan küçük olmalı.")).color(theme::WARM));
                    }
                }
            });
            card(&mut cols[2], tl("Gamma match", "Gamma eşleme"), &mut |ui| {
                ui.label(RichText::new(tf!("Element diameter {:.1} mm; rod/spacing shared with the hairpin.", "Eleman çapı {:.1} mm; çubuk/aralık hairpin ile ortak.", driven_d * 1e3)).small().color(theme::TEXT_DIM));
                let p = matching::GammaParams { element_diameter: driven_d, rod_diameter: rod, spacing };
                match matching::gamma(z, z0, f, p) {
                    Some(g) => {
                        ui.label(RichText::new(tf!("Gamma rod length: {:.1} mm", "Gamma çubuğu boyu: {:.1} mm", g.length * 1e3)).strong());
                        ui.label(RichText::new(tf!("Series capacitor: {:.1} pF", "Seri kondansatör: {:.1} pF", g.capacitance * 1e12)).strong());
                        ui.label(tf!("Impedance step-up ratio: {:.2}×", "Empedans yükseltme oranı: {:.2}×", g.step_up));
                        ui.label(tf!("Gamma section line Z₀: {:.0} Ω", "Gamma bölümü hat Z₀: {:.0} Ω", g.line_z0));
                        ui.label(RichText::new(tl("Approximate model (Healey). Works best with a resonant element.", "Yaklaşık model (Healey). Rezonanstaki elemanla en iyi sonuç verir.")).small().color(theme::TEXT_DIM));
                    }
                    None => {
                        ui.label(RichText::new(tl("No solution for this geometry (rod length exceeds 0.2 λ).", "Bu geometri için çözüm bulunamadı (çubuk boyu 0.2 λ'yı aşıyor).")).color(theme::WARM));
                    }
                }
            });
        });
        self.project.match_rod = rod;
        self.project.match_spacing = spacing;
        if let Some(l) = install {
            self.project.hairpin_l = l;
        }
        if self.project.hairpin_l > 0.0 {
            ui.add_space(6.0);
            egui::Frame::new().fill(Color32::from_rgb(30, 52, 44)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tf!("Hairpin installed: {:.1} nH (X = {:.1} Ω at {:.3} MHz)", "Takılı hairpin: {0:.1} nH ({2:.3} MHz'de X = {1:.1} Ω)", self.project.hairpin_l * 1e9, 2.0 * std::f64::consts::PI * f * self.project.hairpin_l, f / 1e6)).color(theme::GOOD));
                    let mut nh = self.project.hairpin_l * 1e9;
                    if ui.add(egui::DragValue::new(&mut nh).range(0.1..=1e6).speed(0.5).suffix(" nH")).changed() {
                        self.project.hairpin_l = nh * 1e-9;
                    }
                    if ui.button(tl("Remove", "Çıkar")).clicked() {
                        self.project.hairpin_l = 0.0;
                    }
                });
                ui.label(RichText::new(tl("Feed impedance, SWR and the sweep now include the hairpin. The hairpin values above are always computed for the bare antenna.", "Besleme empedansı, SWR ve tarama artık hairpin dahil. Yukarıdaki hairpin hesabı her zaman çıplak anten içindir.")).small().color(theme::TEXT_DIM));
            });
        }

        if let Model::Yagi(d) = &self.project.model {
            ui.add_space(8.0);
            egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(tl("Auto-tune the driven element", "Sürülen elemanı otomatik ayarla")).strong().color(theme::ACCENT));
                ui.label(RichText::new(tl("The driven element length is iterated with the full solver until the feed reactance hits the target.", "Sürülen elemanın boyu, gerçek çözücüyle yinelenerek hedef besleme reaktansına getirilir.")).small().color(theme::TEXT_DIM));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tl("Make resonant (X = 0, gamma/direct feed)", "Rezonansa getir (X = 0, gamma/doğrudan besleme)")).clicked() {
                        match d.tune_driven(|_| Some(0.0)) {
                            Some((nd, nz)) => {
                                let mut p = self.project.clone();
                                p.model = Model::Yagi(nd.clone());
                                apply = Some(p);
                                self.tune_msg = Some(tf!("Driven element {:.1} → {:.1} mm, Z = {}", "Sürülen eleman {:.1} → {:.1} mm, Z = {}", d.elements[d.driven].length * 1e3, nd.elements[d.driven].length * 1e3, fmt_z(nz)));
                            }
                            None => self.tune_msg = Some(tl("Tuning did not converge.", "Ayar yakınsamadı.").into()),
                        }
                    }
                    if ui.button(tl("Make capacitive for hairpin (X = −√(R(Z₀−R)))", "Hairpin için kapasitif yap (X = −√(R(Z₀−R)))")).clicked() {
                        match d.tune_driven(|zz| (zz.re < z0).then(|| -(zz.re * (z0 - zz.re)).sqrt())) {
                            Some((nd, nz)) => {
                                let mut p = self.project.clone();
                                p.model = Model::Yagi(nd.clone());
                                apply = Some(p);
                                let hl = matching::hairpin(nz, z0, f, rod, spacing).map(|h| tf!(", hairpin {:.1} mm", ", hairpin {:.1} mm", h.length * 1e3)).unwrap_or_default();
                                self.tune_msg = Some(tf!("Driven element {:.1} → {:.1} mm, Z = {}{hl}", "Sürülen eleman {:.1} → {:.1} mm, Z = {}{hl}", d.elements[d.driven].length * 1e3, nd.elements[d.driven].length * 1e3, fmt_z(nz)));
                            }
                            None => self.tune_msg = Some(tl("Tuning did not converge (R may be ≥ Z₀).", "Ayar yakınsamadı (R ≥ Z₀ olabilir).").into()),
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
            self.status = tl("Optimization result applied (Ctrl+Z to undo)", "İyileştirme sonucu uygulandı (Ctrl+Z ile geri alınabilir)").into();
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
    d.name = tl("7-el. tapered Yagi", "7 el. kademeli Yagi").into();
    Project::with_model(Model::Yagi(d))
}

impl eframe::App for AntAnApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.window_rect = Some(ui.max_rect());
        // Ölçek menüden ya da Ctrl +/−/0 ile değiştiyse kaydet.
        let z = ctx.zoom_factor();
        if (z - self.text_scale).abs() > 1e-3 {
            self.text_scale = z;
            if std::env::var_os("ANTAN_SCALE").is_none() {
                Settings::current(z).save();
            }
        }
        self.shortcuts(&ctx);
        self.recompute_if_needed(&ctx);
        if let Some(r) = self.sweep_job.poll() {
            match r {
                Ok(s) => self.sweep = Some(s),
                Err(e) => {
                    self.sweep = None;
                    self.status = tf!("Sweep: {e}", "Tarama: {e}");
                }
            }
        }
        if let Some(res) = self.shotter.poll(&ctx) {
            self.status = match res {
                Ok(p) => tf!("Image saved: {}", "Görüntü kaydedildi: {}", p.display()),
                Err(e) => tf!("Could not save image: {e}", "Görüntü kaydedilemedi: {e}"),
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
            if self.autoshot_frame == 3
                && std::env::var_os("ANTAN_AUTOSHOT_HAIRPIN").is_some()
                && let Some(z) = self.sol().map(|s| s.impedance())
                && let Some(h) = matching::hairpin(z, self.project.z0, self.antenna.frequency, self.project.match_rod, self.project.match_spacing)
            {
                self.project.hairpin_l = h.inductance;
            }
            if self.autoshot_frame == 2 {
                match std::env::var("ANTAN_AUTOSHOT_WINDOW").ok().as_deref() {
                    Some("suggest") => {
                        self.suggest.open = true;
                        self.suggest.start_at(&ctx, self.project.model.frequency());
                    }
                    Some("about") => self.show_about = true,
                    Some("height") => {
                        self.heightscan.open = true;
                        self.heightscan.start(&ctx, self.antenna.clone());
                    }
                    _ => {}
                }
            }
            if self.autoshot_frame > 8 && !self.sweep_job.pending() && !self.optim.busy() && !self.suggest.busy() && !self.heightscan.busy() && self.autoshot_frame < 1000 {
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
        // Yan paneller pencereye orantılı sınırlanır; büyük yazı boyutunda ya da küçük ekranda
        // ortadaki grafik alanı sıkışmasın. Geniş içerik yatay kaydırılır, paneli büyütmez.
        let total = ui.available_width();
        let left_max = (total * 0.18).clamp(150.0, 360.0);
        let right_max = (total * 0.36).clamp(300.0, 620.0);
        egui::Panel::left("tree").resizable(true).default_size(215.0f32.min(left_max)).size_range(150.0..=left_max).show(ui, |ui| editor::model_tree(self, ui));
        egui::Panel::right("props").resizable(true).default_size(400.0f32.min(right_max)).size_range(300.0..=right_max).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(2.0);
                editor::properties(self, ui);
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (t, l) in [(Tab::Pattern3d, tl("🌐 3D pattern", "🌐 3B diyagram")), (Tab::Cuts, tl("◔ Cuts and currents", "◔ Kesitler ve akımlar")), (Tab::Sweep, tl("📈 SWR / Smith", "📈 SWR / Smith")), (Tab::Matching, tl("🔌 Matching", "🔌 Eşleme")), (Tab::Optimize, tl("⚙ Optimization", "⚙ İyileştirme"))] {
                    let sel = self.tab == t;
                    if ui.add(egui::Button::selectable(sel, RichText::new(l).size(15.5))).clicked() {
                        self.tab = t;
                    }
                }
            });
            ui.separator();
            if self.sol().is_none() && self.tab != Tab::Optimize {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(tl("No solution", "Çözüm yok")).heading().color(theme::BAD));
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
        let lowest = self.antenna.wires.iter().flat_map(|w| [w.start[2], w.end[2]]).fold(f64::INFINITY, f64::min);
        if let Some(h) = self.heightscan.show(&ctx, &self.antenna, self.project.z0, lowest) {
            self.set_height(h);
        }
        if let Some(p) = self.suggest.show(&ctx, self.project.model.frequency()) {
            self.status = tf!("Opened suggestion: {}", "Öneri açıldı: {}", p.model.name());
            self.replace_project(p);
        }
        let n = match &self.project.model {
            Model::Yagi(d) => d.elements.len(),
            Model::Wires(a) => a.wires.len(),
        };
        self.selected = self.selected.min(n.saturating_sub(1));
        self.track_undo(&ctx);
    }
}

/// Katkıda bulunanlar: (çağrı işareti, ad, katkı). Yeni katkıda bulunanlar buraya ve
/// README.md / README.tr.md'deki "Thanks / Teşekkürler" bölümüne eklenir.
const CONTRIBUTORS: &[(&str, &str, &str)] = &[
    ("TA7MVL", "Mevlüt", "v0.2.0"),
    ("TA6TTT", "Asım", "v0.2.0"),
    ("TA1OZV", "Tayfun", "v0.2.0"),
];

/// Özet değer kartı.
struct Card {
    title: String,
    value: String,
    sub: String,
    color: Color32,
}

/// Kullanıcı ayarları (dil, yazı boyutu): Windows'ta %APPDATA%\AntAn\settings.json,
/// diğer sistemlerde ~/.config/antan/settings.json.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Settings {
    language: Language,
    /// Arayüz ölçeği (egui zoom), 1.0 = normal.
    text_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { language: Language::default(), text_scale: 1.0 }
    }
}

impl Settings {
    fn current(text_scale: f32) -> Self {
        Self { language: language(), text_scale }
    }
    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("AntAn")).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("antan")))?;
        Some(base.join("settings.json"))
    }
    fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
