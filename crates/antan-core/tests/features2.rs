//! 0.2 özellikleri: LFA, bobinli dipol, Moxon boşluğu, boyut sınırı.

use antan_core::farfield::Pattern;
use antan_core::solver::{self, SolveError};
use antan_core::templates::{self, Kind, Tube};
use antan_core::yagi::{Profile, YagiDesign};
use antan_core::{C0, Material};

fn tube10() -> Tube {
    Tube::aluminum(0.010, Some(0.001))
}

#[test]
fn lfa_is_near_50_ohm_with_good_pattern() {
    for n in [3usize, 4, 5, 6, 7, 9, 11] {
        let f = 144.3e6;
        let d = YagiDesign::lfa(f, n, Profile::uniform(tube10()));
        let a = d.build();
        // Halka: 4 tel, sürülen arka kenar ortasından beslenir.
        assert_eq!(a.wires.len(), n - 1 + 4);
        let s = solver::solve(&a, f).unwrap();
        let z = s.impedance();
        let p = Pattern::compute(&s, 5.0).summary(&s);
        println!("{n} el LFA: Z = {z:.1}, G = {:.2} dBi, F/B = {:.1} dB", p.max_gain_dbi, p.front_to_back_db);
        assert!((z.re - 50.0).abs() < 20.0 && z.im.abs() < 25.0, "Z = {z}");
        assert!(p.front_to_back_db > 10.0);
        assert!(p.max_gain_dbi > 8.0);
        // Ön yön +X.
        assert!(p.max_theta.to_degrees() > 80.0 && p.max_phi.to_degrees().abs() < 10.0);
    }
}

#[test]
fn lfa_survives_json_and_driven_tuning() {
    let d = YagiDesign::lfa(50.15e6, 5, Profile::uniform(tube10()));
    let s = serde_json::to_string(&d).unwrap();
    let back: YagiDesign = serde_json::from_str(&s).unwrap();
    assert_eq!(back, d);
    let (_, z) = d.tune_driven(|_| Some(0.0)).expect("halka boyu ayarlanabilmeli");
    assert!(z.im.abs() < 0.1);
}

#[test]
fn loaded_dipole_is_resonant_and_short() {
    let f = 7.1e6;
    let t = Tube { diameter: 0.002, wall: None, material: Material::copper() };
    let a = Kind::LoadedDipole.build(f, &t);
    let len = a.wires[0].length();
    assert!((len / (C0 / f) - 0.24).abs() < 1e-9);
    let s = solver::solve(&a, f).unwrap();
    let z = s.impedance();
    assert!(z.im.abs() < 1.0, "Z = {z}");
    // Kısa dipol: düşük ışıma direnci, bobin kaybıyla verim < %100.
    assert!(z.re > 5.0 && z.re < 40.0, "Z = {z}");
    assert!(s.efficiency() < 0.99 && s.efficiency() > 0.5);
    // İki bobin eşit.
    assert_eq!(a.loads[0].kind, a.loads[1].kind);
    assert_eq!(antan_core::loading::symmetric_partner(&a, 0), Some(1));
}

#[test]
fn moxon_gap_is_reported() {
    let f = 144.3e6;
    let t = Tube::aluminum(0.004, None);
    let a = templates::moxon(f, &t);
    let lambda = C0 / f;
    let [_, _, c, _, _] = templates::moxon_dimensions(t.diameter / lambda).map(|v| v * lambda * 1.012);
    let gaps = templates::open_end_gaps(&a, 0.2 * lambda);
    assert_eq!(gaps.len(), 2, "{gaps:?}");
    for (_, _, g) in gaps {
        assert!((g - c).abs() < 1e-6, "{g} vs {c}");
    }
    // Dipolde boşluk yok.
    assert!(templates::open_end_gaps(&templates::dipole(f, 0.48, &t), lambda).is_empty());
}

#[test]
fn too_large_model_is_refused_quickly() {
    let d = YagiDesign::standard(144.3e6, 5, Profile::uniform(tube10()));
    let mut a = d.build();
    a.frequency = 30e9;
    let t0 = std::time::Instant::now();
    match solver::solve(&a, a.frequency) {
        Err(SolveError::TooLarge(n)) => assert!(n > solver::MAX_UNKNOWNS),
        other => panic!("{:?}", other.map(|s| s.impedance())),
    }
    assert!(t0.elapsed().as_millis() < 100);
}

#[test]
fn height_scan_dipole_takeoff_drops_with_height() {
    let f = 14.2e6;
    let lambda = C0 / f;
    let mut a = templates::dipole(f, 0.48, &Tube::aluminum(0.01, None));
    a.ground = antan_core::geometry::Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    let hs: Vec<f64> = [0.25, 0.5, 1.0].iter().map(|k| k * lambda).collect();
    let rows = antan_core::scan::height_scan(&a, &hs, &|| {});
    assert_eq!(rows.len(), 3);
    // λ/2'de ~27–30°, λ'da ~14°: kalkış açısı yükseklikle düşer.
    assert!(rows[1].takeoff_deg > 22.0 && rows[1].takeoff_deg < 34.0, "{:?}", rows[1]);
    assert!(rows[2].takeoff_deg < rows[1].takeoff_deg);
    assert!(rows[0].takeoff_deg > rows[1].takeoff_deg);
}

#[test]
fn yagi_tips_are_not_reported_as_gaps() {
    let f = 144.3e6;
    let a = YagiDesign::standard(f, 7, Profile::uniform(tube10())).build();
    assert!(templates::open_end_gaps(&a, 0.25 * C0 / f).is_empty());
    let l = YagiDesign::lfa(f, 7, Profile::uniform(tube10())).build();
    assert!(templates::open_end_gaps(&l, 0.25 * C0 / f).is_empty());
}
