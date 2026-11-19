//! Proje dosyası (.antan, JSON): model + analiz ayarları.

use serde::{Deserialize, Serialize};
use antan_core::i18n::t as tl;
use antan_core::tf;

use std::path::Path;

use antan_core::Antenna;
use antan_core::magloop::MagLoop;
use antan_core::optimize::Goals;
use antan_core::templates::{self, Tube};
use antan_core::yagi::{Profile, YagiDesign};

pub const EXT: &str = "antan";

/// Düzenlenen model: eleman bazlı Yagi tasarımı ya da serbest tel modeli.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Model {
    Yagi(YagiDesign),
    Wires(Antenna),
}

impl Model {
    pub fn name(&self) -> &str {
        match self {
            Model::Yagi(d) => &d.name,
            Model::Wires(a) => &a.name,
        }
    }
    pub fn name_mut(&mut self) -> &mut String {
        match self {
            Model::Yagi(d) => &mut d.name,
            Model::Wires(a) => &mut a.name,
        }
    }
    pub fn frequency(&self) -> f64 {
        match self {
            Model::Yagi(d) => d.frequency,
            Model::Wires(a) => a.frequency,
        }
    }
    pub fn set_frequency(&mut self, f: f64) {
        match self {
            Model::Yagi(d) => d.frequency = f,
            Model::Wires(a) => a.frequency = f,
        }
    }
    pub fn ground(&self) -> antan_core::geometry::Ground {
        match self {
            Model::Yagi(d) => d.ground,
            Model::Wires(a) => a.ground,
        }
    }
    pub fn ground_mut(&mut self) -> &mut antan_core::geometry::Ground {
        match self {
            Model::Yagi(d) => &mut d.ground,
            Model::Wires(a) => &mut a.ground,
        }
    }
    /// Geometriyi `k` ile ölçekler (boru çapları değişmez).
    pub fn scale(&mut self, k: f64) {
        match self {
            Model::Yagi(d) => {
                d.scale(k);
                d.height *= k;
            }
            Model::Wires(a) => {
                for w in &mut a.wires {
                    for i in 0..3 {
                        w.start[i] *= k;
                        w.end[i] *= k;
                    }
                }
            }
        }
    }
    /// En büyük boyut (m), sınır kutusundan.
    pub fn largest_dimension(&self) -> f64 {
        let a = match self {
            Model::Yagi(d) => d.build(),
            Model::Wires(a) => a.clone(),
        };
        let (lo, hi) = a.bounds();
        (0..3).map(|i| hi[i] - lo[i]).filter(|v| v.is_finite()).fold(0.0, f64::max)
    }
    pub fn kind_label(&self) -> &'static str {
        match self {
            Model::Yagi(_) => tl("Yagi design", "Yagi tasarımı"),
            Model::Wires(_) => tl("Wire model", "Tel modeli"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub model: Model,
    pub z0: f64,
    /// Tarama genişliği: merkez frekansın ±%'si.
    pub sweep_span_pct: f64,
    pub sweep_points: usize,
    pub goals: Goals,
    /// Rüzgâr hızı (m/s), mekanik yük hesabı için.
    pub wind_speed: f64,
    /// Hairpin/gamma çubuk çapı ve aralığı (m).
    pub match_rod: f64,
    pub match_spacing: f64,
    pub notes: String,
    /// Besleme noktasına takılı hairpin endüktansı (H); 0 = yok. Takılıyken SWR,
    /// empedans ve tarama hat tarafından (hairpin dahil) gösterilir.
    pub hairpin_l: f64,
    /// Geometrinin tasarlandığı (ya da en son ölçeklendiği) frekans (Hz); 0 = bilinmiyor.
    pub geom_freq: f64,
    /// İstif düzeni (1×1 = istif yok).
    pub stack: antan_core::stack::Stack,
    /// Manyetik loop tasarımı; varsa tel modeli bundan üretilir.
    pub magloop: Option<MagLoop>,
    /// Kondansatörün en son ayarlandığı frekans (Hz).
    pub loop_tuned_f: f64,
    /// Verici gücü (W): loop'ta kondansatör gerilimi ve akım için.
    pub tx_power: f64,
}

impl Default for Project {
    fn default() -> Self {
        Self::yagi(144.3e6, 3)
    }
}

/// Antene paralel `l` henrilik endüktans (hairpin) takılıyken giriş empedansı.
pub fn with_hairpin(z: antan_core::Complex64, f: f64, l: f64) -> antan_core::Complex64 {
    if l <= 0.0 || z.norm() == 0.0 {
        return z;
    }
    let jxl = antan_core::Complex64::new(0.0, 2.0 * std::f64::consts::PI * f * l);
    z * jxl / (z + jxl)
}

pub fn default_tube() -> Tube {
    Tube::aluminum(0.010, Some(0.001))
}

impl Project {
    pub fn with_model(model: Model) -> Self {
        let geom_freq = model.frequency();
        let mut p = Self::bare(model);
        p.geom_freq = geom_freq;
        p
    }
    fn bare(model: Model) -> Self {
        Self {
            version: 1,
            model,
            z0: 50.0,
            sweep_span_pct: 3.0,
            sweep_points: 41,
            goals: Goals::default(),
            wind_speed: 30.0,
            match_rod: 0.004,
            match_spacing: 0.03,
            notes: String::new(),
            geom_freq: 0.0,
            hairpin_l: 0.0,
            stack: Default::default(),
            magloop: None,
            loop_tuned_f: 0.0,
            tx_power: 100.0,
        }
    }
    /// Standart Yagi: boylar boru kalınlığına göre düzeltilmiş, sürülen eleman rezonansta (X = 0).
    pub fn yagi(frequency: f64, n: usize) -> Self {
        let d = YagiDesign::standard(frequency, n, Profile::uniform(default_tube()));
        let d = d.tune_driven(|_| Some(0.0)).map(|(t, _)| t).unwrap_or(d);
        Self::with_model(Model::Yagi(d))
    }
    /// Geometrinin tasarım frekansı: kayıtlıysa o, yoksa Yagi için en uzun
    /// elemandan (≈0.495 λ) tahmin; tel modelinde bilinmiyorsa `None`.
    pub fn design_frequency(&self) -> Option<f64> {
        if self.geom_freq > 0.0 {
            return Some(self.geom_freq);
        }
        match &self.model {
            Model::Yagi(d) => d.elements.iter().map(|e| e.length).reduce(f64::max).map(|l| 0.495 * antan_core::C0 / l),
            Model::Wires(_) => None,
        }
    }
    /// Geometriyi, tasarım frekansından şimdiki frekansa ölçekler.
    pub fn scale_to_frequency(&mut self) -> bool {
        let f = self.model.frequency();
        let Some(g) = self.design_frequency() else { return false };
        self.model.scale(g / f);
        // Boru çapları ölçeklenmez: eleman boylarını kalınlık farkına göre düzelt.
        if let Model::Yagi(d) = &mut self.model {
            d.rethickness(g, f);
        }
        self.geom_freq = f;
        true
    }
    /// Bant değişiminde: frekansı `f` yapar, `scale` ise geometriyi de oranla ölçekler.
    pub fn change_frequency(&mut self, f: f64, scale: bool) {
        let old = self.model.frequency();
        self.model.set_frequency(f);
        // Manyetik loop: ölçeklenirse halka boyutları değişir; her durumda kondansatör yeniden ayarlanır.
        if let Some(m) = &mut self.magloop {
            if scale {
                m.diameter *= old / f;
                m.coupling_gap *= old / f;
            }
            self.retune_loop();
            return;
        }
        if scale {
            self.model.scale(old / f);
            if let Model::Yagi(d) = &mut self.model {
                d.rethickness(old, f);
            }
            if self.geom_freq > 0.0 {
                self.geom_freq *= f / old;
            }
        }
    }
    /// Manyetik loop: çevresi ≈ 0.1 λ (0.3–3 m çap), 22 mm bakır boru; kuplaj
    /// halkası 50 Ω'a eşlenmiş, kondansatör frekansa ayarlı. Tarama dar (yüksek Q).
    pub fn magloop(frequency: f64) -> Self {
        let lambda = antan_core::C0 / frequency;
        let mut m = MagLoop { diameter: (0.1 * lambda / std::f64::consts::PI).clamp(0.3, 3.0), ..Default::default() };
        if let Some((mm, _)) = m.match_coupling(frequency, 50.0, Default::default()) {
            m = mm;
        }
        let mut p = Self::with_model(Model::Wires(m.build(frequency)));
        p.magloop = Some(m);
        p.loop_tuned_f = frequency;
        p.sweep_span_pct = 0.06;
        p.sweep_points = 81;
        p
    }

    /// Loop tasarımından tel modelini yeniden üretir; ad, zemin ve yükseklik korunur.
    pub fn rebuild_loop(&mut self) {
        let Some(m) = &mut self.magloop else { return };
        let f = self.model.frequency();
        if let Model::Wires(a) = &self.model {
            let lo = a.wires.iter().flat_map(|w| [w.start[2], w.end[2]]).fold(f64::INFINITY, f64::min);
            if lo.is_finite() {
                m.bottom = lo;
            }
            let mut b = m.build(f);
            b.name = a.name.clone();
            b.ground = a.ground;
            self.model = Model::Wires(b);
        }
    }

    /// Kondansatörü şimdiki frekansa ayarlar (en düşük SWR). Başarısızsa `false`.
    pub fn retune_loop(&mut self) -> bool {
        let f = self.model.frequency();
        let (z0, ground) = (self.z0, self.model.ground());
        self.loop_tuned_f = f;
        let ok = match &mut self.magloop {
            Some(m) => match m.two_port(f, ground).ok().and_then(|tp| m.tune(&tp, z0)) {
                Some(c) => {
                    m.capacitance = c;
                    true
                }
                None => false,
            },
            None => return false,
        };
        self.rebuild_loop();
        ok
    }

    /// Kuplaj halkasının boyunu ve açıklığını en düşük SWR için ayarlar; SWR'yi döner.
    pub fn match_loop(&mut self) -> Option<f64> {
        let f = self.model.frequency();
        let (z0, ground) = (self.z0, self.model.ground());
        let (m, swr) = self.magloop.as_ref()?.match_coupling(f, z0, ground)?;
        self.magloop = Some(m);
        self.loop_tuned_f = f;
        self.rebuild_loop();
        Some(swr)
    }

    pub fn lfa(frequency: f64, n: usize) -> Self {
        Self::with_model(Model::Yagi(YagiDesign::lfa(frequency, n, Profile::uniform(default_tube()))))
    }
    pub fn template(kind: templates::Kind, frequency: f64) -> Self {
        let tube = match kind {
            templates::Kind::Quad2 | templates::Kind::DeltaLoop | templates::Kind::Moxon => Tube::aluminum(0.004, None),
            templates::Kind::GroundPlane => Tube::aluminum(0.008, Some(0.001)),
            templates::Kind::TrapDipole | templates::Kind::LoadedDipole => Tube { diameter: 0.002, wall: None, material: antan_core::Material::copper() },
            _ => default_tube(),
        };
        let mut p = Self::with_model(Model::Wires(kind.build(frequency, &tube)));
        if matches!(kind, templates::Kind::Quad2 | templates::Kind::DeltaLoop) {
            p.z0 = 75.0;
        }
        if matches!(kind, templates::Kind::FoldedDipole) {
            p.z0 = 300.0;
        }
        if matches!(kind, templates::Kind::TrapDipole) {
            // 80 m bandı (3.5–3.8 MHz); 40 m için bant düğmesi ya da frekans 7.1 MHz.
            p.sweep_span_pct = 4.5;
            p.z0 = 50.0;
        }
        p
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| tf!("Could not read file: {e}", "Dosya okunamadı: {e}"))
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_templates() {
        let mut list = vec![Project::yagi(144.3e6, 5), Project::lfa(144.3e6, 7), Project::magloop(7.1e6)];
        for k in templates::Kind::ALL {
            list.push(Project::template(k, 50.1e6));
        }
        // Kayıpsız malzeme (sonsuz iletkenlik) de korunmalı.
        let mut p = Project::yagi(28.5e6, 2);
        if let Model::Yagi(d) = &mut p.model {
            d.profile.tip.material = antan_core::Material::perfect();
        }
        list.push(p);
        for p in list {
            let s = serde_json::to_string(&p).unwrap();
            let back: Project = serde_json::from_str(&s).unwrap();
            assert_eq!(back, p);
        }
    }
}
