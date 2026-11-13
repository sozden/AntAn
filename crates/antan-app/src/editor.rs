//! Sol panel: eleman listesi. Sağ panel: seçili elemanın özellikleri.

use eframe::egui::{self, RichText};

use antan_core::geometry::Wire;
use antan_core::{Antenna, Complex64, Material};

use crate::app::AntAnApp;

pub fn explorer(app: &mut AntAnApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(RichText::new("Elemanlar").strong());
    ui.separator();
    let feed_wire = app.antenna.sources.first().map(|s| s.wire);
    let mut remove: Option<usize> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for i in 0..app.antenna.wires.len() {
            let name = app.antenna.wires[i].name.clone();
            let label = if Some(i) == feed_wire { format!("📡 {name}") } else { name };
            ui.horizontal(|ui| {
                if ui.selectable_label(app.selected == i, label).clicked() {
                    app.selected = i;
                }
                if app.antenna.wires.len() > 1 && ui.small_button("✕").on_hover_text("Elemanı kaldır").clicked() {
                    remove = Some(i);
                }
            });
        }
    });
    if let Some(i) = remove {
        app.antenna.wires.remove(i);
        app.antenna.sources.retain_mut(|s| {
            if s.wire == i {
                return false;
            }
            if s.wire > i {
                s.wire -= 1;
            }
            true
        });
        if app.antenna.sources.is_empty() {
            app.antenna.sources.push(antan_core::geometry::Source { wire: 0, position: 0.5, voltage: Complex64::new(1.0, 0.0) });
        }
        app.selected = app.selected.min(app.antenna.wires.len() - 1);
        app.dirty = true;
    }
    ui.separator();
    if ui.button("+ Eleman ekle").clicked() {
        let lambda = antan_core::C0 / app.antenna.frequency;
        let (lo, hi) = app.antenna.bounds();
        let x = hi[0] + 0.2 * lambda;
        let tube = app.default_tube.clone();
        app.antenna.wires.push(Wire {
            name: format!("Eleman {}", app.antenna.wires.len() + 1),
            start: [x, -0.22 * lambda, 0.0],
            end: [x, 0.22 * lambda, 0.0],
            diameter: tube.0,
            wall: tube.1,
            material: tube.2,
            segments: 0,
        });
        let _ = lo;
        app.selected = app.antenna.wires.len() - 1;
        app.dirty = true;
    }
}

pub fn properties(app: &mut AntAnApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(RichText::new("Anten").strong());
    ui.separator();
    egui::Grid::new("ant_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Ad");
        if ui.text_edit_singleline(&mut app.antenna.name).changed() {
            app.dirty = true;
        }
        ui.end_row();
        ui.label("Frekans (MHz)");
        let mut mhz = app.antenna.frequency / 1.0e6;
        if ui.add(egui::DragValue::new(&mut mhz).range(0.1..=30_000.0).suffix(" MHz")).changed() {
            app.antenna.frequency = mhz * 1.0e6;
            app.dirty = true;
        }
        ui.end_row();
        ui.label("Referans empedans");
        if ui.add(egui::DragValue::new(&mut app.z0).range(1.0..=1000.0).suffix(" Ω")).changed() {
            app.dirty = true;
        }
        ui.end_row();
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Seçili eleman").strong());
    ui.separator();
    if app.antenna.wires.is_empty() {
        ui.weak("Eleman yok.");
        return;
    }
    let sel = app.selected.min(app.antenna.wires.len() - 1);
    let lambda = antan_core::C0 / app.antenna.frequency;
    let is_feed = app.antenna.sources.first().map(|s| s.wire) == Some(sel);
    let mut changed = false;
    {
        let w = &mut app.antenna.wires[sel];
        egui::Grid::new("wire_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Ad");
            changed |= ui.text_edit_singleline(&mut w.name).changed();
            ui.end_row();

            ui.label("Bum konumu (x)");
            let mut x = w.start[0];
            let mut x_wl = x / lambda;
            if ui.add(egui::DragValue::new(&mut x_wl).speed(0.002).suffix(" λ")).changed() {
                x = x_wl * lambda;
                w.start[0] = x;
                w.end[0] = x;
                changed = true;
            }
            ui.end_row();

            ui.label("Uzunluk");
            let len = ((w.end[1] - w.start[1]).powi(2) + (w.end[2] - w.start[2]).powi(2)).sqrt();
            let mut len_wl = len / lambda;
            if ui.add(egui::DragValue::new(&mut len_wl).speed(0.001).range(0.001..=5.0).suffix(" λ")).changed() {
                let half = 0.5 * len_wl * lambda;
                w.start[1] = -half;
                w.end[1] = half;
                changed = true;
            }
            ui.label(format!("({:.1} mm)", len_wl * lambda * 1000.0));
            ui.end_row();

            ui.label("Dış çap");
            let mut d_mm = w.diameter * 1000.0;
            if ui.add(egui::DragValue::new(&mut d_mm).speed(0.1).range(0.1..=200.0).suffix(" mm")).changed() {
                w.diameter = d_mm / 1000.0;
                changed = true;
            }
            ui.end_row();

            ui.label("Yapı");
            let mut hollow = w.wall.is_some();
            ui.horizontal(|ui| {
                if ui.radio(!hollow, "İçi dolu çubuk").clicked() {
                    hollow = false;
                    changed = true;
                }
                if ui.radio(hollow, "Boru").clicked() {
                    hollow = true;
                    changed = true;
                }
            });
            w.wall = if hollow { Some(w.wall.unwrap_or(0.001).min(w.diameter * 0.5)) } else { None };
            ui.end_row();

            if let Some(wall) = &mut w.wall {
                ui.label("Et kalınlığı");
                let mut t_mm = *wall * 1000.0;
                if ui.add(egui::DragValue::new(&mut t_mm).speed(0.05).range(0.1..=50.0).suffix(" mm")).changed() {
                    *wall = (t_mm / 1000.0).min(w.diameter * 0.5);
                    changed = true;
                }
                ui.end_row();
            }

            ui.label("Malzeme");
            egui::ComboBox::from_id_salt("material").selected_text(w.material.name.clone()).show_ui(ui, |ui| {
                for m in Material::presets() {
                    if ui.selectable_label(m.name == w.material.name, &m.name).clicked() {
                        w.material = m;
                        changed = true;
                    }
                }
            });
            ui.end_row();

            ui.label("Parça sayısı");
            let mut auto = w.segments == 0;
            if ui.checkbox(&mut auto, "Otomatik").changed() {
                w.segments = if auto { 0 } else { w.segment_count(lambda) };
                changed = true;
            }
            if !auto {
                let mut n = w.segments.max(4);
                if ui.add(egui::DragValue::new(&mut n).range(4..=200)).changed() {
                    w.segments = n + n % 2;
                    changed = true;
                }
            }
            ui.end_row();
        });
    }
    if changed {
        app.dirty = true;
    }

    ui.add_space(6.0);
    if is_feed {
        ui.label(RichText::new("📡 Besleme bu elemanda (orta nokta)").italics());
    } else if ui.button("Beslemeyi buraya taşı").clicked() {
        app.antenna.sources = vec![antan_core::geometry::Source { wire: sel, position: 0.5, voltage: Complex64::new(1.0, 0.0) }];
        app.dirty = true;
    }
}

pub fn status_line(ant: &Antenna) -> String {
    format!("{} — {:.3} MHz, {} eleman", ant.name, ant.frequency / 1.0e6, ant.wires.len())
}
