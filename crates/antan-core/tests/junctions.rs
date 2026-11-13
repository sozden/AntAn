//! Tel birleşimleri ve yeni şablonların doğrulanması.

use antan_core::farfield::Pattern;
use antan_core::templates::{self, Tube, wire};
use antan_core::yagi::{Profile, TaperSection, YagiDesign};
use antan_core::{Antenna, Material, solver};
use num_complex::Complex64;
use std::f64::consts::PI;

fn perfect(d: f64) -> Tube {
    Tube { diameter: d, wall: None, material: Material::perfect() }
}

/// Üç düz parçaya bölünmüş dipol, tek parça dipolle aynı sonucu vermeli.
#[test]
fn split_dipole_matches_single_wire() {
    let f = 300.0e6;
    let t = perfect(0.002);
    let single = templates::dipole(f, 0.48, &t);
    let mut s = single.clone();
    s.wires[0].segments = 40;
    let z1 = solver::solve(&s, f).unwrap().impedance();

    let h = 0.24;
    let split = Antenna {
        name: "bölünmüş".into(),
        frequency: f,
        wires: vec![
            Wire { segments: 10, ..wire("sol", [0.0, -h, 0.0], [0.0, -0.08, 0.0], &t) },
            Wire { segments: 10, ..wire("orta", [0.0, -0.08, 0.0], [0.0, 0.08, 0.0], &t) },
            Wire { segments: 10, ..wire("sağ", [0.0, 0.08, 0.0], [0.0, h, 0.0], &t) },
        ],
        sources: vec![templates::source(1, 0.5)],
    };
    let z2 = solver::solve(&split, f).unwrap().impedance();
    println!("tek parça {z1:.2}, bölünmüş {z2:.2}");
    // Parça boyları farklı (12 mm / 16 mm), küçük fark beklenir.
    assert!((z1 - z2).norm() < 3.0, "{z1} vs {z2}");
}
use antan_core::Wire;

/// Uçtan beslenen iki kollu düz anten (birleşimden besleme) dipolle aynı olmalı.
#[test]
fn junction_feed_matches_center_feed() {
    let f = 300.0e6;
    let t = perfect(0.002);
    let mut d = templates::dipole(f, 0.48, &t);
    d.wires[0].segments = 40;
    let z1 = solver::solve(&d, f).unwrap().impedance();
    let v = templates::inverted_v(f, 180.0, &t); // 180° = düz
    let mut v = v;
    for w in &mut v.wires {
        w.end[1] *= 0.96; // kollar 0.25λ → 0.24λ
        w.end[2] = 0.0;
        w.segments = 20;
    }
    let z2 = solver::solve(&v, f).unwrap().impedance();
    println!("orta besleme {z1:.2}, birleşim beslemesi {z2:.2}");
    assert!((z1 - z2).norm() < 2.0, "{z1} vs {z2}");
}

/// Katlanmış dipol: eşdeğer yarıçaplı dipolün 4 katı ile kısa devre hat modunun
/// paraleli (klasik model). Model yaklaşık olduğundan %15 tolerans.
#[test]
fn folded_dipole_matches_equivalent_radius_model() {
    let f = 300.0e6; // λ = 1 m
    let (a, s) = (0.001f64, 0.02f64);
    let fd = templates::folded_dipole(f, s, &perfect(2.0 * a));
    let zf = solver::solve(&fd, f).unwrap().impedance();
    let ae = (a * s).sqrt();
    let zd = solver::solve(&templates::dipole(f, 0.48, &perfect(2.0 * ae)), f).unwrap().impedance();
    let z0 = 120.0 * (s / (2.0 * a)).acosh();
    let zt = Complex64::new(0.0, z0 * (2.0 * PI * 0.24).tan());
    let model = (4.0 * zd) * (2.0 * zt) / (4.0 * zd + 2.0 * zt);
    println!("katlanmış {zf:.1}, model {model:.1}");
    assert!((zf.re - model.re).abs() / model.re < 0.15);
}

/// Kayıpsız birleşimli antenlerde ışınan güç = giren güç (kazanç = yönlülük).
#[test]
fn junction_antennas_conserve_power() {
    let f = 144.3e6;
    let t = perfect(0.004);
    for a in [templates::quad(f, 2, &t), templates::moxon(f, &t), templates::ground_plane(f, 45.0, &t), templates::delta_loop(f, &t)] {
        let sol = solver::solve(&a, f).unwrap();
        let s = Pattern::compute(&sol, 2.0).summary(&sol);
        assert!((s.max_gain_dbi - s.directivity_dbi).abs() < 0.05, "{}: {} vs {}", a.name, s.max_gain_dbi, s.directivity_dbi);
    }
}

/// Kirchhoff: GP tabanında dikey akım = radyal akımların toplamı.
#[test]
fn ground_plane_kcl_and_pattern() {
    let f = 145.0e6;
    let gp = templates::ground_plane(f, 45.0, &Tube::aluminum(0.006, None));
    let sol = solver::solve(&gp, f).unwrap();
    let iv = sol.wire_currents(0)[0].1;
    let ir: Complex64 = (1..5).map(|w| sol.wire_currents(w)[0].1).sum();
    println!("GP Z = {:.2}, dikey {iv:.4}, radyaller {ir:.4}", sol.impedance());
    // Akımlar tel yönünde: dikey yukarı, radyaller dışa doğru → I_dikey = −Σ I_radyal.
    assert!((iv + ir).norm() < 1e-9 * iv.norm());
    assert!((iv - sol.feeds[0].current).norm() < 1e-12);
    let z = sol.impedance();
    assert!(z.re > 20.0 && z.re < 70.0, "{z}");
    let s = Pattern::compute(&sol, 3.0).summary(&sol);
    println!("GP kazanç {:.2} dBi, yükseliş {:.0}°", s.max_gain_dbi, 90.0 - s.max_theta.to_degrees());
    assert!((90.0 - s.max_theta.to_degrees()).abs() < 30.0);
}

#[test]
fn quad_and_moxon_are_directional() {
    let f = 144.3e6;
    let t = Tube::aluminum(0.004, None);
    for ant in [templates::quad(f, 2, &t), templates::moxon(f, &t), templates::delta_loop(f, &t)] {
        let sol = solver::solve(&ant, f).unwrap();
        let s = Pattern::compute(&sol, 3.0).summary(&sol);
        println!(
            "{}: Z = {:.1}, G = {:.2} dBi, F/B = {:.1} dB, yön φ={:.0}° θ={:.0}°",
            ant.name,
            sol.impedance(),
            s.max_gain_dbi,
            s.front_to_back_db,
            s.max_phi.to_degrees(),
            s.max_theta.to_degrees()
        );
        if ant.name == "Delta loop" {
            // Rezonansta, iki yöne (±X) eşit ışıyan halka.
            assert!(sol.impedance().im.abs() < 25.0 && (s.front_to_back_db).abs() < 0.5);
        } else {
            assert!(s.max_phi.cos() > 0.95 && (s.max_theta - PI / 2.0).abs() < 0.1, "{} ileri bakmıyor", ant.name);
            assert!(s.front_to_back_db > 20.0, "{} F/B {}", ant.name, s.front_to_back_db);
            assert!(s.max_gain_dbi > 5.5);
        }
    }
}

/// Sürülen eleman ayarı: rezonans (X = 0) ve hairpin (X = −√(R(Z₀−R))).
#[test]
fn tune_driven_element() {
    let f = 144.3e6;
    let d = YagiDesign::standard(f, 4, Profile::uniform(Tube::aluminum(0.01, None)));
    let (res, z) = d.tune_driven(|_| Some(0.0)).unwrap();
    println!("rezonans: L {:.1} → {:.1} mm, Z {z:.2}", d.elements[1].length * 1e3, res.elements[1].length * 1e3);
    assert!(z.im.abs() < 0.1);
    // Hairpin R < Z₀ ister: 3 elemanlı tasarımda R ≈ 25 Ω.
    let d = YagiDesign::standard(f, 3, Profile::uniform(Tube::aluminum(0.01, None)));
    let (hp, z) = d.tune_driven(|z| (z.re < 50.0).then(|| -(z.re * (50.0 - z.re)).sqrt())).unwrap();
    let h = antan_core::matching::hairpin(z, 50.0, f, 0.004, 0.03).unwrap();
    println!("hairpin: L {:.1} mm, Z {z:.2}, saplama {:.1} mm", hp.elements[1].length * 1e3, h.length * 1e3);
    assert!(h.reactance_change.abs() < 0.1);
}

/// Kademeli profil, aynı çaplı bölümlerle tekdüze elemanla aynı sonucu vermeli;
/// incelen uçlar rezonansı yukarı kaydırmalı (aynı boyda daha kapasitif).
#[test]
fn tapered_yagi() {
    let f = 144.3e6;
    let tube = Tube::aluminum(0.012, Some(0.001));
    let uniform = YagiDesign::standard(f, 3, Profile::uniform(tube.clone()));
    let same = YagiDesign::standard(f, 3, Profile { sections: vec![TaperSection { half_length: 0.15, tube: tube.clone() }], tip: tube.clone() });
    let z_u = solver::solve(&uniform.build(), f).unwrap().impedance();
    let z_s = solver::solve(&same.build(), f).unwrap().impedance();
    println!("tekdüze {z_u:.2}, aynı çaplı bölümlü {z_s:.2}");
    assert!((z_u - z_s).norm() < 2.0);

    let thin = Tube::aluminum(0.006, Some(0.001));
    let taper = YagiDesign::standard(f, 3, Profile { sections: vec![TaperSection { half_length: 0.15, tube }], tip: thin });
    let (ant, map) = taper.build_with_map();
    assert_eq!(map.len(), 3);
    assert_eq!(map[1].len(), 3);
    let z_t = solver::solve(&ant, f).unwrap().impedance();
    println!("kademeli {z_t:.2}");
    assert!(z_t.im < z_u.im + 25.0);
}
