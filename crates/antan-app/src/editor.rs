//! Sol panel: model ağacı. Sağ panel: özellikler (Yagi tasarımı ya da tel modeli).

use eframe::egui::{self, Color32, RichText};

use antan_core::mechanics::{self, cantilever_tip_sag, tube_props, wind_force};
use antan_core::templates::{Tube, wire};
use antan_core::yagi::{TaperSection, YagiDesign, YagiElement};
use antan_core::{Antenna, C0, Material};

use crate::app::AntAnApp;
use crate::project::Model;
use crate::theme;

const MM: f64 = 1000.0;

fn section(ui: &mut egui::Ui, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
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
    egui::ComboBox::from_id_salt(id).selected_text(m.name.clone()).width(180.0).show_ui(ui, |ui| {
        for p in Material::presets() {
            if ui.selectable_label(p.name == m.name, &p.name).clicked() {
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
    if ui.checkbox(&mut hollow, "boru").on_hover_text("İşaretli değilse içi dolu çubuk").changed() {
        *wall = if hollow { Some((0.1 * *d).clamp(0.0005, 0.5 * *d)) } else { None };
        changed = true;
    }
    if let Some(t) = wall {
        changed |= mm_value(ui, t, 0.02, 0.0001..=0.15);
        *t = t.min(0.5 * *d);
    } else {
        ui.weak("dolu");
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
    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| match &app.project.model {
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
            if ui.button("+ Yönlendirici").clicked() {
                action = Some(TreeAction::Add);
            }
            if ui.button("Sil").clicked() {
                action = Some(TreeAction::Remove);
            }
        }
        Model::Wires(_) => {
            if ui.button("+ Tel").clicked() {
                action = Some(TreeAction::Add);
            }
            if ui.button("Kopyala").clicked() {
                action = Some(TreeAction::Duplicate);
            }
            if ui.button("Sil").clicked() {
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
            let n = d.elements.iter().filter(|e| e.name.starts_with("Yönlendirici")).count() + 1;
            d.elements.push(YagiElement { name: format!("Yönlendirici {n}"), x, length: len });
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
                app.status = "Sürülen eleman ya da son iki eleman silinemez".into();
            }
        }
        (Model::Wires(a), TreeAction::Add) => {
            let (_, hi) = a.bounds();
            let x = if hi[0].is_finite() { hi[0] + 0.2 * lambda } else { 0.0 };
            let tube = a.wires.get(app.selected).map(|w| Tube { diameter: w.diameter, wall: w.wall, material: w.material.clone() }).unwrap_or_else(crate::project::default_tube);
            a.wires.push(wire(&format!("Tel {}", a.wires.len() + 1), [x, -0.24 * lambda, 0.0], [x, 0.24 * lambda, 0.0], &tube));
            app.selected = a.wires.len() - 1;
        }
        (Model::Wires(a), TreeAction::Duplicate) => {
            if let Some(w) = a.wires.get(app.selected).cloned() {
                let mut w2 = w;
                w2.name = format!("{} (kopya)", w2.name);
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
    section(ui, "Anten", true, |ui| {
        egui::Grid::new("ant").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Ad");
            ui.add(egui::TextEdit::singleline(app.project.model.name_mut()).desired_width(170.0));
            ui.end_row();
            ui.label("Frekans");
            let mut mhz = app.project.model.frequency() / 1e6;
            if ui.add(egui::DragValue::new(&mut mhz).range(0.1..=30_000.0).speed(0.01).max_decimals(4).suffix(" MHz")).changed() {
                app.project.model.set_frequency(mhz * 1e6);
            }
            ui.end_row();
            ui.label("Dalga boyu");
            ui.label(format!("{:.1} mm", C0 / app.project.model.frequency() * MM));
            ui.end_row();
            ui.label("Hat empedansı Z₀");
            ui.add(egui::DragValue::new(&mut app.project.z0).range(1.0..=1000.0).suffix(" Ω"));
            ui.end_row();
        });
    });

    let selected = app.selected;
    match &mut app.project.model {
        Model::Yagi(d) => yagi_editor(ui, d, selected),
        Model::Wires(a) => wire_editor(ui, a, selected),
    }

    mechanics_section(app, ui);

    section(ui, "Notlar", false, |ui| {
        ui.add(egui::TextEdit::multiline(&mut app.project.notes).desired_rows(4).desired_width(f32::INFINITY));
    });
}

fn yagi_editor(ui: &mut egui::Ui, d: &mut YagiDesign, selected: usize) {
    let lambda = C0 / d.frequency;
    section(ui, "Elemanlar", true, |ui| {
        let mut remove = None;
        egui::Grid::new("elements").num_columns(5).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Bes.").small()).on_hover_text("Sürülen (beslenen) eleman");
            ui.label(RichText::new("Ad").small());
            ui.label(RichText::new("Bum konumu").small());
            ui.label(RichText::new("Uçtan uca boy").small());
            ui.label("");
            ui.end_row();
            for i in 0..d.elements.len() {
                ui.radio_value(&mut d.driven, i, "");
                let e = &mut d.elements[i];
                let r = ui.add_sized([108.0, 20.0], egui::TextEdit::singleline(&mut e.name));
                if i == selected {
                    r.highlight();
                }
                mm_value(ui, &mut e.x, 1.0, -50.0..=50.0);
                ui.horizontal(|ui| {
                    mm_value(ui, &mut e.length, 0.5, 0.01..=60.0);
                    ui.label(RichText::new(format!("{:.3}λ", e.length / lambda)).small().color(theme::TEXT_DIM));
                });
                if ui.small_button("🗑").on_hover_text("Elemanı sil").clicked() {
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
            if ui.button("Konuma göre sırala").clicked() {
                d.sort();
            }
            ui.label(RichText::new(format!("Bum: {:.0} mm ({:.2} λ)", d.boom_length() * MM, d.boom_length() / lambda)).color(theme::TEXT_DIM));
        });
    });

    section(ui, "Boru profili (kademeli eleman)", true, |ui| {
        ui.label(RichText::new("Ortadan uca doğru bölümler; her bölüm bir taraftaki uzunluktur. Kalan kısım uç borusudur. Tüm elemanlarda aynı profil kullanılır.").small().color(theme::TEXT_DIM));
        ui.horizontal(|ui| {
            ui.label("Malzeme");
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
            ui.label(RichText::new("Bölüm").small());
            ui.label(RichText::new("Uzunluk").small());
            ui.label(RichText::new("Dış çap / yapı / et").small());
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
            ui.label("Uç");
            ui.label(RichText::new("kalan").color(theme::TEXT_DIM));
            ui.horizontal(|ui| tube_editor(ui, &mut d.profile.tip.diameter, &mut d.profile.tip.wall));
            ui.label("");
            ui.end_row();
        });
        if let Some(k) = remove {
            d.profile.sections.remove(k);
        }
        if ui.button("+ İç bölüm ekle").on_hover_text("Ortaya daha kalın bir boru bölümü ekler").clicked() {
            let tip = d.profile.tip.clone();
            let base = d.profile.sections.last().map(|s| s.tube.clone()).unwrap_or(tip);
            let tube = Tube { diameter: base.diameter + 0.002, ..base };
            d.profile.sections.push(TaperSection { half_length: 0.1 * lambda, tube });
            // Dıştaki bölüm içtekinden ince olmalı: listeyi çapa göre azalan sırala.
            d.profile.sections.sort_by(|a, b| b.tube.diameter.partial_cmp(&a.tube.diameter).unwrap());
        }
    });
}

fn wire_editor(ui: &mut egui::Ui, a: &mut Antenna, selected: usize) {
    let lambda = C0 / a.frequency;
    if a.wires.is_empty() {
        ui.weak("Tel yok.");
        return;
    }
    let sel = selected.min(a.wires.len() - 1);
    section(ui, "Seçili tel", true, |ui| {
        let w = &mut a.wires[sel];
        egui::Grid::new("wire").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Ad");
            ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(170.0));
            ui.end_row();
            for (label, p) in [("Başlangıç", &mut w.start), ("Bitiş", &mut w.end)] {
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
            ui.label("Uzunluk");
            ui.label(format!("{:.1} mm ({:.3} λ)", w.length() * MM, w.length() / lambda));
            ui.end_row();
            ui.label("Dış çap");
            ui.horizontal(|ui| tube_editor(ui, &mut w.diameter, &mut w.wall));
            ui.end_row();
            ui.label("Malzeme");
            material_combo(ui, "wire_mat", &mut w.material);
            ui.end_row();
            ui.label("Parça sayısı");
            ui.horizontal(|ui| {
                let mut auto = w.segments == 0;
                if ui.checkbox(&mut auto, "otomatik").changed() {
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
            if ui.button("Tüm tellere uygula: çap/malzeme").clicked() {
                let (d, t, m) = (a.wires[sel].diameter, a.wires[sel].wall, a.wires[sel].material.clone());
                for w in &mut a.wires {
                    w.diameter = d;
                    w.wall = t;
                    w.material = m.clone();
                }
            }
        });
    });

    section(ui, "Besleme", true, |ui| {
        if a.sources.is_empty() {
            a.sources.push(antan_core::templates::source(0, 0.5));
        }
        let names: Vec<String> = a.wires.iter().map(|w| w.name.clone()).collect();
        let s = &mut a.sources[0];
        egui::Grid::new("feed").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Tel");
            egui::ComboBox::from_id_salt("feed_wire").selected_text(names.get(s.wire).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut s.wire, i, n);
                }
            });
            ui.end_row();
            ui.label("Konum");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut s.position, 0.0..=1.0).max_decimals(3));
                for (l, v) in [("baş", 0.0), ("orta", 0.5), ("son", 1.0)] {
                    if ui.small_button(l).clicked() {
                        s.position = v;
                    }
                }
            });
            ui.end_row();
        });
        ui.label(RichText::new("Uçtan besleme yalnız başka tellerle birleşen uçlarda geçerlidir (ör. ters V tepesi, GP tabanı).").small().color(theme::TEXT_DIM));
        if ui.button("Beslemeyi seçili tele taşı (orta)").clicked() {
            s.wire = sel;
            s.position = 0.5;
        }
    });
}

fn mechanics_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    section(ui, "Mekanik", false, |ui| {
        ui.horizontal(|ui| {
            ui.label("Rüzgâr hızı");
            ui.add(egui::DragValue::new(&mut app.project.wind_speed).range(0.0..=80.0).suffix(" m/s"));
            ui.label(RichText::new(format!("({:.0} km/sa)", app.project.wind_speed * 3.6)).color(theme::TEXT_DIM));
        });
        let v = app.project.wind_speed;
        let (mut total_mass, mut total_area) = (0.0, 0.0);
        egui::Grid::new("mech").num_columns(4).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
            ui.label(RichText::new("Eleman").small());
            ui.label(RichText::new("Kütle").small());
            ui.label(RichText::new("Uç sarkması").small());
            ui.label(RichText::new("Rüzgâr").small());
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
        ui.label(RichText::new(format!("Toplam eleman kütlesi: {:.2} kg · Rüzgâr alanı: {:.3} m² · Kuvvet: {:.0} N", total_mass, total_area, wind_force(total_area, v))).strong());
        ui.label(RichText::new("Sarkma: ortadan tutulan elemanın kendi ağırlığıyla ucunun inişi (kademeli kiriş). Bum ve kelepçeler dahil değildir.").small().color(theme::TEXT_DIM));
    });
}
