//! Hazır anten şablonları. Şimdilik: yarım dalga dipol ve Yagi-Uda.

use num_complex::Complex64;

use crate::C0;
use crate::geometry::{Antenna, Source, Wire};
use crate::material::Material;

/// Eleman kesiti: dış çap, et kalınlığı (`None` = dolu), malzeme.
#[derive(Clone, Debug)]
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

/// Y eksenine paralel, `x` konumunda, ortası bum üzerinde bir eleman.
pub fn element(name: &str, x: f64, length: f64, tube: &Tube) -> Wire {
    Wire {
        name: name.into(),
        start: [x, -0.5 * length, 0.0],
        end: [x, 0.5 * length, 0.0],
        diameter: tube.diameter,
        wall: tube.wall,
        material: tube.material.clone(),
        segments: 0,
    }
}

fn center_source(wire: usize) -> Source {
    Source { wire, position: 0.5, voltage: Complex64::new(1.0, 0.0) }
}

/// Ortadan beslemeli dipol; `length_wl` dalga boyu cinsinden boy.
pub fn dipole(frequency: f64, length_wl: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    Antenna { name: "Dipol".into(), frequency, wires: vec![element("Dipol", 0.0, length_wl * lambda, tube)], sources: vec![center_source(0)] }
}

/// `elements` elemanlı (en az 2) başlangıç Yagi'si: yansıtıcı, sürülen eleman
/// ve yönlendiriciler. Ölçüler genel geçer başlangıç değerleridir; iyileştirme
/// (optimizasyon) kullanıcıya/iyileştiriciye bırakılır.
pub fn yagi(frequency: f64, elements: usize, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let n = elements.max(2);
    let mut wires = vec![element("Yansıtıcı", 0.0, 0.495 * lambda, tube), element("Sürülen", 0.18 * lambda, 0.468 * lambda, tube)];
    let mut x = 0.18;
    for d in 0..n - 2 {
        // İlk yönlendirici yakın, sonrakiler giderek açılan aralıkla.
        x += if d == 0 { 0.12 } else { (0.20 + 0.03 * d as f64).min(0.32) };
        let len = (0.445 - 0.006 * d as f64).max(0.40);
        wires.push(element(&format!("Yönlendirici {}", d + 1), x * lambda, len * lambda, tube));
    }
    Antenna { name: format!("{n} elemanlı Yagi"), frequency, wires, sources: vec![center_source(1)] }
}
