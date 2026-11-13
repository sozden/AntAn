//! Sol panel: model ağacı. Sağ panel: özellikler (Yagi tasarımı ya da tel modeli).

use eframe::egui::{self, Color32, RichText};

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

    ground_section(app, ui);

    let selected = app.selected;
    match &mut app.project.model {
        Model::Yagi(d) => yagi_editor(ui, d, selected),
        Model::Wires(a) => wire_editor(ui, a, selected),
    }

    loads_section(app, ui);
    mechanics_section(app, ui);

    section(ui, "Notlar", false, |ui| {
        ui.add(egui::TextEdit::multiline(&mut app.project.notes).desired_rows(4).desired_width(f32::INFINITY));
    });
}

fn ground_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    use antan_core::geometry::Ground;
    let lambda = C0 / app.project.model.frequency();
    section(ui, "Zemin ve yükseklik", true, |ui| {
        let g = app.project.model.ground_mut();
        let kind = match g {
            Ground::FreeSpace => 0,
            Ground::Perfect => 1,
            Ground::Real { .. } => 2,
        };
        let mut k = kind;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut k, 0, "Serbest uzay");
            ui.selectable_value(&mut k, 1, "Kusursuz zemin");
            ui.selectable_value(&mut k, 2, "Gerçek zemin");
        });
        if k != kind {
            *g = match k {
                0 => Ground::FreeSpace,
                1 => Ground::Perfect,
                _ => Ground::Real { permittivity: 13.0, conductivity: 0.005 },
            };
        }
        if let Ground::Real { permittivity, conductivity } = g {
            egui::Grid::new("real_ground").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Hazır");
                let current = Ground::PRESETS.iter().find(|p| (p.1 - *permittivity).abs() < 1e-9 && (p.2 - *conductivity).abs() < 1e-12).map(|p| p.0).unwrap_or("Özel");
                egui::ComboBox::from_id_salt("ground_preset").selected_text(current).width(200.0).show_ui(ui, |ui| {
                    for (name, e, s) in Ground::PRESETS {
                        if ui.selectable_label(current == name, format!("{name}  (εr {e}, σ {s})")).clicked() {
                            *permittivity = e;
                            *conductivity = s;
                        }
                    }
                });
                ui.end_row();
                ui.label("Dielektrik sabiti εr");
                ui.add(egui::DragValue::new(permittivity).range(1.0..=100.0).speed(0.1));
                ui.end_row();
                ui.label("İletkenlik σ");
                ui.add(egui::DragValue::new(conductivity).range(0.0..=10.0).speed(0.0005).max_decimals(4).suffix(" S/m"));
                ui.end_row();
            });
        }
        let grounded = !g.is_free();
        match &mut app.project.model {
            Model::Yagi(d) => {
                ui.add_enabled_ui(grounded, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Bum yüksekliği");
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
                ui.horizontal(|ui| {
                    ui.label(format!("Yükseklik aralığı: {:.0} … {:.0} mm", lo[2] * MM, hi[2] * MM));
                });
                ui.horizontal(|ui| {
                    ui.label("Tümünü kaydır (z)");
                    for (l, dz) in [("−λ/4", -0.25), ("−1 m", -1.0 / lambda), ("+1 m", 1.0 / lambda), ("+λ/4", 0.25)] {
                        if ui.small_button(l).clicked() {
                            for w in &mut a.wires {
                                w.start[2] += dz * lambda;
                                w.end[2] += dz * lambda;
                            }
                        }
                    }
                    if ui.small_button("zemine otur").on_hover_text("En alt nokta z = 0 olacak şekilde kaydırır").clicked() {
                        for w in &mut a.wires {
                            w.start[2] -= lo[2];
                            w.end[2] -= lo[2];
                        }
                    }
                });
            }
        }
        if grounded {
            ui.label(
                RichText::new("Zemin z = 0 düzlemidir. Akımlar kusursuz zemin görüntüsüyle bulunur; gerçek zeminde uzak alan Fresnel katsayılarıyla hesaplanır (MININEC/MMANA yaklaşımı). Toprak kaybı besleme empedansına girmez.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
    });
}

fn yagi_editor(ui: &mut egui::Ui, d: &mut YagiDesign, selected: usize) {
    let lambda = C0 / d.frequency;
    section(ui, "Elemanlar", true, |ui| {
        let mut remove = None;
        egui::Grid::new("elements").num_columns(6).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Bes.").small()).on_hover_text("Sürülen (beslenen) eleman");
            ui.label(RichText::new("Ad").small());
            ui.label(RichText::new("Bum konumu").small());
            ui.label(RichText::new("Elektriksel boy").small()).on_hover_text("Çözücünün kullandığı, bum etkisi olmayan boy");
            ui.label(RichText::new("Kesim boyu").small()).on_hover_text("Elektriksel boy + bum düzeltmesi");
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
                    .on_hover_text(format!("Bum düzeltmesi: +{:.2} mm", corr * MM));
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
        ui.label(RichText::new("C = 0: kondansatör yok (kısa devre).").small().color(theme::TEXT_DIM));
    }
    changed
}

fn loads_section(app: &mut AntAnApp, ui: &mut egui::Ui) {
    let f = app.project.model.frequency();
    let title = match app.project.model {
        Model::Yagi(_) => "Eleman yüklemeleri (bobin, tuzak)",
        Model::Wires(_) => "Yüklemeler (bobin, kondansatör, tuzak)",
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
                            ui.label(RichText::new(format!("Yük {}", i + 1)).strong());
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
                            ui.label("Konum (ortadan uca)");
                            ui.add(egui::Slider::new(&mut ld.position, 0.0..=1.0).max_decimals(3));
                        });
                        ui.label(RichText::new(if ld.position < 1e-9 { "Ortada tek yük (sürülen elemanda beslemeyle seri)." } else { "İki kolda simetrik birer yük." }).small().color(theme::TEXT_DIM));
                        load_kind_editor(ui, ("y", i), &mut ld.kind);
                        let z = ld.kind.impedance(f);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Z({:.3} MHz) = {}", f / 1e6, crate::viz::fmt_z(z))).color(theme::ACCENT));
                            if antan_core::loading::inductance(&ld.kind).is_some() && ui.small_button("L'yi rezonansa ayarla").clicked() {
                                tune = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    d.loads.remove(i);
                }
                if ui.button("+ Yük ekle").clicked() {
                    d.loads.push(ElementLoad { element: d.driven, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 2e-6, c: 0.0 } });
                }
            }
            Model::Wires(a) => {
                let names: Vec<String> = a.wires.iter().map(|w| w.name.clone()).collect();
                for (i, ld) in a.loads.iter_mut().enumerate() {
                    egui::Frame::new().fill(Color32::from_rgb(34, 40, 51)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Yük {}", i + 1)).strong());
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
                            ui.label("Konum");
                            ui.add(egui::Slider::new(&mut ld.position, 0.0..=1.0).max_decimals(3));
                            if ui.small_button("orta").clicked() {
                                ld.position = 0.5;
                            }
                        });
                        load_kind_editor(ui, ("w", i), &mut ld.kind);
                        let z = ld.kind.impedance(f);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Z({:.3} MHz) = {}", f / 1e6, crate::viz::fmt_z(z))).color(theme::ACCENT));
                            if antan_core::loading::inductance(&ld.kind).is_some() && ui.small_button("L'yi rezonansa ayarla").clicked() {
                                tune = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    a.loads.remove(i);
                }
                if ui.button("+ Yük ekle").clicked() {
                    let w = app.selected.min(a.wires.len().saturating_sub(1));
                    a.loads.push(Load { wire: w, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 2e-6, c: 0.0 } });
                }
            }
        }
        ui.label(
            RichText::new("Yük, en yakın düğüme seri empedans olarak girer (NEC LD kartları gibi). Uca konan yük o uç başka tek bir telle birleşiyorsa geçerlidir. Bobin kaybı için seri R ya da tuzakta Q kullanın; kayıp verime yansır.")
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
        Model::Wires(a) => antan_core::loading::tune_load_inductance(a, i, 0.0),
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
                Model::Wires(a) => set_inductance(&mut a.loads[i].kind, l),
                Model::Yagi(d) => set_inductance(&mut d.loads[i].kind, l),
            }
            app.status = format!("Yük {}: L = {:.3} µH, Z = {}", i + 1, l * 1e6, crate::viz::fmt_z(z));
        }
        None => app.status = format!("Yük {} ayarlanamadı (yakınsamadı ya da çözüm yok)", i + 1),
    }
}

fn mount_combo(ui: &mut egui::Ui, id: &str, m: &mut BoomMount) {
    egui::ComboBox::from_id_salt(id).selected_text(m.label()).width(220.0).show_ui(ui, |ui| {
        for opt in [BoomMount::ThroughBonded, BoomMount::ThroughInsulated, BoomMount::None] {
            if ui.selectable_label(*m == opt, opt.label()).clicked() {
                *m = opt;
            }
        }
        if ui.selectable_label(matches!(m, BoomMount::Custom(_)), "Kullanıcı tanımlı (ör. bum üstünde)").clicked() && !matches!(m, BoomMount::Custom(_)) {
            *m = BoomMount::Custom(0.0);
        }
    });
    if let BoomMount::Custom(v) = m {
        ui.horizontal(|ui| {
            ui.label("  ek boy");
            mm_value(ui, v, 0.05, -0.1..=0.1);
        });
    }
}

fn boom_section(ui: &mut egui::Ui, d: &mut YagiDesign) {
    let f = d.frequency;
    let lambda = C0 / f;
    section(ui, "Bum ve bum düzeltmesi", true, |ui| {
        egui::Grid::new("boom").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(if d.boom.square { "Kenar" } else { "Dış çap" });
            ui.horizontal(|ui| {
                mm_value(ui, &mut d.boom.diameter, 0.2, 0.005..=0.3);
                ui.checkbox(&mut d.boom.square, "kare profil");
                ui.label(RichText::new(format!("{:.4} λ", d.boom.diameter / lambda)).small().color(theme::TEXT_DIM));
            });
            ui.end_row();
            ui.label("Et kalınlığı");
            mm_value(ui, &mut d.boom.wall, 0.05, 0.0005..=0.02);
            ui.end_row();
            ui.label("Malzeme");
            material_combo(ui, "boom_mat", &mut d.boom.material);
            ui.end_row();
            ui.label("Parazitik elemanlar");
            ui.vertical(|ui| mount_combo(ui, "mount_par", &mut d.boom.mount));
            ui.end_row();
            ui.label("Sürülen eleman");
            ui.vertical(|ui| mount_combo(ui, "mount_drv", &mut d.boom.driven_mount));
            ui.end_row();
            ui.label("Uç taşması (her uç)");
            mm_value(ui, &mut d.boom.overhang, 1.0, 0.0..=2.0);
            ui.end_row();
            ui.label("Geçme payı");
            mm_value(ui, &mut d.boom.overlap, 1.0, 0.0..=0.3);
            ui.end_row();
        });
        let cp = d.boom.correction(d.boom.mount, f);
        let cd = d.boom.correction(d.boom.driven_mount, f);
        ui.label(RichText::new(format!("Düzeltme: parazitik +{:.2} mm, sürülen +{:.2} mm · bum boyu {:.0} mm", cp * MM, cd * MM, d.boom_total_length() * MM)).strong());
        if !d.boom.in_range(f) {
            ui.label(RichText::new(format!("Uyarı: bum çapı {:.3} λ, DL6WU ölçümlerinin geçerli olduğu 0.055 λ'yı aşıyor.", d.boom.diameter / lambda)).color(theme::BAD));
        }
        ui.label(
            RichText::new(
                "Metal bum, içinden geçen elemanı elektriksel olarak kısaltır. Formül: G3SEK'in DL6WU ölçümlerine uydurduğu C = 25.195·B − 229·B² (B: bum çapı/λ, C: bum çapının kesri), yalıtılmış geçişte yarısı. Tasarımdaki boylar elektriksel boydur; kesim boyu düzeltme eklenmiş halidir. Bum üstüne yalıtkanla monte elemanlar için yayımlanmış formül yok; düzeltme küçüktür ve yükseklikle azalır — kullanıcı tanımlı değer girin.",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    });
}

/// Kesim listesini düz metin olarak üretir (panoya kopyalamak için).
pub fn cut_list_text(d: &YagiDesign) -> String {
    let mut s = format!("{} — kesim listesi ({:.3} MHz)\n", d.name, d.frequency / 1e6);
    s += &format!("Bum: Ø{:.1} mm, boy {:.0} mm\n\n", d.boom.diameter * MM, d.boom_total_length() * MM);
    for (i, e) in d.elements.iter().enumerate() {
        s += &format!("{}  (konum {:.1} mm, kesim boyu {:.1} mm)\n", e.name, e.x * MM, d.cut_length(i) * MM);
        for p in d.cut_list(i) {
            s += &format!("    {} × {:>8.1} mm  Ø{:.1}{}  {}\n", p.count, p.length * MM, p.tube.diameter * MM, p.tube.wall.map(|w| format!("×{:.1}", w * MM)).unwrap_or_else(|| " dolu".into()), p.label);
        }
    }
    s
}

fn cut_list_section(ui: &mut egui::Ui, d: &YagiDesign) {
    section(ui, "Kesim listesi", false, |ui| {
        // Boru türüne göre toplam boy (alışveriş listesi).
        let mut totals: Vec<(String, f64)> = Vec::new();
        egui::Grid::new("cuts").num_columns(4).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
            for h in ["Eleman / parça", "Adet", "Kesim", "Boru"] {
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
                    let tube = format!("Ø{:.1}{}", p.tube.diameter * MM, p.tube.wall.map(|w| format!(" × {:.1}", w * MM)).unwrap_or_else(|| " dolu".into()));
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
        ui.label(RichText::new("Toplam boru ihtiyacı (fire hariç)").strong());
        for (t, l) in &totals {
            ui.label(format!("  {t}: {:.2} m", l));
        }
        ui.label(format!("  Bum Ø{:.1} × {:.1}: {:.2} m", d.boom.diameter * MM, d.boom.wall * MM, d.boom_total_length()));
        if ui.button("📋 Kesim listesini panoya kopyala").clicked() {
            ui.ctx().copy_text(cut_list_text(d));
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
                    let (bm, ba) = d.boom.mass_and_area(d.boom_total_length());
                    total_mass += bm;
                    total_area += ba;
                    ui.label(RichText::new("Bum").strong());
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
        ui.label(RichText::new(format!("Toplam eleman kütlesi: {:.2} kg · Rüzgâr alanı: {:.3} m² · Kuvvet: {:.0} N", total_mass, total_area, wind_force(total_area, v))).strong());
        ui.label(RichText::new("Sarkma: ortadan tutulan elemanın kendi ağırlığıyla ucunun inişi (kademeli kiriş). Bum ve kelepçeler dahil değildir.").small().color(theme::TEXT_DIM));
    });
}
