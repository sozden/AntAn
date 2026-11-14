//! Anten önerileri: girilen frekans için şablonları kurar, arka planda çözer,
//! kazanç / ön-arka / empedans / boyut ile karşılaştırmalı listeler.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};

use antan_core::C0;
use antan_core::farfield::Pattern;
use antan_core::i18n::t as tl;
use antan_core::matching;
use antan_core::solver::solve;
use antan_core::templates::Kind;
use antan_core::{Complex64, bands, tf};

use crate::project::{Model, Project};
use crate::theme;
use crate::viz::fmt_z;

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Beam,
    Wire,
    Vertical,
}

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Beam => tl("Directional (beams)", "Yönlü (beam)"),
            Group::Wire => tl("Simple / wire antennas", "Basit / tel antenler"),
            Group::Vertical => tl("Verticals (omnidirectional)", "Dikeyler (her yöne)"),
        }
    }
}

struct Candidate {
    group: Group,
    name: String,
    note: &'static str,
    project: Project,
}

#[derive(Clone, Copy)]
struct Metrics {
    gain: f64,
    fb: f64,
    z: Complex64,
    size: f64,
}

pub struct Suggest {
    pub open: bool,
    freq_mhz: f64,
    list: Vec<Candidate>,
    metrics: Arc<Mutex<Vec<Option<Option<Metrics>>>>>,
}

impl Default for Suggest {
    fn default() -> Self {
        Self { open: false, freq_mhz: 14.175, list: Vec::new(), metrics: Arc::default() }
    }
}

fn candidates(f: f64) -> Vec<Candidate> {
    let mut v = Vec::new();
    let mut add = |group, name: String, note: &'static str, project: Project| v.push(Candidate { group, name, note, project });
    for n in [3usize, 5, 7] {
        add(Group::Beam, tf!("{n}-element Yagi", "{n} elemanlı Yagi"), tl("Classic rotatable beam; needs a matching device (hairpin/gamma) for 50 Ω.", "Klasik döner beam; 50 Ω için eşleme (hairpin/gamma) gerekir."), Project::yagi(f, n));
    }
    add(Group::Beam, tf!("{}-element LFA Yagi", "{} elemanlı LFA Yagi", 5), tl("Loop-fed: direct 50 Ω feed, clean pattern.", "Halka beslemeli: doğrudan 50 Ω, temiz diyagram."), Project::lfa(f, 5));
    add(Group::Beam, Kind::Moxon.label().into(), tl("Compact 2-element beam, ~50 Ω, very good F/B.", "Kompakt 2 elemanlı beam, ~50 Ω, çok iyi Ö/A."), Project::template(Kind::Moxon, f));
    add(Group::Beam, Kind::Quad2.label().into(), tl("2-element loop beam; ~120 Ω, needs 75 Ω line or match.", "2 elemanlı halka beam; ~120 Ω, 75 Ω hat ya da eşleme gerekir."), Project::template(Kind::Quad2, f));
    add(Group::Wire, Kind::Dipole.label().into(), tl("Simplest reference antenna; needs a λ/2 span.", "En basit referans anten; λ/2 açıklık gerekir."), Project::template(Kind::Dipole, f));
    add(Group::Wire, Kind::InvertedV.label().into(), tl("Single support at the center; closer to 50 Ω.", "Ortada tek direk; 50 Ω'a daha yakın."), Project::template(Kind::InvertedV, f));
    add(Group::Wire, Kind::LoadedDipole.label().into(), tl("Half the span of a dipole; narrower band, some coil loss.", "Dipolün yarı açıklığı; dar bant, biraz bobin kaybı."), Project::template(Kind::LoadedDipole, f));
    add(Group::Wire, Kind::FoldedDipole.label().into(), tl("~300 Ω, wider band; feed with ladder line or a 4:1 balun.", "~300 Ω, geniş bant; merdiven hat ya da 4:1 balun."), Project::template(Kind::FoldedDipole, f));
    add(Group::Wire, Kind::DeltaLoop.label().into(), tl("Full-wave loop; ~120 Ω, quieter than a dipole.", "Tam dalga halka; ~120 Ω, dipolden sessiz."), Project::template(Kind::DeltaLoop, f));
    if [(3.5e6, 3.8e6), (7.0e6, 7.2e6)].iter().any(|&(lo, hi)| f >= lo && f <= hi) {
        let mut p = Project::template(Kind::TrapDipole, f);
        p.model.set_frequency(f);
        add(Group::Wire, Kind::TrapDipole.label().into(), tl("One wire for both 40 m and 80 m (fixed design).", "40 m ve 80 m için tek tel (sabit tasarım)."), p);
    }
    add(Group::Vertical, Kind::GroundPlane.label().into(), tl("Elevated λ/4 with drooping radials, ~50 Ω.", "Yükseltilmiş λ/4, eğik radyaller, ~50 Ω."), Project::template(Kind::GroundPlane, f));
    add(Group::Vertical, Kind::GroundVertical.label().into(), tl("Ground mounted; needs a radial field in practice.", "Zemine monte; pratikte radyal sistem gerekir."), Project::template(Kind::GroundVertical, f));
    add(Group::Vertical, Kind::LoadedVertical.label().into(), tl("λ/8 with a coil, e.g. mobile; low resistance, narrow band.", "Bobinli λ/8, ör. araç; düşük direnç, dar bant."), Project::template(Kind::LoadedVertical, f));
    v
}

impl Suggest {
    /// Doğrulama için: verilen frekansla hesaplamayı başlatır.
    pub fn start_at(&mut self, ctx: &egui::Context, f: f64) {
        self.freq_mhz = f / 1e6;
        self.compute(ctx);
    }
    pub fn busy(&self) -> bool {
        self.open && self.metrics.lock().unwrap().iter().any(|m| m.is_none())
    }
    fn compute(&mut self, ctx: &egui::Context) {
        let f = self.freq_mhz * 1e6;
        self.list = candidates(f);
        *self.metrics.lock().unwrap() = vec![None; self.list.len()];
        let jobs: Vec<(usize, Model)> = self.list.iter().map(|c| c.project.model.clone()).enumerate().collect();
        let (metrics, ctx) = (self.metrics.clone(), ctx.clone());
        std::thread::spawn(move || {
            std::thread::scope(|s| {
                for (i, model) in jobs {
                    let (metrics, ctx) = (metrics.clone(), ctx.clone());
                    s.spawn(move || {
                        let a = match &model {
                            Model::Yagi(d) => d.build(),
                            Model::Wires(a) => a.clone(),
                        };
                        let m = solve(&a, a.frequency).ok().map(|sol| {
                            let p = Pattern::compute(&sol, 5.0).summary(&sol);
                            Metrics { gain: p.max_gain_dbi, fb: p.front_to_back_db, z: sol.impedance(), size: model.largest_dimension() }
                        });
                        metrics.lock().unwrap()[i] = Some(m);
                        ctx.request_repaint();
                    });
                }
            });
        });
    }

    /// Kullanıcı bir öneriyi açarsa projeyi döndürür.
    pub fn show(&mut self, ctx: &egui::Context, current_f: f64) -> Option<Project> {
        if !self.open {
            return None;
        }
        let mut out = None;
        let mut open = self.open;
        egui::Window::new(tl("Antenna suggestions", "Anten önerileri")).open(&mut open).default_size([1000.0, 640.0]).resizable(true).show(ctx, |ui| {
            ui.label(RichText::new(tl("Enter a frequency: suitable antenna types are built for it and solved, so you can compare gain, front-to-back, feed impedance and size. Beams and wire antennas are in free space; verticals are shown with their ground.", "Bir frekans girin: uygun anten türleri bu frekansa göre kurulur ve çözülür; kazanç, ön/arka, besleme empedansı ve boyutu karşılaştırabilirsiniz. Beam ve tel antenler serbest uzayda, dikeyler zeminleriyle hesaplanır.")).small().color(theme::TEXT_DIM));
            ui.horizontal_wrapped(|ui| {
                ui.label(tl("Frequency", "Frekans"));
                let r = ui.add(egui::DragValue::new(&mut self.freq_mhz).range(bands::MIN_FREQ / 1e6..=bands::MAX_FREQ / 1e6).speed(0.01).max_decimals(4).suffix(" MHz"));
                egui::ComboBox::from_id_salt("sg_band").selected_text(bands::band_of(self.freq_mhz * 1e6).map(|b| b.name).unwrap_or("—")).width(80.0).show_ui(ui, |ui| {
                    for b in &bands::BANDS {
                        if ui.selectable_label(false, b.name).clicked() {
                            self.freq_mhz = b.center / 1e6;
                        }
                    }
                });
                if ui.small_button(tl("current", "şimdiki")).clicked() {
                    self.freq_mhz = current_f / 1e6;
                }
                if ui.add(egui::Button::new(RichText::new(tl("💡 Suggest", "💡 Öner")).strong()).fill(theme::ACCENT_DIM)).clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.compute(ctx);
                }
                ui.label(RichText::new(tf!("λ = {:.2} m", "λ = {:.2} m", C0 / (self.freq_mhz * 1e6))).color(theme::TEXT_DIM));
            });
            if self.list.is_empty() {
                return;
            }
            let metrics = self.metrics.lock().unwrap().clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for group in [Group::Beam, Group::Wire, Group::Vertical] {
                    ui.add_space(6.0);
                    ui.label(RichText::new(group.label()).strong().color(theme::ACCENT));
                    egui::Grid::new(("sg", group as u8)).striped(true).num_columns(8).spacing([12.0, 4.0]).show(ui, |ui| {
                        for t in [tl("Antenna", "Anten"), tl("Gain", "Kazanç"), tl("F/B", "Ö/A"), tl("Feed Z", "Besleme Z"), "SWR", tl("Size", "Boyut"), tl("Notes", "Notlar"), ""] {
                            ui.label(RichText::new(t).small().color(theme::TEXT_DIM));
                        }
                        ui.end_row();
                        for (i, c) in self.list.iter().enumerate().filter(|(_, c)| c.group == group) {
                            ui.label(RichText::new(&c.name).strong());
                            match metrics.get(i).copied().flatten() {
                                None => {
                                    ui.spinner();
                                    for _ in 0..4 {
                                        ui.label("");
                                    }
                                }
                                Some(None) => {
                                    ui.label(RichText::new(tl("could not be solved", "çözülemedi")).color(theme::BAD));
                                    for _ in 0..4 {
                                        ui.label("");
                                    }
                                }
                                Some(Some(m)) => {
                                    ui.label(format!("{:.1} dBi", m.gain));
                                    ui.label(if group == Group::Beam { format!("{:.0} dB", m.fb) } else { "—".into() });
                                    ui.label(fmt_z(m.z));
                                    let swr = matching::swr(m.z, c.project.z0);
                                    ui.label(RichText::new(format!("{swr:.1} ({:.0} Ω)", c.project.z0)).color(theme::swr_color(swr)));
                                    let col = if m.size > 25.0 { theme::BAD } else if m.size > 12.0 { theme::WARM } else { theme::TEXT_DIM };
                                    ui.label(RichText::new(format!("{:.2} m", m.size)).color(col)).on_hover_text(tl("Largest dimension. Orange > 12 m, red > 25 m (hard to rotate / support).", "En büyük boyut. Turuncu > 12 m, kırmızı > 25 m (döndürmesi / taşıması zor)."));
                                }
                            }
                            ui.allocate_ui(egui::Vec2::new(300.0, 0.0), |ui| {
                                ui.set_width(300.0);
                                ui.add(egui::Label::new(RichText::new(c.note).small().color(theme::TEXT_DIM)).wrap());
                            });
                            if ui.button(tl("Open", "Aç")).clicked() {
                                out = Some(c.project.clone());
                            }
                            ui.end_row();
                        }
                    });
                }
            });
        });
        self.open = open;
        if out.is_some() {
            self.open = false;
        }
        out
    }
}
