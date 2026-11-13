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
}
