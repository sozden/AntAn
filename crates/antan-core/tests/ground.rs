//! Zemin (görüntü yöntemi ve gerçek zemin) doğrulamaları.

use antan_core::farfield::{self, Pattern, to_db};
use antan_core::geometry::{Ground, Source};
use antan_core::solver::{self, SolveError};
use antan_core::templates::{Tube, source, wire};
use antan_core::{Antenna, Complex64, Material};
use std::f64::consts::PI;

const F: f64 = 300.0e6; // λ = 1 m

fn t() -> Tube {
    Tube { diameter: 0.002, wall: None, material: Material::perfect() }
}

fn horizontal(h: f64, ground: Ground) -> Antenna {
    let mut a = Antenna::new("yatay", F, vec![wire("d", [0.0, -0.24, h], [0.0, 0.24, h], &t())], vec![source(0, 0.5)]);
    a.ground = ground;
    a
}

/// Kusursuz zemin = serbest uzayda ters işaretle beslenen açık görüntü dipolü.
#[test]
fn horizontal_dipole_equals_explicit_image() {
    let h = 0.5;
    let g = solver::solve(&horizontal(h, Ground::Perfect), F).unwrap();
    let pair = Antenna::new(
        "çift",
        F,
        vec![wire("d", [0.0, -0.24, h], [0.0, 0.24, h], &t()), wire("g", [0.0, -0.24, -h], [0.0, 0.24, -h], &t())],
        vec![source(0, 0.5), Source { wire: 1, position: 0.5, voltage: Complex64::new(-1.0, 0.0) }],
    );
    let p = solver::solve(&pair, F).unwrap();
    println!("zemin {:.3}, açık görüntü {:.3}", g.impedance(), p.impedance());
    assert!((g.impedance() - p.impedance()).norm() < 1e-6 * p.impedance().norm());
    // Üst yarı uzayda alan aynı; zeminli antenin giriş gücü yarı → kazanç +3.01 dB.
    for (th, ph) in [(0.3, 0.0), (1.0, 0.7), (1.4, 2.0)] {
        let d = to_db(farfield::gain(&g, th, ph)) - to_db(farfield::gain(&p, th, ph));
        assert!((d - 10.0 * 2f64.log10()).abs() < 1e-6, "{d}");
    }
}

#[test]
fn vertical_dipole_equals_explicit_image() {
    let h = 0.4;
    let mut a = Antenna::new("dikey", F, vec![wire("d", [0.0, 0.0, h - 0.24], [0.0, 0.0, h + 0.24], &t())], vec![source(0, 0.5)]);
    a.ground = Ground::Perfect;
    let g = solver::solve(&a, F).unwrap();
    let pair = Antenna::new(
        "çift",
        F,
        vec![wire("d", [0.0, 0.0, h - 0.24], [0.0, 0.0, h + 0.24], &t()), wire("g", [0.0, 0.0, -h - 0.24], [0.0, 0.0, -h + 0.24], &t())],
        vec![source(0, 0.5), source(1, 0.5)],
    );
    let p = solver::solve(&pair, F).unwrap();
    println!("zemin {:.3}, açık görüntü {:.3}", g.impedance(), p.impedance());
    assert!((g.impedance() - p.impedance()).norm() < 1e-6 * p.impedance().norm());
}

/// Zemine bağlı, tabandan beslenen λ/4 monopol: Z = dipol/2, kazanç = dipol + 3 dB.
#[test]
fn monopole_is_half_dipole() {
    let mut mono = Antenna::new("monopol", F, vec![wire("m", [0.0, 0.0, 0.0], [0.0, 0.0, 0.24], &t())], vec![source(0, 0.0)]);
    mono.wires[0].segments = 20;
    mono.ground = Ground::Perfect;
    let mut dip = Antenna::new("dipol", F, vec![wire("d", [0.0, 0.0, -0.24], [0.0, 0.0, 0.24], &t())], vec![source(0, 0.5)]);
    dip.wires[0].segments = 40;
    let zm = solver::solve(&mono, F).unwrap();
    let zd = solver::solve(&dip, F).unwrap();
    println!("monopol {:.3}, dipol/2 {:.3}", zm.impedance(), zd.impedance() / 2.0);
    assert!((zm.impedance() - zd.impedance() / 2.0).norm() < 1e-6 * zd.impedance().norm());
    let dg = to_db(farfield::gain(&zm, PI / 2.0, 0.0)) - to_db(farfield::gain(&zd, PI / 2.0, 0.0));
    assert!((dg - 3.0103).abs() < 1e-3, "{dg}");
    // Zemin altında ışıma yok.
    assert_eq!(farfield::gain(&zm, 2.0, 0.0), 0.0);
}

/// λ/2 yükseklikte yatay dipol: kalkış açısı 30°, enerji korunumu.
#[test]
fn horizontal_dipole_takeoff_and_power() {
    let sol = solver::solve(&horizontal(0.5, Ground::Perfect), F).unwrap();
    let pat = Pattern::compute(&sol, 1.0);
    let s = pat.summary(&sol);
    let takeoff = 90.0 - s.max_theta.to_degrees();
    println!("kalkış {takeoff:.1}°, G {:.2} dBi, D {:.2} dBi", s.max_gain_dbi, s.directivity_dbi);
    assert!((takeoff - 30.0).abs() < 1.5);
    assert!((s.max_gain_dbi - s.directivity_dbi).abs() < 0.1);
    assert!(s.max_gain_dbi > 7.0 && s.max_gain_dbi < 9.0);
}

/// Gerçek zemin: dikey polarizasyon ufukta söner; çok iletken zemin kusursuza yaklaşır.
#[test]
fn real_ground_reflection() {
    let mut mono = Antenna::new("monopol", 14.2e6, vec![wire("m", [0.0, 0.0, 0.0], [0.0, 0.0, 5.0], &Tube::aluminum(0.03, None))], vec![source(0, 0.0)]);
    mono.ground = Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    let real = solver::solve(&mono, 14.2e6).unwrap();
    let horizon = farfield::gain(&real, PI / 2.0, 0.0);
    let s = Pattern::compute(&real, 1.0).summary(&real);
    println!("ortalama toprak: maks {:.2} dBi @ {:.0}°, ufuk {:.1} dBi", s.max_gain_dbi, 90.0 - s.max_theta.to_degrees(), to_db(horizon));
    assert!(to_db(horizon) < s.max_gain_dbi - 30.0);
    let takeoff = 90.0 - s.max_theta.to_degrees();
    assert!(takeoff > 5.0 && takeoff < 40.0);

    mono.ground = Ground::Real { permittivity: 1.0, conductivity: 1e9 };
    let good = solver::solve(&mono, 14.2e6).unwrap();
    mono.ground = Ground::Perfect;
    let perfect = solver::solve(&mono, 14.2e6).unwrap();
    for th in [0.3, 0.8, 1.2] {
        let d = to_db(farfield::gain(&good, th, 0.0)) - to_db(farfield::gain(&perfect, th, 0.0));
        assert!(d.abs() < 0.01, "θ={th}: {d}");
    }
}

#[test]
fn wire_below_ground_is_rejected() {
    let a = horizontal(-0.1, Ground::Perfect);
    assert_eq!(solver::solve(&a, F).unwrap_err(), SolveError::BelowGround(0));
    // Serbest uzayda aynı geometri geçerli.
    assert!(solver::solve(&horizontal(-0.1, Ground::FreeSpace), F).is_ok());
}

/// Zemin üzerindeki Yagi ileri ve yukarı ışır; yükseklik arttıkça kalkış açısı düşer.
#[test]
fn yagi_over_ground_takeoff_falls_with_height() {
    use antan_core::yagi::{Profile, YagiDesign};
    let f = 144.3e6;
    let lambda = antan_core::C0 / f;
    let mut d = YagiDesign::standard(f, 5, Profile::uniform(Tube::aluminum(0.01, None)));
    d.ground = Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    let mut last = 90.0;
    for h in [0.5, 1.0, 2.0] {
        d.height = h * lambda;
        let sol = solver::solve(&d.build(), f).unwrap();
        let s = Pattern::compute(&sol, 1.0).summary(&sol);
        let takeoff = 90.0 - s.max_theta.to_degrees();
        println!("h = {h} λ: kalkış {takeoff:.0}°, G {:.2} dBi, Ö/A {:.1} dB", s.max_gain_dbi, s.front_to_back_db);
        assert!(s.max_phi.cos() > 0.99);
        assert!(takeoff < last);
        last = takeoff;
    }
}

/// İyileştirici zemin üzerinde kazancı ufukta değil kalkış açısında ölçmeli.
#[test]
fn optimizer_scores_at_takeoff_angle() {
    use antan_core::optimize::{Goals, evaluate};
    use antan_core::yagi::{Profile, YagiDesign};
    let f = 144.3e6;
    let mut d = YagiDesign::standard(f, 5, Profile::uniform(Tube::aluminum(0.01, None)));
    let free = evaluate(&d, &Goals::default()).unwrap();
    d.ground = Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    d.height = 2.0 * antan_core::C0 / f;
    let over = evaluate(&d, &Goals::default()).unwrap();
    println!("serbest {:.2} dBi, zemin üzerinde {:.2} dBi", free.gain_dbi, over.gain_dbi);
    assert!(over.gain_dbi > free.gain_dbi + 4.0);
}
