//! Sol panel: model ağacı. Sağ panel: özellikler (Yagi tasarımı ya da tel modeli).

use eframe::egui::{self, Color32, RichText};
use antan_core::i18n::t as tl;
use antan_core::tf;


use antan_core::mechanics::{self, cantilever_tip_sag, tube_props, wind_force};
use antan_core::templates::{Tube, wire};
use antan_core::geometry::{Load, LoadKind};
use antan_core::yagi::{BoomMount, ElementLoad, TaperSection, YagiDesign, YagiElement};
use antan_core::{Antenna, C0, Material};

use crate::app::AntAnApp;
use crate::project::Model;
use crate::theme;

const MM: f64 = 1000.0;

fn section(ui: &mut egui::Ui, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    // Doğrulama çekimlerinde tüm bölümler açık gelsin.
    let open = open || std::env::var_os("ANTAN_AUTOSHOT_OPEN_ALL").is_some();
    egui::CollapsingHeader::new(RichText::new(title).strong().color(Color32::WHITE)).default_open(open).show(ui, add);
}

fn mm_value(ui: &mut egui::Ui, v: &mut f64, speed: f64, range: std::ops::RangeInclusive<f64>) -> bool {
    let mut x = *v * MM;
    let r = ui.add(egui::DragValue::new(&mut x).speed(speed).range(*range.start() * MM..=*range.end() * MM).max_decimals(2).suffix(" mm"));
    if r.changed() {
        *v = x / MM;
        true
    } else {
        false
    }
}

fn material_combo(ui: &mut egui::Ui, id: &str, m: &mut Material) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(m.display_name().to_string()).width(180.0).show_ui(ui, |ui| {
        for p in Material::presets() {
            if ui.selectable_label(p.name == m.name, p.display_name()).clicked() {
                *m = p;
                changed = true;
            }
        }
    });
    changed
}

/// Boru: çap + (dolu | et kalınlığı). Değiştiyse true.
fn tube_editor(ui: &mut egui::Ui, d: &mut f64, wall: &mut Option<f64>) -> bool {
    let mut changed = mm_value(ui, d, 0.1, 0.0005..=0.3);
    let mut hollow = wall.is_some();
    if ui.checkbox(&mut hollow, tl("tube", "boru")).on_hover_text(tl("Unchecked means a solid rod", "İşaretli değilse içi dolu çubuk")).changed() {
        *wall = if hollow { Some((0.1 * *d).clamp(0.0005, 0.5 * *d)) } else { None };
        changed = true;
    }
    if let Some(t) = wall {
        changed |= mm_value(ui, t, 0.02, 0.0001..=0.15);
        *t = t.min(0.5 * *d);
    } else {
        ui.weak(tl("solid", "dolu"));
    }
    changed
}

// ------------------------------------------------------------------ sol panel

pub fn model_tree(app: &mut AntAnApp, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.label(RichText::new(app.project.model.kind_label()).small().color(theme::ACCENT));
    ui.label(RichText::new(app.project.model.name().to_string()).heading().color(Color32::WHITE));
    ui.separator();
    let lambda = C0 / app.project.model.frequency();
    let mut action: Option<TreeAction> = None;
    egui::ScrollArea::both().auto_shrink([false, true]).show(ui, |ui| match &app.project.model {
        Model::Yagi(d) => {
            for (i, e) in d.elements.iter().enumerate() {
                let icon = if i == d.driven { "📡".to_string() } else { format!("{}.", i + 1) };
                let r = ui.selectable_label(app.selected == i, format!("{icon} {}", e.name));
                if r.clicked() {
                    action = Some(TreeAction::Select(i));
                }
                ui.label(RichText::new(format!("    x {:.0} mm · L {:.0} mm · {:.3} λ", e.x * MM, e.length * MM, e.length / lambda)).small().color(theme::TEXT_DIM));
            }
        }
        Model::Wires(a) => {
            let feeds: Vec<usize> = a.sources.iter().map(|s| s.wire).collect();
            for (i, w) in a.wires.iter().enumerate() {
                let icon = if feeds.contains(&i) { "📡".to_string() } else { format!("{}.", i + 1) };
                if ui.selectable_label(app.selected == i, format!("{icon} {}", w.name)).clicked() {
                    action = Some(TreeAction::Select(i));
                }
                ui.label(RichText::new(format!("    {:.0} mm · Ø{:.1} mm", w.length() * MM, w.diameter * MM)).small().color(theme::TEXT_DIM));
            }
        }
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| match &app.project.model {
        Model::Yagi(_) => {
            if ui.button(tl("+ Director", "+ Yönlendirici")).clicked() {
                action = Some(TreeAction::Add);
            }
            if ui.button(tl("Delete", "Sil")).clicked() {
                action = Some(TreeAction::Remove);
            }
        }
        Model::Wires(_) => {
            if ui.button(tl("+ Wire", "+ Tel")).clicked() {
                action = Some(TreeAction::Add);
            }
            if ui.button(tl("Duplicate", "Kopyala")).clicked() {
                action = Some(TreeAction::Duplicate);
            }
            if ui.button(tl("Delete", "Sil")).clicked() {
                action = Some(TreeAction::Remove);
            }
        }
    });
    if let Some(a) = action {
        apply_tree_action(app, a);
    }
}

enum TreeAction {
    Select(usize),
    Add,
    Duplicate,
    Remove,
}

fn apply_tree_action(app: &mut AntAnApp, a: TreeAction) {
    let lambda = C0 / app.project.model.frequency();
    match (&mut app.project.model, a) {
        (_, TreeAction::Select(i)) => app.selected = i,
        (Model::Yagi(d), TreeAction::Add) => {
            let last = d.elements.iter().max_by(|a, b| a.x.partial_cmp(&b.x).unwrap()).cloned();
            let (x, len) = last.map(|e| (e.x + 0.22 * lambda, (e.length * 0.99).max(0.40 * lambda))).unwrap_or((0.0, 0.45 * lambda));
            let n = d.elements.iter().filter(|e| e.name.starts_with("Director") || e.name.starts_with("Yönlendirici")).count() + 1;
            d.elements.push(YagiElement { name: tf!("Director {n}", "Yönlendirici {n}"), x, length: len });
            app.selected = d.elements.len() - 1;
        }
        (Model::Yagi(d), TreeAction::Remove) => {
            if d.elements.len() > 2 && app.selected < d.elements.len() && app.selected != d.driven {
                d.elements.remove(app.selected);
                if d.driven > app.selected {
                    d.driven -= 1;
                }
                app.selected = app.selected.min(d.elements.len() - 1);
            } else {
                app.status = tl("The driven element or the last two elements cannot be deleted", "Sürülen eleman ya da son iki eleman silinemez").into();
            }
        }
        (Model::Wires(a), TreeAction::Add) => {
            let (_, hi) = a.bounds();
            let x = if hi[0].is_finite() { hi[0] + 0.2 * lambda } else { 0.0 };
            let tube = a.wires.get(app.selected).map(|w| Tube { diameter: w.diameter, wall: w.wall, material: w.material.clone() }).unwrap_or_else(crate::project::default_tube);
            a.wires.push(wire(&tf!("Wire {}", "Tel {}", a.wires.len() + 1), [x, -0.24 * lambda, 0.0], [x, 0.24 * lambda, 0.0], &tube));
            app.selected = a.wires.len() - 1;
        }
        (Model::Wires(a), TreeAction::Duplicate) => {
            if let Some(w) = a.wires.get(app.selected).cloned() {
                let mut w2 = w;
                w2.name = tf!("{} (copy)", "{} (kopya)", w2.name);
                w2.start[0] += 0.1 * lambda;
                w2.end[0] += 0.1 * lambda;
                a.wires.push(w2);
                app.selected = a.wires.len() - 1;
            }
        }
        (Model::Wires(a), TreeAction::Remove) => {
            let i = app.selected;
            if a.wires.len() > 1 && i < a.wires.len() {
                a.wires.remove(i);
                a.sources.retain(|s| s.wire != i);
                for s in &mut a.sources {
                    if s.wire > i {
                        s.wire -= 1;
                    }
                }
                if a.sources.is_empty() {
                    a.sources.push(antan_core::templates::source(0, 0.5));
                }
                app.selected = i.min(a.wires.len() - 1);
            }
        }
        (Model::Yagi(_), TreeAction::Duplicate) => {}
    }
}

// ------------------------------------------------------------------ sağ panel

pub fn properties(app: &mut AntAnApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    section(ui, tl("Antenna", "Anten"), true, |ui| {
        egui::Grid::new("ant").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(tl("Name", "Ad"));
            ui.add(egui::TextEdit::singleline(app.project.model.name_mut()).desired_width(170.0));
            ui.end_row();
            ui.label(tl("Frequency", "Frekans"));
            use antan_core::bands;
            let mut mhz = app.project.model.frequency() / 1e6;
            ui.horizontal(|ui| {
                if ui
                    .add(egui::DragValue::new(&mut mhz).range(bands::MIN_FREQ / 1e6..=bands::MAX_FREQ / 1e6).speed(0.01).max_decimals(4).suffix(" MHz"))
                    .on_hover_text(tl("0.1 … 3000 MHz. Changing the frequency does not resize the antenna; use the Band menu with “scale” or the button on the right for that.", "0.1 … 3000 MHz. Frekansı değiştirmek anteni yeniden boyutlandırmaz; bunun için Bant menüsünü “ölçekle” ile ya da sağdaki düğmeyi kullanın."))
                    .changed()
                {
                    app.project.model.set_frequency(mhz * 1e6);
                }
                match bands::band_of(mhz * 1e6) {
                    Some(b) => ui.label(RichText::new(b.name).color(theme::ACCENT)),
                    None => ui.label(RichText::new(tl("outside ham bands", "amatör bant dışı")).small().color(theme::TEXT_DIM)),
                };
            });
            ui.end_row();
            // Frekans, geometrinin tasarlandığı frekanstan çok uzaksa ölçekleme önerisi.
            let lambda = C0 / app.project.model.frequency();
            let biggest = app.project.model.largest_dimension();
            let off = app.project.design_frequency().map(|g| (g / app.project.model.frequency()).ln().abs() > 0.25_f64.ln_1p()).unwrap_or(false);
            // Manyetik loop bilerek küçüktür ve frekans değişince kondansatörle ayarlanır.
            if app.project.magloop.is_none() && (biggest > 4.0 * lambda || biggest < 0.05 * lambda || off) {
                ui.label("");
                ui.vertical(|ui| {
                    let msg = match app.project.design_frequency() {
                        Some(g) => tf!("Geometry is sized for {:.3} MHz (largest dimension {:.2} λ here).", "Geometri {:.3} MHz için boyutlu (burada en büyük boyut {:.2} λ).", g / 1e6, biggest / lambda),
                        None => tf!("The antenna is {:.2} λ at this frequency.", "Anten bu frekansta {:.2} λ.", biggest / lambda),
                    };
                    ui.label(RichText::new(msg).small().color(theme::WARM));
                    if app.project.design_frequency().is_some() && ui.small_button(tl("Scale the antenna to this frequency", "Anteni bu frekansa ölçekle")).clicked() {
                        app.project.scale_to_frequency();
                    }
                });
                ui.end_row();
            }
            ui.label(tl("Wavelength", "Dalga boyu"));
            ui.label(format!("{:.1} mm", C0 / app.project.model.frequency() * MM));
            ui.end_row();
            ui.label(tl("Line impedance Z₀", "Hat empedansı Z₀"));
            ui.add(egui::DragValue::new(&mut app.project.z0).range(1.0..=1000.0).suffix(" Ω"));
            ui.end_row();
        });
    });

    if !app.warnings.is_empty() {
        egui::Frame::new().fill(Color32::from_rgb(60, 44, 20)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for w in &app.warnings {
                ui.label(RichText::new(format!("⚠ {w}")).color(theme::WARM));
            }
        });
    }
    ground_section(app, ui);
    if app.project.magloop.is_some() {
        loop_section(app, ui);
    }

    let selected = app.selected;
    match &mut app.project.model {
        Model::Yagi(d) => yagi_editor(ui, d, selected),
        Model::Wires(a) => wire_editor(ui, a, selected),
    }

    loads_section(app, ui);
    mechanics_section(app, ui);

    section(ui, tl("Notes", "Notlar"), false, |ui| {
        ui.add(egui::TextEdit::multiline(&mut app.project.notes).desired_rows(4).desired_width(f32::INFINITY));
    });
}

/// Manyetik loop parametreleri, kondansatör ayarı ve kuplaj halkası eşlemesi.
fn loop_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    let Some(mut m) = app.project.magloop.clone() else { return };
    let f = app.project.model.frequency();
    let lambda = C0 / f;
    let mut tune = false;
    let mut do_match = false;
    section(ui, tl("Magnetic loop", "Manyetik loop"), true, |ui| {
        egui::Grid::new("loop_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(tl("Loop diameter", "Halka çapı"));
            ui.add(egui::DragValue::new(&mut m.diameter).range(0.05..=20.0).speed(0.005).max_decimals(3).suffix(" m"));
            ui.end_row();
            ui.label(tl("  circumference", "  çevre"));
            ui.label(RichText::new(format!("{:.2} m = {:.3} λ", m.circumference(), m.circumference() / lambda)).color(theme::TEXT_DIM));
            ui.end_row();
            ui.label(tl("Shape", "Biçim"));
            ui.horizontal(|ui| {
                for (n, l) in [(8usize, tl("octagon", "sekizgen")), (16, tl("16-gon", "16 kenar")), (24, tl("≈ circle", "≈ daire"))] {
                    ui.selectable_value(&mut m.sides, n, l);
                }
            });
            ui.end_row();
            ui.label(tl("Tube", "Boru"));
            let mut d = m.tube.diameter * MM;
            if ui.add(egui::DragValue::new(&mut d).range(1.0..=200.0).speed(0.2).suffix(" mm")).changed() {
                m.tube.diameter = d / MM;
            }
            ui.end_row();
            ui.label(tl("  material", "  malzeme"));
            ui.horizontal(|ui| {
                let cu = m.tube.material.name == "Copper";
                if ui.selectable_label(cu, tl("copper", "bakır")).on_hover_text(Material::copper().display_name()).clicked() {
                    m.tube.material = Material::copper();
                }
                if ui.selectable_label(!cu, tl("aluminum", "alüminyum")).on_hover_text(Material::aluminum_6063().display_name()).clicked() {
                    m.tube.material = Material::aluminum_6063();
                }
            });
            ui.end_row();
            ui.label(tl("Coupling loop", "Kuplaj halkası"));
            ui.horizontal(|ui| {
                let mut direct = !m.coupled();
                if ui.checkbox(&mut direct, tl("direct feed (analysis)", "doğrudan besleme (analiz)")).on_hover_text(tl("Feeds the main loop directly at the bottom: shows the loop's own resistance (milliohms). Not for real use.", "Ana halkayı alttan doğrudan besler: halkanın kendi direncini (miliohm) gösterir. Gerçek kullanım için değildir.")).changed() {
                    m.coupling_ratio = if direct { 0.0 } else { 0.2 };
                    tune = true;
                }
            });
            ui.end_row();
            if m.coupled() {
                ui.label(tl("  diameter", "  çapı"));
                ui.horizontal(|ui| {
                    let mut dc = m.coupling_ratio * m.diameter * MM;
                    if ui.add(egui::DragValue::new(&mut dc).range(10.0..=m.diameter * 600.0).speed(1.0).suffix(" mm")).changed() {
                        m.coupling_ratio = dc / MM / m.diameter;
                    }
                    ui.label(RichText::new(format!("D/{:.1}", 1.0 / m.coupling_ratio)).color(theme::TEXT_DIM));
                });
                ui.end_row();
                ui.label(tl("  gap to loop", "  halkaya açıklık"));
                let mut g = m.coupling_gap * MM;
                if ui.add(egui::DragValue::new(&mut g).range(0.0..=500.0).speed(0.5).suffix(" mm")).changed() {
                    m.coupling_gap = g / MM;
                }
                ui.end_row();
                ui.label(tl("  wire", "  tel"));
                let mut w = m.coupling_wire * MM;
                if ui.add(egui::DragValue::new(&mut w).range(0.5..=30.0).speed(0.1).suffix(" mm")).changed() {
                    m.coupling_wire = w / MM;
                }
                ui.end_row();
            }
            ui.label(tl("Tuning capacitor", "Ayar kondansatörü"));
            let mut c = m.capacitance * 1e12;
            if ui.add(egui::DragValue::new(&mut c).range(0.5..=20000.0).speed(0.05).max_decimals(2).suffix(" pF")).changed() {
                m.capacitance = c / 1e12;
            }
            ui.end_row();
            ui.label(tl("  ESR (loss)", "  ESR (kayıp)"));
            let mut r = m.cap_esr * 1e3;
            if ui.add(egui::DragValue::new(&mut r).range(0.0..=1000.0).speed(0.2).suffix(" mΩ")).on_hover_text(tl("Butterfly or vacuum capacitor ≈ 5–10 mΩ, ordinary air variable with wiper contacts ≈ 20–100 mΩ.", "Kelebek ya da vakum kondansatör ≈ 5–10 mΩ, sürtünmeli kontaklı sıradan döner kondansatör ≈ 20–100 mΩ.")).changed() {
                m.cap_esr = r / 1e3;
            }
            ui.end_row();
            ui.label(tl("Transmitter power", "Verici gücü"));
            ui.add(egui::DragValue::new(&mut app.project.tx_power).range(0.1..=5000.0).speed(1.0).suffix(" W"));
            ui.end_row();
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(tl("🎯 Tune capacitor", "🎯 Kondansatörü ayarla")).on_hover_text(tl("Sets the capacitor for the lowest SWR at the current frequency (done automatically when the frequency changes)", "Kondansatörü şimdiki frekansta en düşük SWR'ye ayarlar (frekans değişince kendiliğinden yapılır)")).clicked() {
                tune = true;
            }
            if m.coupled() && ui.button(tl("🎯 Match coupling loop", "🎯 Kuplaj halkasını eşle")).on_hover_text(tl("Adjusts the coupling loop diameter and its gap to the main loop for SWR ≈ 1", "SWR ≈ 1 için kuplaj halkasının çapını ve ana halkaya açıklığını ayarlar")).clicked() {
                do_match = true;
            }
        });
        ui.label(RichText::new(tl("Not included: solder and clamp contact resistance, proximity effect, nearby objects (walls, railings). Real-ground loss does not enter the impedance — near the ground the real efficiency is lower.", "Hesaba girmeyenler: lehim ve kelepçe kontak direnci, yakınlık etkisi, yakındaki nesneler (duvar, korkuluk). Gerçek zemin kaybı empedansa girmez — yere yakın kurulumda gerçek verim daha düşüktür.")).small().color(theme::TEXT_DIM));
    });
    if app.project.magloop.as_ref() != Some(&m) {
        app.project.magloop = Some(m);
        app.project.rebuild_loop();
    }
    if do_match {
        app.status = match app.project.match_loop() {
            Some(s) => tf!("Coupling loop matched: SWR {:.2}", "Kuplaj halkası eşlendi: SWR {:.2}", s),
            None => tl("Coupling loop could not be matched", "Kuplaj halkası eşlenemedi").into(),
        };
    } else if tune {
        app.status = if app.project.retune_loop() {
            tl("Capacitor tuned", "Kondansatör ayarlandı").into()
        } else {
            tl("The loop cannot be tuned with a capacitor at this frequency (it is too large: above its self-resonance)", "Halka bu frekansta kondansatörle ayarlanamaz (çok büyük: kendi rezonansının üstünde)").into()
        };
    }
}

fn ground_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    use antan_core::geometry::Ground;
    let lambda = C0 / app.project.model.frequency();
    section(ui, tl("Ground and height", "Zemin ve yükseklik"), true, |ui| {
        let g = app.project.model.ground_mut();
        let kind = match g {
            Ground::FreeSpace => 0,
            Ground::Perfect => 1,
            Ground::Real { .. } => 2,
        };
        let mut k = kind;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut k, 0, tl("Free space", "Serbest uzay"));
            ui.selectable_value(&mut k, 1, tl("Perfect ground", "Kusursuz zemin"));
            ui.selectable_value(&mut k, 2, tl("Real ground", "Gerçek zemin"));
        });
        if k != kind {
            *g = match k {
                0 => Ground::FreeSpace,
                1 => Ground::Perfect,
                _ => Ground::Real { permittivity: 13.0, conductivity: 0.005 },
            };
            // Serbest uzaydan zemine geçerken z ≈ 0'da (ya da altında) duran tel modelini
            // makul bir yüksekliğe kaldır: λ/2, en çok 10 m.
            if kind == 0
                && let Model::Wires(a) = &mut app.project.model
            {
                let lo = a.wires.iter().flat_map(|w| [w.start[2], w.end[2]]).fold(f64::INFINITY, f64::min);
                if lo < 0.01 * lambda {
                    let h = (0.5 * lambda).min(10.0);
                    *a = antan_core::scan::at_height(a, h);
                    app.status = tf!("Antenna lifted to {:.2} m above ground (change it under Ground and height)", "Anten zeminden {:.2} m yükseğe kaldırıldı (Zemin ve yükseklik bölümünden değiştirin)", h);
                }
            }
        }
        let g = app.project.model.ground_mut();
        if let Ground::Real { permittivity, conductivity } = g {
            egui::Grid::new("real_ground").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tl("Preset", "Hazır"));
                let current = Ground::presets().iter().find(|p| (p.1 - *permittivity).abs() < 1e-9 && (p.2 - *conductivity).abs() < 1e-12).map(|p| p.0).unwrap_or(tl("Custom", "Özel"));
                egui::ComboBox::from_id_salt("ground_preset").selected_text(current).width(200.0).show_ui(ui, |ui| {
                    for (name, e, s) in Ground::presets() {
                        if ui.selectable_label(current == name, format!("{name}  (εr {e}, σ {s})")).clicked() {
                            *permittivity = e;
                            *conductivity = s;
                        }
                    }
                });
                ui.end_row();
                ui.label(tl("Dielectric constant εr", "Dielektrik sabiti εr"));
                ui.add(egui::DragValue::new(permittivity).range(1.0..=100.0).speed(0.1));
                ui.end_row();
                ui.label(tl("Conductivity σ", "İletkenlik σ"));
                ui.add(egui::DragValue::new(conductivity).range(0.0..=10.0).speed(0.0005).max_decimals(4).suffix(" S/m"));
                ui.end_row();
            });
        }
        let grounded = !g.is_free();
        match &mut app.project.model {
            Model::Yagi(d) => {
                ui.add_enabled_ui(grounded, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tl("Boom height", "Bum yüksekliği"));
                        let mut h = d.height;
                        if ui.add(egui::DragValue::new(&mut h).range(0.05..=500.0).speed(0.05).max_decimals(2).suffix(" m")).changed() {
                            d.height = h;
                        }
                        ui.label(RichText::new(format!("{:.2} λ", d.height / lambda)).color(theme::TEXT_DIM));
                    });
                });
            }
            Model::Wires(a) => {
                let (lo, hi) = a.bounds();
                ui.add_enabled_ui(grounded, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tl("Height (lowest point)", "Yükseklik (en alt nokta)"));
                        let mut h = lo[2];
                        if ui.add(egui::DragValue::new(&mut h).range(0.0..=1000.0).speed(0.05).max_decimals(2).suffix(" m")).changed() {
                            *a = antan_core::scan::at_height(a, h);
                        }
                        ui.label(RichText::new(format!("{:.2} λ", lo[2] / lambda)).color(theme::TEXT_DIM));
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tf!("Height range: {:.0} … {:.0} mm", "Yükseklik aralığı: {:.0} … {:.0} mm", lo[2] * MM, hi[2] * MM)).small().color(theme::TEXT_DIM));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(tl("Shift all (z)", "Tümünü kaydır (z)"));
                    for (l, dz) in [("−λ/4", -0.25), ("−1 m", -1.0 / lambda), ("+1 m", 1.0 / lambda), ("+λ/4", 0.25)] {
                        if ui.small_button(l).clicked() {
                            for w in &mut a.wires {
                                w.start[2] += dz * lambda;
                                w.end[2] += dz * lambda;
                            }
                        }
                    }
                    if ui.small_button(tl("sit on ground", "zemine otur")).on_hover_text(tl("Shifts the model so its lowest point is at z = 0", "En alt nokta z = 0 olacak şekilde kaydırır")).clicked() {
                        for w in &mut a.wires {
                            w.start[2] -= lo[2];
                            w.end[2] -= lo[2];
                        }
                    }
                });
            }
        }
        if ui
            .add_enabled(grounded, egui::Button::new(tl("📈 Height scan…", "📈 Yükseklik taraması…")))
            .on_hover_text(tl("Gain, takeoff angle and impedance versus height", "Yüksekliğe göre kazanç, kalkış açısı ve empedans"))
            .on_disabled_hover_text(tl("Select perfect or real ground first", "Önce kusursuz ya da gerçek zemin seçin"))
            .clicked()
        {
            app.heightscan.open = true;
        }
        if grounded {
            ui.label(
                RichText::new(tl("Ground is the z = 0 plane. Currents are solved with a perfect-ground image; over real ground the far field uses Fresnel coefficients (MININEC/MMANA approach). Soil loss does not enter the feed impedance.", "Zemin z = 0 düzlemidir. Akımlar kusursuz zemin görüntüsüyle bulunur; gerçek zeminde uzak alan Fresnel katsayılarıyla hesaplanır (MININEC/MMANA yaklaşımı). Toprak kaybı besleme empedansına girmez."))
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
    });
}

fn yagi_editor(ui: &mut egui::Ui, d: &mut YagiDesign, selected: usize) {
    let lambda = C0 / d.frequency;
    section(ui, tl("Elements", "Elemanlar"), true, |ui| {
        let mut remove = None;
        egui::Grid::new("elements").num_columns(6).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(tl("Feed", "Bes.")).small()).on_hover_text(tl("Driven (fed) element", "Sürülen (beslenen) eleman"));
            ui.label(RichText::new(tl("Name", "Ad")).small());
            ui.label(RichText::new(tl("Boom position", "Bum konumu")).small());
            ui.label(RichText::new(tl("Electrical length", "Elektriksel boy")).small()).on_hover_text(tl("Length used by the solver, without boom effect", "Çözücünün kullandığı, bum etkisi olmayan boy"));
            ui.label(RichText::new(tl("Cut length", "Kesim boyu")).small()).on_hover_text(tl("Electrical length + boom correction", "Elektriksel boy + bum düzeltmesi"));
            ui.label("");
            ui.end_row();
            for i in 0..d.elements.len() {
                ui.radio_value(&mut d.driven, i, "");
                let corr = d.boom_correction(i);
                let e = &mut d.elements[i];
                let r = ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(&mut e.name));
                if i == selected {
                    r.highlight();
                }
                mm_value(ui, &mut e.x, 1.0, -50.0..=50.0);
                ui.horizontal(|ui| {
                    mm_value(ui, &mut e.length, 0.5, 0.01..=60.0);
                    ui.label(RichText::new(format!("{:.3}λ", e.length / lambda)).small().color(theme::TEXT_DIM));
                });
                ui.label(RichText::new(format!("{:.1}", (e.length + corr) * MM)).color(if corr > 0.0 { theme::WARM } else { Color32::WHITE }))
                    .on_hover_text(tf!("Boom correction: +{:.2} mm", "Bum düzeltmesi: +{:.2} mm", corr * MM));
                if ui.small_button("🗑").on_hover_text(tl("Delete element", "Elemanı sil")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove
            && d.elements.len() > 2
            && i != d.driven
        {
            d.elements.remove(i);
            if d.driven > i {
                d.driven -= 1;
            }
        }
        ui.horizontal(|ui| {
            if ui.button(tl("Sort by position", "Konuma göre sırala")).clicked() {
                d.sort();
            }
            ui.label(RichText::new(tf!("Boom: {:.0} mm ({:.2} λ)", "Bum: {:.0} mm ({:.2} λ)", d.boom_length() * MM, d.boom_length() / lambda)).color(theme::TEXT_DIM));
        });
        ui.separator();
        // LFA: sürülen eleman eleman düzleminde yatan dikdörtgen halka.
        ui.horizontal_wrapped(|ui| {
            let mut lfa = d.loop_width().is_some();
            if ui
                .checkbox(&mut lfa, tl("LFA driven loop", "LFA sürülen halka"))
                .on_hover_text(tl(
                    "Loop Fed Array (G0KSC style): the driven element is a rectangular loop lying in the element plane, fed at the middle of its rear side. Its length is the long side; the width is measured along the boom.",
                    "Loop Fed Array (G0KSC tarzı): sürülen eleman, eleman düzleminde yatan dikdörtgen halkadır; arka kenarın ortasından beslenir. Boy uzun kenardır, genişlik bum boyunca ölçülür.",
                ))
                .changed()
            {
                d.driven_loop = if lfa { Some(0.08 * lambda) } else { None };
            }
            if let Some(w) = &mut d.driven_loop {
                ui.label(tl("loop width", "halka genişliği"));
                mm_value(ui, w, 0.5, 0.001..=20.0);
                let len = d.elements.get(d.driven).map(|e| e.length).unwrap_or(0.0);
                ui.label(RichText::new(tf!("perimeter {:.2} λ", "çevre {:.2} λ", 2.0 * (len + *w) / lambda)).small().color(theme::TEXT_DIM));
            }
        });
    });

    section(ui, tl("Tube profile (tapered element)", "Boru profili (kademeli eleman)"), true, |ui| {
        ui.label(RichText::new(tl("Sections from the center outwards; each length is per side. The remainder is the tip tube. All elements share the same profile.", "Ortadan uca doğru bölümler; her bölüm bir taraftaki uzunluktur. Kalan kısım uç borusudur. Tüm elemanlarda aynı profil kullanılır.")).small().color(theme::TEXT_DIM));
        ui.horizontal(|ui| {
            ui.label(tl("Material", "Malzeme"));
            let mut m = d.profile.tip.material.clone();
            if material_combo(ui, "yagi_mat", &mut m) {
                d.profile.tip.material = m.clone();
                for s in &mut d.profile.sections {
                    s.tube.material = m.clone();
                }
            }
        });
        let mut remove = None;
        egui::Grid::new("profile").num_columns(4).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(tl("Section", "Bölüm")).small());
            ui.label(RichText::new(tl("Length", "Uzunluk")).small());
            ui.label(RichText::new(tl("Outer diameter / type / wall", "Dış çap / yapı / et")).small());
            ui.label("");
            ui.end_row();
            for (k, s) in d.profile.sections.iter_mut().enumerate() {
                ui.label(format!("{}", k + 1));
                mm_value(ui, &mut s.half_length, 1.0, 0.005..=20.0);
                ui.horizontal(|ui| tube_editor(ui, &mut s.tube.diameter, &mut s.tube.wall));
                if ui.small_button("🗑").clicked() {
                    remove = Some(k);
                }
                ui.end_row();
            }
            ui.label(tl("Tip", "Uç"));
            ui.label(RichText::new(tl("rest", "kalan")).color(theme::TEXT_DIM));
            ui.horizontal(|ui| tube_editor(ui, &mut d.profile.tip.diameter, &mut d.profile.tip.wall));
            ui.label("");
            ui.end_row();
        });
        if let Some(k) = remove {
            d.profile.sections.remove(k);
        }
        if ui.button(tl("+ Add inner section", "+ İç bölüm ekle")).on_hover_text(tl("Adds a thicker tube section at the center", "Ortaya daha kalın bir boru bölümü ekler")).clicked() {
            let tip = d.profile.tip.clone();
            let base = d.profile.sections.last().map(|s| s.tube.clone()).unwrap_or(tip);
            let tube = Tube { diameter: base.diameter + 0.002, ..base };
            d.profile.sections.push(TaperSection { half_length: 0.1 * lambda, tube });
            // Dıştaki bölüm içtekinden ince olmalı: listeyi çapa göre azalan sırala.
            d.profile.sections.sort_by(|a, b| b.tube.diameter.partial_cmp(&a.tube.diameter).unwrap());
        }
    });

    boom_section(ui, d);
    cut_list_section(ui, d);
}

// ------------------------------------------------------------------ yüklemeler

/// Değer düzenleyici: SI değeri `scale` ile gösterim birimine çevirir.
fn unit_value(ui: &mut egui::Ui, v: &mut f64, scale: f64, speed: f64, suffix: &str) -> bool {
    let mut x = *v * scale;
    let r = ui.add(egui::DragValue::new(&mut x).speed(speed).range(0.0..=1e9).max_decimals(3).suffix(suffix));
    if r.changed() {
        *v = x / scale;
        true
    } else {
        false
    }
}

/// Yük türü ve parametreleri. Değiştiyse true.
fn load_kind_editor(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, k: &mut LoadKind) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(("load_kind", &id)).selected_text(k.label()).width(210.0).show_ui(ui, |ui| {
        let l_now = antan_core::loading::inductance(k).or(match *k {
            LoadKind::Trap { l, .. } => Some(l),
            _ => None,
        });
        let l = l_now.unwrap_or(5e-6);
        for opt in [
            LoadKind::SeriesRlc { r: 0.0, l, c: 0.0 },
            LoadKind::ParallelRlc { r: 0.0, l, c: 100e-12 },
            LoadKind::Trap { l, resonance: 7.1e6, q: 150.0 },
            LoadKind::Fixed { r: 0.0, x: 0.0 },
        ] {
            if ui.selectable_label(std::mem::discriminant(k) == std::mem::discriminant(&opt), opt.label()).clicked() && std::mem::discriminant(k) != std::mem::discriminant(&opt) {
                *k = opt;
                changed = true;
            }
        }
    });
    ui.horizontal_wrapped(|ui| match k {
        LoadKind::SeriesRlc { r, l, c } | LoadKind::ParallelRlc { r, l, c } => {
            ui.label("R");
            changed |= unit_value(ui, r, 1.0, 0.1, " Ω");
            ui.label("L");
            changed |= unit_value(ui, l, 1e6, 0.01, " µH");
            ui.label("C");
            changed |= unit_value(ui, c, 1e12, 0.5, " pF");
        }
        LoadKind::Trap { l, resonance, q } => {
            ui.label("L");
            changed |= unit_value(ui, l, 1e6, 0.01, " µH");
            ui.label("f₀");
            changed |= unit_value(ui, resonance, 1e-6, 0.005, " MHz");
            ui.label("Q");
            changed |= unit_value(ui, q, 1.0, 1.0, "");
            ui.label(RichText::new(format!("C = {:.1} pF", LoadKind::trap_capacitance(*l, *resonance) * 1e12)).color(theme::TEXT_DIM));
        }
        LoadKind::Fixed { r, x } => {
            ui.label("R");
            changed |= unit_value(ui, r, 1.0, 0.1, " Ω");
            ui.label("X");
            let mut v = *x;
            if ui.add(egui::DragValue::new(&mut v).speed(0.5).suffix(" Ω")).changed() {
                *x = v;
                changed = true;
            }
        }
    });
    if let LoadKind::SeriesRlc { c, .. } = k
        && *c == 0.0
    {
        ui.label(RichText::new(tl("C = 0: no capacitor (short circuit).", "C = 0: kondansatör yok (kısa devre).")).small().color(theme::TEXT_DIM));
    }
    changed
}

fn loads_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    let f = app.project.model.frequency();
    let title = match app.project.model {
        Model::Yagi(_) => tl("Element loads (coil, trap)", "Eleman yüklemeleri (bobin, tuzak)"),
        Model::Wires(_) => tl("Loads (coil, capacitor, trap)", "Yüklemeler (bobin, kondansatör, tuzak)"),
    };
    let mut tune: Option<usize> = None;
    section(ui, title, false, |ui| {
        let mut remove = None;
        match &mut app.project.model {
            Model::Yagi(d) => {
                let names: Vec<String> = d.elements.iter().map(|e| e.name.clone()).collect();
                for (i, ld) in d.loads.iter_mut().enumerate() {
                    egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tf!("Load {}", "Yük {}", i + 1)).strong());
                            egui::ComboBox::from_id_salt(("yl_el", i)).selected_text(names.get(ld.element).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                                for (k, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut ld.element, k, n);
                                }
                            });
                            if ui.small_button("🗑").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(tl("Position (center to tip)", "Konum (ortadan uca)"));
                            ui.add(egui::Slider::new(&mut ld.position, 0.0..=1.0).max_decimals(3));
                        });
                        ui.label(RichText::new(if ld.position < 1e-9 { tl("Single load at the center (in series with the feed on the driven element).", "Ortada tek yük (sürülen elemanda beslemeyle seri).") } else { tl("One symmetric load on each arm.", "İki kolda simetrik birer yük.") }).small().color(theme::TEXT_DIM));
                        load_kind_editor(ui, ("y", i), &mut ld.kind);
                        let z = ld.kind.impedance(f);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Z({:.3} MHz) = {}", f / 1e6, crate::viz::fmt_z(z))).color(theme::ACCENT));
                            if antan_core::loading::inductance(&ld.kind).is_some() && ui.small_button(tl("Tune L to resonance", "L'yi rezonansa ayarla")).clicked() {
                                tune = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    d.loads.remove(i);
                }
                if ui.button(tl("+ Add load", "+ Yük ekle")).clicked() {
                    d.loads.push(ElementLoad { element: d.driven, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 2e-6, c: 0.0 } });
                }
            }
            Model::Wires(a) => {
                let names: Vec<String> = a.wires.iter().map(|w| w.name.clone()).collect();
                for (i, ld) in a.loads.iter_mut().enumerate() {
                    egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tf!("Load {}", "Yük {}", i + 1)).strong());
                            egui::ComboBox::from_id_salt(("wl_w", i)).selected_text(names.get(ld.wire).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                                for (k, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut ld.wire, k, n);
                                }
                            });
                            if ui.small_button("🗑").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(tl("Position", "Konum"));
                            ui.add(egui::Slider::new(&mut ld.position, 0.0..=1.0).max_decimals(3));
                            if ui.small_button(tl("center", "orta")).clicked() {
                                ld.position = 0.5;
                            }
                        });
                        load_kind_editor(ui, ("w", i), &mut ld.kind);
                        let z = ld.kind.impedance(f);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Z({:.3} MHz) = {}", f / 1e6, crate::viz::fmt_z(z))).color(theme::ACCENT));
                            if antan_core::loading::inductance(&ld.kind).is_some() && ui.small_button(tl("Tune L to resonance", "L'yi rezonansa ayarla")).clicked() {
                                tune = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    a.loads.remove(i);
                }
                if ui.button(tl("+ Add load", "+ Yük ekle")).clicked() {
                    let w = app.selected.min(a.wires.len().saturating_sub(1));
                    a.loads.push(Load { wire: w, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 2e-6, c: 0.0 } });
                }
            }
        }
        ui.label(
            RichText::new(tl("A load is inserted as a series impedance at the nearest node (like NEC LD cards). A load at a wire end is valid only if that end joins exactly one other wire. Use series R or a trap's Q for coil loss; the loss shows up in the efficiency.", "Yük, en yakın düğüme seri empedans olarak girer (NEC LD kartları gibi). Uca konan yük o uç başka tek bir telle birleşiyorsa geçerlidir. Bobin kaybı için seri R ya da tuzakta Q kullanın; kayıp verime yansır."))
                .small()
                .color(theme::TEXT_DIM),
        );
    });
    if let Some(i) = tune {
        tune_load(app, i);
    }
}

/// Seçilen yükün endüktansını besleme reaktansı 0 olana kadar ayarlar.
fn tune_load(app: &mut AntAnApp, i: usize) {
    use antan_core::loading::{inductance, set_inductance, tune_inductance};
    let result = match &app.project.model {
        Model::Wires(a) => {
            // Simetrik eşi varsa (kısaltılmış dipol gibi) ikisi birlikte ayarlanır.
            let group: Vec<usize> = std::iter::once(i).chain(antan_core::loading::symmetric_partner(a, i)).collect();
            antan_core::loading::tune_loads_inductance(a, &group, 0.0)
        }
        Model::Yagi(d) => {
            let f = d.frequency;
            let start = d.loads.get(i).and_then(|l| inductance(&l.kind));
            start.and_then(|l0| {
                tune_inductance(l0, 0.0, |l| {
                    let mut d2 = d.clone();
                    set_inductance(&mut d2.loads[i].kind, l);
                    antan_core::solver::solve(&d2.build(), f).ok().map(|s| s.impedance())
                })
            })
        }
    };
    match result {
        Some((l, z)) => {
            match &mut app.project.model {
                Model::Wires(a) => {
                    if let Some(j) = antan_core::loading::symmetric_partner(a, i) {
                        set_inductance(&mut a.loads[j].kind, l);
                    }
                    set_inductance(&mut a.loads[i].kind, l);
                }
                Model::Yagi(d) => set_inductance(&mut d.loads[i].kind, l),
            }
            app.status = tf!("Load {}: L = {:.3} µH, Z = {}", "Yük {}: L = {:.3} µH, Z = {}", i + 1, l * 1e6, crate::viz::fmt_z(z));
        }
        None => app.status = tf!("Load {} could not be tuned (no convergence or no solution)", "Yük {} ayarlanamadı (yakınsamadı ya da çözüm yok)", i + 1),
    }
}

fn mount_combo(ui: &mut egui::Ui, id: &str, m: &mut BoomMount) {
    egui::ComboBox::from_id_salt(id).selected_text(m.label()).width(220.0).show_ui(ui, |ui| {
        for opt in [BoomMount::ThroughBonded, BoomMount::ThroughInsulated, BoomMount::None] {
            if ui.selectable_label(*m == opt, opt.label()).clicked() {
                *m = opt;
            }
        }
        if ui.selectable_label(matches!(m, BoomMount::Custom(_)), tl("User defined (e.g. above the boom)", "Kullanıcı tanımlı (ör. bum üstünde)")).clicked() && !matches!(m, BoomMount::Custom(_)) {
            *m = BoomMount::Custom(0.0);
        }
    });
    if let BoomMount::Custom(v) = m {
        ui.horizontal(|ui| {
            ui.label(tl("  extra length", "  ek boy"));
            mm_value(ui, v, 0.05, -0.1..=0.1);
        });
    }
}

fn boom_section(ui: &mut egui::Ui, d: &mut YagiDesign) {
    let f = d.frequency;
    let lambda = C0 / f;
    section(ui, tl("Boom and boom correction", "Bum ve bum düzeltmesi"), true, |ui| {
        egui::Grid::new("boom").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(if d.boom.square { tl("Side", "Kenar") } else { tl("Outer diameter", "Dış çap") });
            ui.horizontal(|ui| {
                mm_value(ui, &mut d.boom.diameter, 0.2, 0.005..=0.3);
                ui.checkbox(&mut d.boom.square, tl("square tube", "kare profil"));
                ui.label(RichText::new(format!("{:.4} λ", d.boom.diameter / lambda)).small().color(theme::TEXT_DIM));
            });
            ui.end_row();
            ui.label(tl("Wall thickness", "Et kalınlığı"));
            mm_value(ui, &mut d.boom.wall, 0.05, 0.0005..=0.02);
            ui.end_row();
            ui.label(tl("Material", "Malzeme"));
            material_combo(ui, "boom_mat", &mut d.boom.material);
            ui.end_row();
            ui.label(tl("Parasitic elements", "Parazitik elemanlar"));
            ui.vertical(|ui| mount_combo(ui, "mount_par", &mut d.boom.mount));
            ui.end_row();
            ui.label(tl("Driven element", "Sürülen eleman"));
            ui.vertical(|ui| mount_combo(ui, "mount_drv", &mut d.boom.driven_mount));
            ui.end_row();
            ui.label(tl("Overhang (each end)", "Uç taşması (her uç)"));
            mm_value(ui, &mut d.boom.overhang, 1.0, 0.0..=2.0);
            ui.end_row();
            ui.label(tl("Telescoping overlap", "Geçme payı"));
            mm_value(ui, &mut d.boom.overlap, 1.0, 0.0..=0.3);
            ui.end_row();
        });
        let cp = d.boom.correction(d.boom.mount, f);
        let cd = d.boom.correction(d.boom.driven_mount, f);
        ui.label(RichText::new(tf!("Correction: parasitic +{:.2} mm, driven +{:.2} mm · boom length {:.0} mm", "Düzeltme: parazitik +{:.2} mm, sürülen +{:.2} mm · bum boyu {:.0} mm", cp * MM, cd * MM, d.boom_total_length() * MM)).strong());
        if !d.boom.in_range(f) {
            ui.label(RichText::new(tf!("Warning: boom diameter {:.3} λ exceeds the 0.055 λ range of the DL6WU measurements.", "Uyarı: bum çapı {:.3} λ, DL6WU ölçümlerinin geçerli olduğu 0.055 λ'yı aşıyor.", d.boom.diameter / lambda)).color(theme::BAD));
        }
        ui.label(
            RichText::new(
                tl("A metal boom electrically shortens an element passing through it. Formula: G3SEK's fit to DL6WU's measurements, C = 25.195·B − 229·B² (B: boom diameter/λ, C: fraction of boom diameter), half of that for insulated through-boom elements. Design lengths are electrical lengths; the cut length includes the correction. No published formula exists for elements insulated above the boom; the correction is small and decreases with height — enter a user-defined value.", "Metal bum, içinden geçen elemanı elektriksel olarak kısaltır. Formül: G3SEK'in DL6WU ölçümlerine uydurduğu C = 25.195·B − 229·B² (B: bum çapı/λ, C: bum çapının kesri), yalıtılmış geçişte yarısı. Tasarımdaki boylar elektriksel boydur; kesim boyu düzeltme eklenmiş halidir. Bum üstüne yalıtkanla monte elemanlar için yayımlanmış formül yok; düzeltme küçüktür ve yükseklikle azalır — kullanıcı tanımlı değer girin."),
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    });
}

/// Kesim listesini düz metin olarak üretir (panoya kopyalamak için).
pub fn cut_list_text(d: &YagiDesign) -> String {
    let mut s = tf!("{} — cut list ({:.3} MHz)\n", "{} — kesim listesi ({:.3} MHz)\n", d.name, d.frequency / 1e6);
    s += &tf!("Boom: Ø{:.1} mm, length {:.0} mm\n\n", "Bum: Ø{:.1} mm, boy {:.0} mm\n\n", d.boom.diameter * MM, d.boom_total_length() * MM);
    for (i, e) in d.elements.iter().enumerate() {
        s += &tf!("{}  (position {:.1} mm, cut length {:.1} mm)\n", "{}  (konum {:.1} mm, kesim boyu {:.1} mm)\n", e.name, e.x * MM, d.cut_length(i) * MM);
        for p in d.cut_list(i) {
            s += &format!("    {} × {:>8.1} mm  Ø{:.1}{}  {}\n", p.count, p.length * MM, p.tube.diameter * MM, p.tube.wall.map(|w| format!("×{:.1}", w * MM)).unwrap_or_else(|| tl(" solid", " dolu").into()), p.label);
        }
    }
    s
}

fn cut_list_section(ui: &mut egui::Ui, d: &YagiDesign) {
    section(ui, tl("Cut list", "Kesim listesi"), false, |ui| {
        // Boru türüne göre toplam boy (alışveriş listesi).
        let mut totals: Vec<(String, f64)> = Vec::new();
        egui::Grid::new("cuts").num_columns(4).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
            for h in [tl("Element / piece", "Eleman / parça"), tl("Qty", "Adet"), tl("Cut", "Kesim"), tl("Tube", "Boru")] {
                ui.label(RichText::new(h).small().color(theme::TEXT_DIM));
            }
            ui.end_row();
            for (i, e) in d.elements.iter().enumerate() {
                ui.label(RichText::new(&e.name).strong());
                ui.label("");
                ui.label(RichText::new(format!("{:.1} mm", d.cut_length(i) * MM)).strong());
                ui.label("");
                ui.end_row();
                for p in d.cut_list(i) {
                    let tube = format!("Ø{:.1}{}", p.tube.diameter * MM, p.tube.wall.map(|w| format!(" × {:.1}", w * MM)).unwrap_or_else(|| tl(" solid", " dolu").into()));
                    ui.label(format!("   {}", p.label));
                    ui.label(format!("{}", p.count));
                    ui.label(format!("{:.1} mm", p.length * MM));
                    ui.label(&tube);
                    ui.end_row();
                    match totals.iter_mut().find(|t| t.0 == tube) {
                        Some(t) => t.1 += p.length * p.count as f64,
                        None => totals.push((tube, p.length * p.count as f64)),
                    }
                }
            }
        });
        ui.separator();
        ui.label(RichText::new(tl("Total tubing needed (excluding waste)", "Toplam boru ihtiyacı (fire hariç)")).strong());
        for (t, l) in &totals {
            ui.label(format!("  {t}: {:.2} m", l));
        }
        ui.label(tf!("  Boom Ø{:.1} × {:.1}: {:.2} m", "  Bum Ø{:.1} × {:.1}: {:.2} m", d.boom.diameter * MM, d.boom.wall * MM, d.boom_total_length()));
        if ui.button(tl("📋 Copy cut list to clipboard", "📋 Kesim listesini panoya kopyala")).clicked() {
            ui.ctx().copy_text(cut_list_text(d));
        }
    });
}

fn wire_editor(ui: &mut egui::Ui, a: &mut Antenna, selected: usize) {
    let lambda = C0 / a.frequency;
    if a.wires.is_empty() {
        ui.weak(tl("No wires.", "Tel yok."));
        return;
    }
    let sel = selected.min(a.wires.len() - 1);
    // Serbest uçlar arası boşluklar (ör. Moxon: sürülen ve yansıtıcı kuyrukları arası).
    let gaps = antan_core::templates::open_end_gaps(a, 0.25 * lambda);
    if !gaps.is_empty() {
        section(ui, tl("Gaps between wire ends", "Tel uçları arası boşluklar"), true, |ui| {
            ui.label(RichText::new(tl("Distance between free (unconnected) wire ends that face each other, e.g. the gap between the driven and reflector tails of a Moxon.", "Birbirine bakan serbest (bağlanmamış) tel uçları arası mesafe; ör. Moxon'da sürülen ve yansıtıcı kuyrukları arasındaki boşluk.")).small().color(theme::TEXT_DIM));
            egui::Grid::new("gaps").num_columns(2).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                for (i, j, d) in &gaps {
                    ui.label(format!("{} ↔ {}", a.wires[*i].name, a.wires[*j].name));
                    ui.label(RichText::new(format!("{:.1} mm  ({:.4} λ)", d * MM, d / lambda)).strong().color(theme::ACCENT));
                    ui.end_row();
                }
            });
        });
    }
    section(ui, tl("Selected wire", "Seçili tel"), true, |ui| {
        let w = &mut a.wires[sel];
        egui::Grid::new("wire").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(tl("Name", "Ad"));
            ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(170.0));
            ui.end_row();
            for (label, p) in [(tl("Start", "Başlangıç"), &mut w.start), (tl("End", "Bitiş"), &mut w.end)] {
                ui.label(label);
                ui.horizontal(|ui| {
                    for (axis, c) in ["x", "y", "z"].iter().zip(p.iter_mut()) {
                        ui.label(RichText::new(*axis).small().color(theme::TEXT_DIM));
                        let mut v = *c * MM;
                        if ui.add(egui::DragValue::new(&mut v).speed(1.0).max_decimals(1)).changed() {
                            *c = v / MM;
                        }
                    }
                });
                ui.end_row();
            }
            ui.label(tl("Length", "Uzunluk"));
            ui.label(format!("{:.1} mm ({:.3} λ)", w.length() * MM, w.length() / lambda));
            ui.end_row();
            ui.label(tl("Outer diameter", "Dış çap"));
            ui.horizontal(|ui| tube_editor(ui, &mut w.diameter, &mut w.wall));
            ui.end_row();
            ui.label(tl("Material", "Malzeme"));
            material_combo(ui, "wire_mat", &mut w.material);
            ui.end_row();
            ui.label(tl("Segments", "Parça sayısı"));
            ui.horizontal(|ui| {
                let mut auto = w.segments == 0;
                if ui.checkbox(&mut auto, tl("auto", "otomatik")).changed() {
                    w.segments = if auto { 0 } else { w.segment_count(lambda) };
                }
                if !auto {
                    ui.add(egui::DragValue::new(&mut w.segments).range(4..=400));
                } else {
                    ui.label(RichText::new(format!("({})", w.segment_count(lambda))).color(theme::TEXT_DIM));
                }
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(tl("Apply diameter/material to all wires", "Tüm tellere uygula: çap/malzeme")).clicked() {
                let (d, t, m) = (a.wires[sel].diameter, a.wires[sel].wall, a.wires[sel].material.clone());
                for w in &mut a.wires {
                    w.diameter = d;
                    w.wall = t;
                    w.material = m.clone();
                }
            }
        });
    });

    section(ui, tl("Feed", "Besleme"), true, |ui| {
        if a.sources.is_empty() {
            a.sources.push(antan_core::templates::source(0, 0.5));
        }
        let names: Vec<String> = a.wires.iter().map(|w| w.name.clone()).collect();
        let s = &mut a.sources[0];
        egui::Grid::new("feed").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(tl("Wire", "Tel"));
            egui::ComboBox::from_id_salt("feed_wire").selected_text(names.get(s.wire).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut s.wire, i, n);
                }
            });
            ui.end_row();
            ui.label(tl("Position", "Konum"));
            ui.add(egui::Slider::new(&mut s.position, 0.0..=1.0).max_decimals(3));
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                for (l, v) in [(tl("start", "baş"), 0.0), (tl("center", "orta"), 0.5), (tl("end", "son"), 1.0)] {
                    if ui.small_button(l).clicked() {
                        s.position = v;
                    }
                }
            });
            ui.end_row();
        });
        ui.label(RichText::new(tl("End feeding is valid only at ends joined to other wires or to ground (e.g. inverted-V apex, GP base).", "Uçtan besleme yalnız başka tellerle birleşen uçlarda geçerlidir (ör. ters V tepesi, GP tabanı).")).small().color(theme::TEXT_DIM));
        if ui.button(tl("Move feed to the selected wire (center)", "Beslemeyi seçili tele taşı (orta)")).clicked() {
            s.wire = sel;
            s.position = 0.5;
        }
    });
}

fn mechanics_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    section(ui, tl("Mechanics", "Mekanik"), false, |ui| {
        ui.horizontal(|ui| {
            ui.label(tl("Wind speed", "Rüzgâr hızı"));
            ui.add(egui::DragValue::new(&mut app.project.wind_speed).range(0.0..=80.0).suffix(" m/s"));
            ui.label(RichText::new(tf!("({:.0} km/h)", "({:.0} km/sa)", app.project.wind_speed * 3.6)).color(theme::TEXT_DIM));
        });
        let v = app.project.wind_speed;
        let (mut total_mass, mut total_area) = (0.0, 0.0);
        egui::Grid::new("mech").num_columns(4).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
            ui.label(RichText::new(tl("Element", "Eleman")).small());
            ui.label(RichText::new(tl("Mass", "Kütle")).small());
            ui.label(RichText::new(tl("Tip sag", "Uç sarkması")).small());
            ui.label(RichText::new(tl("Wind", "Rüzgâr")).small());
            ui.end_row();
            match &app.project.model {
                Model::Yagi(d) => {
                    for e in &d.elements {
                        let pieces = d.profile.pieces(0.5 * e.length);
                        let mut mass = 0.0;
                        let mut area = 0.0;
                        let mut secs = Vec::new();
                        for (a, b, t) in &pieces {
                            let (ar, _, wl, ei) = tube_props(t);
                            let l = b - a;
                            mass += 2.0 * ar * l * t.material.density;
                            area += 2.0 * t.diameter * l;
                            secs.push((l, wl, ei));
                        }
                        let sag = cantilever_tip_sag(&secs);
                        total_mass += mass;
                        total_area += area;
                        ui.label(&e.name);
                        ui.label(format!("{:.0} g", mass * 1000.0));
                        ui.label(format!("{:.1} mm", sag * MM));
                        ui.label(format!("{:.1} N", wind_force(area, v)));
                        ui.end_row();
                    }
                    let (bm, ba) = d.boom.mass_and_area(d.boom_total_length());
                    total_mass += bm;
                    total_area += ba;
                    ui.label(RichText::new(tl("Boom", "Bum")).strong());
                    ui.label(format!("{:.0} g", bm * 1000.0));
                    ui.label(RichText::new("—").color(theme::TEXT_DIM));
                    ui.label(format!("{:.1} N", wind_force(ba, v)));
                    ui.end_row();
                }
                Model::Wires(a) => {
                    for w in &a.wires {
                        let m = mechanics::wire_mechanics(w);
                        total_mass += m.mass;
                        total_area += m.wind_area;
                        ui.label(&w.name);
                        ui.label(format!("{:.0} g", m.mass * 1000.0));
                        ui.label(RichText::new("—").color(theme::TEXT_DIM));
                        ui.label(format!("{:.1} N", wind_force(m.wind_area, v)));
                        ui.end_row();
                    }
                }
            }
        });
        ui.separator();
        ui.label(RichText::new(tf!("Total mass: {:.2} kg · Wind area: {:.3} m² · Force: {:.0} N", "Toplam eleman kütlesi: {:.2} kg · Rüzgâr alanı: {:.3} m² · Kuvvet: {:.0} N", total_mass, total_area, wind_force(total_area, v))).strong());
        ui.label(RichText::new(tl("Sag: tip deflection of a center-supported element under its own weight (stepped beam). Boom and clamps are not included.", "Sarkma: ortadan tutulan elemanın kendi ağırlığıyla ucunun inişi (kademeli kiriş). Bum ve kelepçeler dahil değildir.")).small().color(theme::TEXT_DIM));
    });
}
