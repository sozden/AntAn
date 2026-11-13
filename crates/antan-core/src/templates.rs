//! Hazır tel modeli şablonları: dipol, ters V, katlanmış dipol, quad, GP, Moxon.
//! Yagi-Uda için eleman bazlı tasarım `yagi` modülündedir.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::C0;
use crate::geometry::{Antenna, Source, Vec3, Wire};
use crate::material::Material;

/// Eleman kesiti: dış çap, et kalınlığı (`None` = dolu), malzeme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tube {
    pub diameter: f64,
    pub wall: Option<f64>,
    pub material: Material,
}

impl Tube {
    pub fn aluminum(diameter: f64, wall: Option<f64>) -> Self {
        Self { diameter, wall, material: Material::aluminum_6063() }
    }
}

pub fn wire(name: &str, start: Vec3, end: Vec3, tube: &Tube) -> Wire {
    Wire { name: name.into(), start, end, diameter: tube.diameter, wall: tube.wall, material: tube.material.clone(), segments: 0 }
}

/// Y eksenine paralel, `x` konumunda, ortası bum üzerinde bir eleman.
pub fn element(name: &str, x: f64, length: f64, tube: &Tube) -> Wire {
    wire(name, [x, -0.5 * length, 0.0], [x, 0.5 * length, 0.0], tube)
}

pub fn source(wire: usize, position: f64) -> Source {
    Source { wire, position, voltage: Complex64::new(1.0, 0.0) }
}

/// Şablon türleri (arayüzde listelenir).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dipole,
    InvertedV,
    FoldedDipole,
    Quad2,
    DeltaLoop,
    GroundPlane,
    Moxon,
    GroundVertical,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Dipole, Kind::InvertedV, Kind::FoldedDipole, Kind::Quad2, Kind::DeltaLoop, Kind::GroundPlane, Kind::Moxon, Kind::GroundVertical];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Dipole => "Yarım dalga dipol",
            Kind::InvertedV => "Ters V dipol",
            Kind::FoldedDipole => "Katlanmış dipol",
            Kind::Quad2 => "2 elemanlı quad",
            Kind::DeltaLoop => "Delta loop",
            Kind::GroundPlane => "Ground plane (GP)",
            Kind::Moxon => "Moxon dikdörtgeni",
            Kind::GroundVertical => "Çeyrek dalga dikey (zemine bağlı)",
        }
    }
    pub fn build(self, frequency: f64, tube: &Tube) -> Antenna {
        match self {
            Kind::Dipole => dipole(frequency, 0.485, tube),
            Kind::InvertedV => inverted_v(frequency, 110.0, tube),
            Kind::FoldedDipole => folded_dipole(frequency, 0.02, tube),
            Kind::Quad2 => quad(frequency, 2, tube),
            Kind::DeltaLoop => delta_loop(frequency, tube),
            Kind::GroundPlane => ground_plane(frequency, 45.0, tube),
            Kind::Moxon => moxon(frequency, tube),
            Kind::GroundVertical => ground_vertical(frequency, tube),
        }
    }
}

/// Tekdüze borulu standart Yagi'nin tel modeli (kısayol).
pub fn yagi(frequency: f64, elements: usize, tube: &Tube) -> Antenna {
    crate::yagi::YagiDesign::standard(frequency, elements, crate::yagi::Profile::uniform(tube.clone())).build()
}

/// Ortadan beslemeli dipol; `length_wl` dalga boyu cinsinden boy.
pub fn dipole(frequency: f64, length_wl: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    Antenna::new("Dipol", frequency, vec![element("Dipol", 0.0, length_wl * lambda, tube)], vec![source(0, 0.5)])
}

/// Tepeden beslemeli ters V; `included_deg` iki kol arasındaki açı.
pub fn inverted_v(frequency: f64, included_deg: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let arm = 0.25 * lambda;
    let h = (0.5 * included_deg).to_radians();
    let tip = |s: f64| [0.0, s * arm * h.sin(), -arm * h.cos()];
    Antenna {
        name: "Ters V".into(),
        frequency,
        wires: vec![wire("Sol kol", [0.0; 3], tip(-1.0), tube), wire("Sağ kol", [0.0; 3], tip(1.0), tube)],
        sources: vec![source(0, 0.0)],
        ground: Default::default(),
    }
}

/// Katlanmış dipol: iki paralel iletken, uçlardan birleşik; alt iletken ortadan beslenir.
pub fn folded_dipole(frequency: f64, spacing_wl: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let (l, s) = (0.48 * lambda, spacing_wl * lambda);
    let p = |y: f64, z: f64| [0.0, y * 0.5 * l, z * s];
    Antenna {
        name: "Katlanmış dipol".into(),
        frequency,
        wires: vec![
            wire("Alt (beslenen)", p(-1.0, 0.0), p(1.0, 0.0), tube),
            wire("Sağ uç", p(1.0, 0.0), p(1.0, 1.0), tube),
            wire("Üst", p(1.0, 1.0), p(-1.0, 1.0), tube),
            wire("Sol uç", p(-1.0, 1.0), p(-1.0, 0.0), tube),
        ],
        sources: vec![source(0, 0.5)],
        ground: Default::default(),
    }
}

/// YZ düzleminde, `x` konumunda, çevresi `perimeter` olan kare halka (4 tel).
fn square_loop(prefix: &str, x: f64, perimeter: f64, tube: &Tube) -> Vec<Wire> {
    let h = perimeter / 8.0;
    let c = |y: f64, z: f64| [x, y * h, z * h];
    vec![
        wire(&format!("{prefix} alt"), c(-1.0, -1.0), c(1.0, -1.0), tube),
        wire(&format!("{prefix} sağ"), c(1.0, -1.0), c(1.0, 1.0), tube),
        wire(&format!("{prefix} üst"), c(1.0, 1.0), c(-1.0, 1.0), tube),
        wire(&format!("{prefix} sol"), c(-1.0, 1.0), c(-1.0, -1.0), tube),
    ]
}

/// N elemanlı quad (yatay polarizasyon, alt kenardan besleme).
/// 2 elemanlı boyutlar bu çözücüyle Ö/A için taranmıştır (çevre 1.02λ / 1.12λ,
/// aralık 0.14λ); besleme empedansı ~120 Ω'dur (75 Ω hat veya eşleme gerekir).
pub fn quad(frequency: f64, elements: usize, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let n = elements.max(2);
    let mut wires = square_loop("Yansıtıcı", 0.0, 1.12 * lambda, tube);
    wires.extend(square_loop("Sürülen", 0.14 * lambda, 1.02 * lambda, tube));
    for d in 0..n - 2 {
        wires.extend(square_loop(&format!("Yönlendirici {}", d + 1), (0.14 + 0.18 * (d + 1) as f64) * lambda, 0.97 * lambda, tube));
    }
    Antenna::new(format!("{n} elemanlı quad"), frequency, wires, vec![source(4, 0.5)])
}

/// Tepesi yukarıda eşkenar üçgen halka, alt kenar ortasından besleme (yatay
/// polarizasyon). Çevre 1.11λ: kalın borulu halka bu çözücüde orada rezonansta (~120 Ω).
pub fn delta_loop(frequency: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let side = 1.11 * lambda / 3.0;
    let h = side * 3f64.sqrt() / 2.0;
    let (a, b, c) = ([0.0, -0.5 * side, 0.0], [0.0, 0.5 * side, 0.0], [0.0, 0.0, h]);
    Antenna {
        name: "Delta loop".into(),
        frequency,
        wires: vec![wire("Alt", a, b, tube), wire("Sağ", b, c, tube), wire("Sol", c, a, tube)],
        sources: vec![source(0, 0.5)],
        ground: Default::default(),
    }
}

/// Dikey çeyrek dalga + 4 radyal, radyaller `droop_deg` kadar aşağı eğik. Tabandan besleme.
/// Dikey 0.228λ: 45° eğik radyallerle bu çözücüde rezonans (~55 Ω).
pub fn ground_plane(frequency: f64, droop_deg: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let v = 0.228 * lambda;
    let r = 0.25 * lambda;
    let d = droop_deg.to_radians();
    let mut wires = vec![wire("Dikey", [0.0; 3], [0.0, 0.0, v], tube)];
    for (i, (cx, cy)) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)].into_iter().enumerate() {
        wires.push(wire(&format!("Radyal {}", i + 1), [0.0; 3], [cx * r * d.cos(), cy * r * d.cos(), -r * d.sin()], tube));
    }
    Antenna::new("Ground plane", frequency, wires, vec![source(0, 0.0)])
}

/// Moxon dikdörtgeni boyutları (dalga boyu cinsinden), L. B. Cebik'in
/// MoxGen eğri uydurmasıyla: A genişlik, B sürülen kuyruk, C boşluk,
/// D yansıtıcı kuyruk, E derinlik.
pub fn moxon_dimensions(diameter_wl: f64) -> [f64; 5] {
    let d = diameter_wl.clamp(1e-6, 1e-2).log10();
    let a = -0.000_857_142_857_1 * d * d - 0.009_571_428_571 * d + 0.339_857_142_9;
    let b = -0.002_142_857_143 * d * d - 0.020_357_142_86 * d + 0.008_285_714_286;
    let c = 0.001_809_523_381 * d * d + 0.017_809_523_81 * d + 0.051_642_857_14;
    let dd = 0.001 * d + 0.071_785_714_29;
    [a, b, c, dd, b + c + dd]
}

/// Moxon; Cebik boyutları bu çözücüde ~%1.2 yüksek frekansa düştüğü için
/// 1.012 ile ölçeklenir (en iyi Ö/A ve ~50 Ω tasarım frekansında).
pub fn moxon(frequency: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let [a, b, _c, d, e] = moxon_dimensions(tube.diameter / lambda).map(|v| v * lambda * 1.012);
    let p = |x: f64, y: f64| [x, y, 0.0];
    Antenna {
        name: "Moxon".into(),
        frequency,
        wires: vec![
            wire("Yansıtıcı", p(0.0, -0.5 * a), p(0.0, 0.5 * a), tube),
            wire("Yansıtıcı kuyruk sağ", p(0.0, 0.5 * a), p(d, 0.5 * a), tube),
            wire("Yansıtıcı kuyruk sol", p(0.0, -0.5 * a), p(d, -0.5 * a), tube),
            wire("Sürülen", p(e, -0.5 * a), p(e, 0.5 * a), tube),
            wire("Sürülen kuyruk sağ", p(e, 0.5 * a), p(e - b, 0.5 * a), tube),
            wire("Sürülen kuyruk sol", p(e, -0.5 * a), p(e - b, -0.5 * a), tube),
        ],
        sources: vec![source(3, 0.5)],
        ground: Default::default(),
    }
}

/// Zemine bağlı, tabandan beslenen çeyrek dalga dikey; ortalama toprak üzerinde.
/// Akımlar kusursuz zeminle hesaplandığından toprak kaybı empedansa girmez.
pub fn ground_vertical(frequency: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let mut a = Antenna::new("Dikey (zemine bağlı)", frequency, vec![wire("Dikey", [0.0; 3], [0.0, 0.0, 0.243 * lambda], tube)], vec![source(0, 0.0)]);
    a.ground = crate::geometry::Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    a
}
