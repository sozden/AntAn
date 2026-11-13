//! Anten modeli: düz teller (boru/çubuk) ve besleme kaynakları.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::material::Material;

pub type Vec3 = [f64; 3];

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// Düz bir iletken: içi dolu çubuk ya da boru.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub name: String,
    pub start: Vec3,
    pub end: Vec3,
    /// Dış çap (m).
    pub diameter: f64,
    /// Et kalınlığı (m). `None` = içi dolu çubuk.
    pub wall: Option<f64>,
    pub material: Material,
    /// Parça (segment) sayısı. `0` = frekansa göre otomatik.
    pub segments: usize,
}

impl Wire {
    pub fn length(&self) -> f64 {
        norm(sub(self.end, self.start))
    }
    pub fn radius(&self) -> f64 {
        self.diameter * 0.5
    }
    pub fn direction(&self) -> Vec3 {
        let d = sub(self.end, self.start);
        scale(d, 1.0 / norm(d))
    }
    pub fn midpoint(&self) -> Vec3 {
        scale(add(self.start, self.end), 0.5)
    }
    /// İletken kesit alanı (m²).
    pub fn cross_section(&self) -> f64 {
        let a = self.radius();
        let inner = self.wall.map(|t| (a - t).max(0.0)).unwrap_or(0.0);
        std::f64::consts::PI * (a * a - inner * inner)
    }
    /// Çözücünün kullanacağı parça sayısı: en az 4, her zaman çift
    /// (ortadan beslenebilmesi için ortada bir düğüm bulunur).
    pub fn segment_count(&self, wavelength: f64) -> usize {
        let n = if self.segments > 0 {
            self.segments
        } else {
            (self.length() / (wavelength / 40.0)).ceil() as usize
        };
        let n = n.max(4);
        n + n % 2
    }
}

/// Gerilim kaynağı (delta-gap besleme).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `Antenna::wires` içindeki tel sırası.
    pub wire: usize,
    /// Tel boyunca konum, 0..1 (0.5 = orta).
    pub position: f64,
    pub voltage: Complex64,
}

/// Zemin modeli. Zemin z = 0 düzlemidir; teller z ≥ 0'da olmalıdır.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Ground {
    /// Serbest uzay (zemin yok).
    #[default]
    FreeSpace,
    /// Kusursuz iletken zemin (görüntü yöntemi).
    Perfect,
    /// Gerçek zemin: akımlar kusursuz zeminle, uzak alan Fresnel yansıma
    /// katsayılarıyla hesaplanır (MININEC/MMANA yaklaşımı).
    Real {
        /// Bağıl dielektrik sabiti εr.
        permittivity: f64,
        /// İletkenlik σ (S/m).
        conductivity: f64,
    },
}

impl Ground {
    pub fn is_free(&self) -> bool {
        matches!(self, Ground::FreeSpace)
    }
    /// Gerçek zemin hazır değerleri (ad, εr, σ).
    pub const PRESETS: [(&'static str, f64, f64); 6] = [
        ("Deniz suyu", 81.0, 5.0),
        ("Tatlı su", 80.0, 0.001),
        ("Çok iyi toprak (verimli ova)", 20.0, 0.0303),
        ("Ortalama toprak", 13.0, 0.005),
        ("Zayıf toprak (kayalık, kumlu)", 5.0, 0.001),
        ("Şehir / sanayi bölgesi", 5.0, 0.001),
    ];
    pub fn label(&self) -> String {
        match self {
            Ground::FreeSpace => "Serbest uzay".into(),
            Ground::Perfect => "Kusursuz zemin".into(),
            Ground::Real { permittivity, conductivity } => format!("Gerçek zemin (εr {permittivity:.0}, σ {conductivity} S/m)"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Antenna {
    pub name: String,
    /// Tasarım frekansı (Hz).
    pub frequency: f64,
    pub wires: Vec<Wire>,
    pub sources: Vec<Source>,
    #[serde(default)]
    pub ground: Ground,
}

impl Antenna {
    pub fn new(name: impl Into<String>, frequency: f64, wires: Vec<Wire>, sources: Vec<Source>) -> Self {
        Self { name: name.into(), frequency, wires, sources, ground: Ground::FreeSpace }
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("anten JSON'a çevrilemedi")
    }
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    /// Tüm tellerin sınır kutusu (min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for w in &self.wires {
            for p in [w.start, w.end] {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
        (lo, hi)
    }
}
