//! Çözücünün bilinen sonuçlarla karşılaştırılması.

use antan_core::farfield::{self, Pattern};
use antan_core::templates::{self, Tube};
use antan_core::{Material, solver};
use std::f64::consts::PI;

fn perfect(diameter: f64) -> Tube {
    Tube { diameter, wall: None, material: Material::perfect() }
}

/// İnce yarım dalga dipol: Z ≈ 73 + j42 Ω, kazanç ≈ 2.15 dBi.
#[test]
fn half_wave_dipole() {
    let f = 300.0e6; // λ = 1 m
    let ant = templates::dipole(f, 0.5, &perfect(0.0002));
    let sol = solver::solve(&ant, f).unwrap();
    let z = sol.impedance();
    println!("dipol Z = {z:.2}");
    assert!((z.re - 73.0).abs() < 8.0, "R = {}", z.re);
    assert!((z.im - 42.5).abs() < 12.0, "X = {}", z.im);

    let g = farfield::to_db(farfield::gain(&sol, PI / 2.0, 0.0));
    println!("dipol kazanç = {g:.2} dBi");
    assert!((g - 2.15).abs() < 0.15, "G = {g}");
    // Eleman ekseni (Y) boyunca ışıma olmamalı.
    assert!(farfield::gain(&sol, PI / 2.0, PI / 2.0) < 1e-6);
}

/// Kayıpsız antende ışınan güç giren güce eşit olmalı (enerji korunumu):
/// yönlülük = kazanç.
#[test]
fn power_is_conserved() {
    let f = 144.3e6;
    let ant = templates::yagi(f, 3, &perfect(0.010));
    let sol = solver::solve(&ant, f).unwrap();
    let pat = Pattern::compute(&sol, 3.0);
    let s = pat.summary(&sol);
    println!("3 el: Z = {:.2}, G = {:.2} dBi, D = {:.2} dBi, F/B = {:.1} dB", sol.impedance(), s.max_gain_dbi, s.directivity_dbi, s.front_to_back_db);
    assert!((s.max_gain_dbi - s.directivity_dbi).abs() < 0.15);
}

/// Yagi ileri (+X) yönde ışımalı, makul kazanç ve ön/arka oranı vermeli.
#[test]
fn yagi_is_directional() {
    let f = 144.3e6;
    for n in [3, 5] {
        let ant = templates::yagi(f, n, &Tube::aluminum(0.010, Some(0.001)));
        let sol = solver::solve(&ant, f).unwrap();
        let s = Pattern::compute(&sol, 3.0).summary(&sol);
        println!(
            "{n} el: Z = {:.2}, G = {:.2} dBi, F/B = {:.1} dB, BW = {:.0}°/{:.0}°, verim = {:.2}%",
            sol.impedance(),
            s.max_gain_dbi,
            s.front_to_back_db,
            s.beamwidth_az_deg,
            s.beamwidth_el_deg,
            sol.efficiency() * 100.0
        );
        assert!(s.max_phi.cos() > 0.99 && (s.max_theta - PI / 2.0).abs() < 0.06, "ana huzme +X'te değil");
        assert!(s.max_gain_dbi > 6.0 && s.max_gain_dbi < 13.0, "G = {}", s.max_gain_dbi);
        assert!(s.front_to_back_db > 6.0);
        assert!(sol.efficiency() > 0.98);
    }
}

/// Parça sayısı arttıkça empedans yakınsamalı.
#[test]
fn converges_with_segments() {
    let f = 300.0e6;
    let z = |segs: usize| {
        let mut ant = templates::dipole(f, 0.47, &perfect(0.002));
        ant.wires[0].segments = segs;
        solver::solve(&ant, f).unwrap().impedance()
    };
    let (z20, z40, z80) = (z(20), z(40), z(80));
    println!("yakınsama: 20 → {z20:.2}, 40 → {z40:.2}, 80 → {z80:.2}");
    assert!((z40 - z80).norm() < (z20 - z40).norm() + 0.5);
    assert!((z40 - z80).norm() < 4.0);
}
