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
    TrapDipole,
    LoadedVertical,
    LoadedDipole,
}

impl Kind {
    pub const ALL: [Kind; 11] = [Kind::Dipole, Kind::InvertedV, Kind::FoldedDipole, Kind::LoadedDipole, Kind::Quad2, Kind::DeltaLoop, Kind::GroundPlane, Kind::Moxon, Kind::GroundVertical, Kind::TrapDipole, Kind::LoadedVertical];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Dipole => crate::i18n::t("Half-wave dipole", "Yarım dalga dipol"),
            Kind::InvertedV => crate::i18n::t("Inverted V dipole", "Ters V dipol"),
            Kind::FoldedDipole => crate::i18n::t("Folded dipole", "Katlanmış dipol"),
            Kind::Quad2 => crate::i18n::t("2-element quad", "2 elemanlı quad"),
            Kind::DeltaLoop => "Delta loop",
            Kind::GroundPlane => "Ground plane (GP)",
            Kind::Moxon => crate::i18n::t("Moxon rectangle", "Moxon dikdörtgeni"),
            Kind::GroundVertical => crate::i18n::t("Quarter-wave vertical (ground-mounted)", "Çeyrek dalga dikey (zemine bağlı)"),
            Kind::TrapDipole => crate::i18n::t("Trap dipole 40/80 m (W3DZZ style)", "Tuzaklı dipol 40/80 m (W3DZZ tarzı)"),
            Kind::LoadedVertical => crate::i18n::t("Coil-loaded short vertical (λ/8)", "Bobin yüklü kısa dikey (λ/8)"),
            Kind::LoadedDipole => crate::i18n::t("Coil-loaded short dipole (half size)", "Bobinle kısaltılmış dipol (yarı boy)"),
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
            Kind::TrapDipole => crate::loading::trap_dipole(tube),
            Kind::LoadedVertical => crate::loading::loaded_vertical(frequency, tube),
            Kind::LoadedDipole => crate::loading::loaded_dipole(frequency, 0.24, tube),
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
    Antenna::new(crate::i18n::t("Dipole", "Dipol"), frequency, vec![element("Dipol", 0.0, length_wl * lambda, tube)], vec![source(0, 0.5)])
}

/// Tepeden beslemeli ters V; `included_deg` iki kol arasındaki açı.
pub fn inverted_v(frequency: f64, included_deg: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let arm = 0.25 * lambda;
    let h = (0.5 * included_deg).to_radians();
    let tip = |s: f64| [0.0, s * arm * h.sin(), -arm * h.cos()];
    Antenna {
        name: crate::i18n::t("Inverted V", "Ters V").into(),
        frequency,
        wires: vec![wire(crate::i18n::t("Left arm", "Sol kol"), [0.0; 3], tip(-1.0), tube), wire(crate::i18n::t("Right arm", "Sağ kol"), [0.0; 3], tip(1.0), tube)],
        sources: vec![source(0, 0.0)],
        ground: Default::default(),
        loads: Vec::new(),
    }
}

/// Katlanmış dipol: iki paralel iletken, uçlardan birleşik; alt iletken ortadan beslenir.
pub fn folded_dipole(frequency: f64, spacing_wl: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let (l, s) = (0.48 * lambda, spacing_wl * lambda);
    let p = |y: f64, z: f64| [0.0, y * 0.5 * l, z * s];
    Antenna {
        name: crate::i18n::t("Folded dipole", "Katlanmış dipol").into(),
        frequency,
        wires: vec![
            wire(crate::i18n::t("Bottom (fed)", "Alt (beslenen)"), p(-1.0, 0.0), p(1.0, 0.0), tube),
            wire(crate::i18n::t("Right end", "Sağ uç"), p(1.0, 0.0), p(1.0, 1.0), tube),
            wire(crate::i18n::t("Top", "Üst"), p(1.0, 1.0), p(-1.0, 1.0), tube),
            wire(crate::i18n::t("Left end", "Sol uç"), p(-1.0, 1.0), p(-1.0, 0.0), tube),
        ],
        sources: vec![source(0, 0.5)],
        ground: Default::default(),
        loads: Vec::new(),
    }
}

/// YZ düzleminde, `x` konumunda, çevresi `perimeter` olan kare halka (4 tel).
fn square_loop(prefix: &str, x: f64, perimeter: f64, tube: &Tube) -> Vec<Wire> {
    let h = perimeter / 8.0;
    let c = |y: f64, z: f64| [x, y * h, z * h];
    vec![
        wire(&crate::tf!("{prefix} bottom", "{prefix} alt"), c(-1.0, -1.0), c(1.0, -1.0), tube),
        wire(&crate::tf!("{prefix} right", "{prefix} sağ"), c(1.0, -1.0), c(1.0, 1.0), tube),
        wire(&crate::tf!("{prefix} top", "{prefix} üst"), c(1.0, 1.0), c(-1.0, 1.0), tube),
        wire(&crate::tf!("{prefix} left", "{prefix} sol"), c(-1.0, 1.0), c(-1.0, -1.0), tube),
    ]
}

/// N elemanlı quad (yatay polarizasyon, alt kenardan besleme).
/// 2 elemanlı boyutlar bu çözücüyle Ö/A için taranmıştır (çevre 1.02λ / 1.12λ,
/// aralık 0.14λ); besleme empedansı ~120 Ω'dur (75 Ω hat veya eşleme gerekir).
pub fn quad(frequency: f64, elements: usize, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let n = elements.max(2);
    let mut wires = square_loop(crate::i18n::t("Reflector", "Yansıtıcı"), 0.0, 1.12 * lambda, tube);
    wires.extend(square_loop(crate::i18n::t("Driven", "Sürülen"), 0.14 * lambda, 1.02 * lambda, tube));
    for d in 0..n - 2 {
        wires.extend(square_loop(&crate::tf!("Director {}", "Yönlendirici {}", d + 1), (0.14 + 0.18 * (d + 1) as f64) * lambda, 0.97 * lambda, tube));
    }
    Antenna::new(crate::tf!("{n}-element quad", "{n} elemanlı quad"), frequency, wires, vec![source(4, 0.5)])
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
        wires: vec![wire(crate::i18n::t("Bottom", "Alt"), a, b, tube), wire(crate::i18n::t("Right", "Sağ"), b, c, tube), wire(crate::i18n::t("Left", "Sol"), c, a, tube)],
        sources: vec![source(0, 0.5)],
        ground: Default::default(),
        loads: Vec::new(),
    }
}

/// Dikey çeyrek dalga + 4 radyal, radyaller `droop_deg` kadar aşağı eğik. Tabandan besleme.
/// Dikey 0.228λ: 45° eğik radyallerle bu çözücüde rezonans (~55 Ω).
pub fn ground_plane(frequency: f64, droop_deg: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let v = 0.228 * lambda;
    let r = 0.25 * lambda;
    let d = droop_deg.to_radians();
    let mut wires = vec![wire(crate::i18n::t("Vertical", "Dikey"), [0.0; 3], [0.0, 0.0, v], tube)];
    for (i, (cx, cy)) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)].into_iter().enumerate() {
        wires.push(wire(&crate::tf!("Radial {}", "Radyal {}", i + 1), [0.0; 3], [cx * r * d.cos(), cy * r * d.cos(), -r * d.sin()], tube));
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
            wire(crate::i18n::t("Reflector", "Yansıtıcı"), p(0.0, -0.5 * a), p(0.0, 0.5 * a), tube),
            wire(crate::i18n::t("Reflector tail right", "Yansıtıcı kuyruk sağ"), p(0.0, 0.5 * a), p(d, 0.5 * a), tube),
            wire(crate::i18n::t("Reflector tail left", "Yansıtıcı kuyruk sol"), p(0.0, -0.5 * a), p(d, -0.5 * a), tube),
            wire(crate::i18n::t("Driven", "Sürülen"), p(e, -0.5 * a), p(e, 0.5 * a), tube),
            wire(crate::i18n::t("Driven tail right", "Sürülen kuyruk sağ"), p(e, 0.5 * a), p(e - b, 0.5 * a), tube),
            wire(crate::i18n::t("Driven tail left", "Sürülen kuyruk sol"), p(e, -0.5 * a), p(e - b, -0.5 * a), tube),
        ],
        sources: vec![source(3, 0.5)],
        ground: Default::default(),
        loads: Vec::new(),
    }
}

/// Zemine bağlı, tabandan beslenen çeyrek dalga dikey; ortalama toprak üzerinde.
/// Akımlar kusursuz zeminle hesaplandığından toprak kaybı empedansa girmez.
pub fn ground_vertical(frequency: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let mut a = Antenna::new(crate::i18n::t("Vertical (ground-mounted)", "Dikey (zemine bağlı)"), frequency, vec![wire(crate::i18n::t("Vertical", "Dikey"), [0.0; 3], [0.0, 0.0, 0.243 * lambda], tube)], vec![source(0, 0.0)]);
    a.ground = crate::geometry::Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    a
}

/// Serbest tel uçları arasındaki boşluklar (ör. Moxon'da sürülen ve yansıtıcı
/// kuyrukları arası). Her serbest uç için en yakın başka telin serbest ucu,
/// `max` mesafeden yakınsa ve iki tel de boşluk doğrultusunda uzanıyorsa
/// (uç uca bakan teller; Yagi eleman uçları gibi yan yana uçlar sayılmaz):
/// (tel a, tel b, uzaklık m). Çiftler tekrarsız.
pub fn open_end_gaps(a: &Antenna, max: f64) -> Vec<(usize, usize, f64)> {
    let tol = 1e-6_f64.max(1e-4 * a.wires.iter().map(|w| w.length()).fold(0.0, f64::max));
    let dist = |p: Vec3, q: Vec3| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
    let grounded = !a.ground.is_free();
    let mut ends = Vec::new();
    for (i, w) in a.wires.iter().enumerate() {
        for p in [w.start, w.end] {
            let joined = a.wires.iter().enumerate().any(|(j, o)| j != i && (dist(p, o.start) < tol || dist(p, o.end) < tol));
            let on_ground = grounded && p[2].abs() < tol;
            if !joined && !on_ground {
                ends.push((i, p));
            }
        }
    }
    let mut out: Vec<(usize, usize, f64)> = Vec::new();
    for &(i, p) in &ends {
        let best = ends.iter().filter(|(j, _)| *j != i).map(|&(j, q)| (j, q, dist(p, q))).min_by(|x, y| x.2.total_cmp(&y.2));
        if let Some((j, q, d)) = best {
            let g = [(q[0] - p[0]) / d.max(1e-12), (q[1] - p[1]) / d.max(1e-12), (q[2] - p[2]) / d.max(1e-12)];
            let along = |k: usize| {
                let u = a.wires[k].direction();
                (u[0] * g[0] + u[1] * g[1] + u[2] * g[2]).abs() > 0.9
            };
            if d < max && along(i) && along(j) && !out.iter().any(|&(a2, b2, d2)| ((a2 == i && b2 == j) || (a2 == j && b2 == i)) && (d2 - d).abs() < tol) {
                out.push((i, j, d));
            }
        }
    }
    out
}
