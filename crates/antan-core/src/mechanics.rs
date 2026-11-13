//! Elemanların mekanik özellikleri: kütle, sarkma, rüzgâr yükü.

use crate::geometry::Wire;

/// Yerçekimi ivmesi (m/s²).
pub const G: f64 = 9.806_65;
/// Hava yoğunluğu (kg/m³).
pub const AIR_DENSITY: f64 = 1.225;
/// Uzun silindir için sürükleme katsayısı.
pub const CYLINDER_CD: f64 = 1.2;

#[derive(Clone, Copy, Debug)]
pub struct WireMechanics {
    /// Kütle (kg).
    pub mass: f64,
    /// Ortadan tutulan elemanın uç sarkması (m): her yarı, kendi ağırlığı
    /// altında ankastre kiriş sayılır, δ = wL⁴/(8EI).
    pub tip_sag: f64,
    /// Rüzgâra dik izdüşüm alanı (m²).
    pub wind_area: f64,
}

pub fn wire_mechanics(w: &Wire) -> WireMechanics {
    let area = w.cross_section();
    let len = w.length();
    let mass = area * len * w.material.density;
    let a = w.radius();
    let inner = w.wall.map(|t| (a - t).max(0.0)).unwrap_or(0.0);
    let inertia = std::f64::consts::PI / 4.0 * (a.powi(4) - inner.powi(4));
    let load = area * w.material.density * G; // N/m
    let half = 0.5 * len;
    WireMechanics { mass, tip_sag: load * half.powi(4) / (8.0 * w.material.youngs_modulus * inertia), wind_area: w.diameter * len }
}

/// Boru/çubuk kesit özellikleri: (alan m², atalet momenti m⁴, birim ağırlık N/m, EI N·m²).
pub fn tube_props(t: &crate::templates::Tube) -> (f64, f64, f64, f64) {
    let a = 0.5 * t.diameter;
    let inner = t.wall.map(|w| (a - w).max(0.0)).unwrap_or(0.0);
    let area = std::f64::consts::PI * (a * a - inner * inner);
    let inertia = std::f64::consts::PI / 4.0 * (a.powi(4) - inner.powi(4));
    (area, inertia, area * t.material.density * G, t.material.youngs_modulus * inertia)
}

/// Ortadan tutulan kademeli elemanın bir yarısı: içten dışa bölümler
/// (uzunluk m, birim ağırlık N/m, EI N·m²). Kendi ağırlığıyla uç sarkması (m).
/// Konsol kiriş, sayısal integral (moment → eğrilik → açı → sehim).
pub fn cantilever_tip_sag(sections: &[(f64, f64, f64)]) -> f64 {
    let total: f64 = sections.iter().map(|s| s.0).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let n = 600;
    let ds = total / n as f64;
    let prop = |s: f64| {
        let mut at = 0.0;
        for &(l, w, ei) in sections {
            if s <= at + l {
                return (w, ei);
            }
            at += l;
        }
        let last = sections[sections.len() - 1];
        (last.1, last.2)
    };
    // Kesme kuvveti ve moment uçtan içe birikir.
    let mut moment = vec![0.0; n + 1];
    let mut shear = 0.0;
    for i in (0..n).rev() {
        let s = (i as f64 + 0.5) * ds;
        let (w, _) = prop(s);
        let shear_new = shear + w * ds;
        moment[i] = moment[i + 1] + 0.5 * (shear + shear_new) * ds;
        shear = shear_new;
    }
    let (mut slope, mut defl) = (0.0, 0.0);
    for i in 0..n {
        let s = (i as f64 + 0.5) * ds;
        let (_, ei) = prop(s);
        let k = 0.5 * (moment[i] + moment[i + 1]) / ei;
        let slope_new = slope + k * ds;
        defl += 0.5 * (slope + slope_new) * ds;
        slope = slope_new;
    }
    defl
}

/// `speed` (m/s) rüzgârda `area` (m²) alanlı silindire etkiyen kuvvet (N).
pub fn wind_force(area: f64, speed: f64) -> f64 {
    0.5 * AIR_DENSITY * speed * speed * CYLINDER_CD * area
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;

    #[test]
    fn tube_is_lighter_and_mass_is_right() {
        let mut w = Wire {
            name: "t".into(),
            start: [0.0, -0.5, 0.0],
            end: [0.0, 0.5, 0.0],
            diameter: 0.010,
            wall: None,
            material: Material::aluminum_6063(),
            segments: 0,
        };
        let solid = wire_mechanics(&w);
        let expect = std::f64::consts::PI * 0.005f64.powi(2) * 2700.0;
        assert!((solid.mass - expect).abs() < 1e-9);
        w.wall = Some(0.001);
        let tube = wire_mechanics(&w);
        assert!(tube.mass < solid.mass);
        // Boru daha hafif olduğundan kendi ağırlığıyla daha az sarkar.
        assert!(tube.tip_sag < solid.tip_sag);
    }

    #[test]
    fn stepped_cantilever_matches_uniform_formula() {
        let w = Wire {
            name: "t".into(),
            start: [0.0, -0.5, 0.0],
            end: [0.0, 0.5, 0.0],
            diameter: 0.010,
            wall: Some(0.001),
            material: Material::aluminum_6063(),
            segments: 0,
        };
        let exact = wire_mechanics(&w).tip_sag;
        let t = crate::templates::Tube { diameter: 0.010, wall: Some(0.001), material: Material::aluminum_6063() };
        let (_, _, wl, ei) = tube_props(&t);
        let num = cantilever_tip_sag(&[(0.2, wl, ei), (0.3, wl, ei)]);
        assert!((num - exact).abs() / exact < 1e-4, "{num} vs {exact}");
    }
}
