//! Proje dosyası (.antan, JSON): model + analiz ayarları.

use serde::{Deserialize, Serialize};
use std::path::Path;

use antan_core::Antenna;
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
    pub fn kind_label(&self) -> &'static str {
        match self {
            Model::Yagi(_) => "Yagi tasarımı",
            Model::Wires(_) => "Tel modeli",
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
}

impl Default for Project {
    fn default() -> Self {
        Self::yagi(144.3e6, 3)
    }
}

pub fn default_tube() -> Tube {
    Tube::aluminum(0.010, Some(0.001))
}

impl Project {
    pub fn with_model(model: Model) -> Self {
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
        }
    }
    pub fn yagi(frequency: f64, n: usize) -> Self {
        Self::with_model(Model::Yagi(YagiDesign::standard(frequency, n, Profile::uniform(default_tube()))))
    }
    pub fn template(kind: templates::Kind, frequency: f64) -> Self {
        let tube = match kind {
            templates::Kind::Quad2 | templates::Kind::DeltaLoop | templates::Kind::Moxon => Tube::aluminum(0.004, None),
            templates::Kind::GroundPlane => Tube::aluminum(0.008, Some(0.001)),
            _ => default_tube(),
        };
        let mut p = Self::with_model(Model::Wires(kind.build(frequency, &tube)));
        if matches!(kind, templates::Kind::Quad2 | templates::Kind::DeltaLoop) {
            p.z0 = 75.0;
        }
        if matches!(kind, templates::Kind::FoldedDipole) {
            p.z0 = 300.0;
        }
        p
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("Dosya okunamadı: {e}"))
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
        let mut list = vec![Project::yagi(144.3e6, 5)];
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
