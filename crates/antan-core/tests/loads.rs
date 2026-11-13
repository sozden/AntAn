//! Toplu yük (bobin, kondansatör, tuzak) doğrulamaları.

use antan_core::farfield::{Pattern, gain, to_db};
use antan_core::geometry::{Load, LoadKind};
use antan_core::loading::{loaded_vertical, trap_dipole, tune_load_inductance};
use antan_core::matching::swr;
use antan_core::solver::{self, SolveError};
use antan_core::templates::{self, Tube};
use antan_core::yagi::{ElementLoad, Profile, YagiDesign};
use antan_core::{Complex64, Material, nec};
use std::f64::consts::PI;

const F: f64 = 300.0e6;

fn perfect() -> Tube {
    Tube { diameter: 0.002, wall: None, material: Material::perfect() }
}

/// Beslemeyle aynı düğümdeki seri yük giriş empedansına aynen eklenir.
#[test]
fn load_at_feed_adds_in_series() {
    let mut a = templates::dipole(F, 0.4, &perfect());
    let z0 = solver::solve(&a, F).unwrap().impedance();
    let kind = LoadKind::SeriesRlc { r: 3.0, l: 50e-9, c: 10e-12 };
    a.loads.push(Load { wire: 0, position: 0.5, kind });
    let z1 = solver::solve(&a, F).unwrap().impedance();
    let zl = kind.impedance(F);
    println!("yüksüz {z0:.3}, yük {zl:.3}, yüklü {z1:.3}");
    assert!((z1 - (z0 + zl)).norm() < 1e-9 * z1.norm());
}

/// Dirençli yük: güç kaybı = ½|I|²R, verim düşer.
#[test]
fn resistive_load_loss_and_efficiency() {
    let mut a = templates::dipole(F, 0.48, &perfect());
    a.loads.push(Load { wire: 0, position: 0.3, kind: LoadKind::Fixed { r: 10.0, x: 0.0 } });
    let s = solver::solve(&a, F).unwrap();
    let l = &s.loads[0];
    assert!((l.power - 0.5 * l.current.norm_sqr() * 10.0).abs() < 1e-15);
    assert!((s.loss_power - l.power).abs() < 1e-15); // kayıpsız iletken
    let p = Pattern::compute(&s, 3.0).summary(&s);
    let eff_db = 10.0 * s.efficiency().log10();
    println!("verim {:.1}%, G {:.2}, D {:.2}", s.efficiency() * 100.0, p.max_gain_dbi, p.directivity_dbi);
    // Kazanç = yönlülük + verim (dB): enerji korunumu yük kaybıyla da tutmalı.
    assert!((p.max_gain_dbi - (p.directivity_dbi + eff_db)).abs() < 0.05);
    assert!(s.efficiency() < 0.95);
}

/// Bobin ayarı: kısa dikey ve kısaltılmış Yagi elemanı rezonansa gelir.
#[test]
fn inductance_tuning() {
    let v = loaded_vertical(14.2e6, &Tube::aluminum(0.01, None));
    let s = solver::solve(&v, 14.2e6).unwrap();
    let l = match v.loads[0].kind {
        LoadKind::SeriesRlc { l, .. } => l,
        _ => unreachable!(),
    };
    println!("λ/8 dikey: L = {:.2} µH, Z = {:.2}, verim {:.1}%", l * 1e6, s.impedance(), s.efficiency() * 100.0);
    assert!(s.impedance().im.abs() < 1.0);
    assert!(l > 1e-6 && l < 50e-6);
    assert!(s.efficiency() < 0.95 && s.efficiency() > 0.3);

    // Bobin olmadan kapasitif olmalı.
    let mut bare = v.clone();
    bare.loads.clear();
    assert!(solver::solve(&bare, 14.2e6).unwrap().impedance().im < -200.0);

    // Ayar fonksiyonu herhangi bir hedefe de gidebilmeli.
    let (_, z) = tune_load_inductance(&v, 0, 25.0).unwrap();
    assert!((z.im - 25.0).abs() < 0.1);
}

/// Tuzaklı dipol iki bantta da kullanılabilir SWR vermeli.
#[test]
fn trap_dipole_is_dual_band() {
    let a = trap_dipole(&Tube { diameter: 0.002, wall: None, material: Material::copper() });
    let best = |f0: f64, f1: f64| {
        (0..=40)
            .map(|i| f0 + (f1 - f0) * i as f64 / 40.0)
            .map(|f| (f, solver::solve(&a, f).unwrap().impedance()))
            .min_by(|x, y| swr(x.1, 60.0).partial_cmp(&swr(y.1, 60.0)).unwrap())
            .unwrap()
    };
    let (f40, z40) = best(6.6e6, 7.6e6);
    let (f80, z80) = best(3.3e6, 4.1e6);
    println!("40 m: {:.3} MHz {z40:.1}  ·  80 m: {:.3} MHz {z80:.1}", f40 / 1e6, f80 / 1e6);
    assert!(swr(z40, 60.0) < 2.0 && swr(z80, 60.0) < 2.0);
    // 40 m'de tuzak dış parçaları ayırır: akımın çoğu iç kollarda.
    let s = solver::solve(&a, f40).unwrap();
    let trap_i = s.loads[0].current.norm();
    let feed_i = s.feeds[0].current.norm();
    println!("40 m tuzak akımı / besleme akımı = {:.3}", trap_i / feed_i);
    assert!(trap_i < 0.2 * feed_i);
}

/// Yagi eleman yükü: ortada tek, dışarıda iki simetrik yük; kısaltılmış eleman rezonansa gelir.
#[test]
fn yagi_element_loads() {
    let f = 14.2e6;
    let lambda = antan_core::C0 / f;
    let mut d = YagiDesign::standard(f, 2, Profile::uniform(Tube::aluminum(0.03, Some(0.002))));
    for e in &mut d.elements {
        e.length *= 0.7;
    }
    d.loads.push(ElementLoad { element: 0, position: 0.0, kind: LoadKind::SeriesRlc { r: 0.0, l: 1e-6, c: 0.0 } });
    d.loads.push(ElementLoad { element: 1, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 4e-6, c: 0.0 } });
    let a = d.build();
    assert_eq!(a.loads.len(), 3);
    // Simetrik çift: aynı akım genliği.
    let s = solver::solve(&a, f).unwrap();
    assert!((s.loads[1].current.norm() - s.loads[2].current.norm()).abs() < 1e-9 * s.loads[1].current.norm());
    // Sürülen elemandaki simetrik bobinleri birlikte ayarla (ikisi de aynı L).
    let x_at = |l: f64| {
        let mut d2 = d.clone();
        d2.loads[1].kind = LoadKind::SeriesRlc { r: 0.0, l, c: 0.0 };
        solver::solve(&d2.build(), f).unwrap().impedance().im
    };
    // X(L) büyük L'de bir kutuptan geçer: önce küçükten yukarı tarayıp ilk
    // sıfır geçişini bul, sonra ikiye böl.
    let mut lo = 0.1e-6;
    while x_at(lo * 1.25) < 0.0 {
        lo *= 1.25;
        assert!(lo < 100e-6, "rezonans bulunamadı");
    }
    let mut hi = lo * 1.25;
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if x_at(mid) > 0.0 { hi = mid } else { lo = mid }
    }
    let l = 0.5 * (lo + hi);
    println!("0.7 boylu sürülen eleman: kol başına {:.2} µH ile rezonans ({:.2} λ uçtan uca)", l * 1e6, d.elements[1].length / lambda);
    assert!(x_at(l).abs() < 0.1);
    let mut d3 = d.clone();
    d3.loads[1].kind = LoadKind::SeriesRlc { r: 0.0, l, c: 0.0 };
    // Yansıtıcının orta bobinini ileri kazanç için tara (elektriksel olarak uzatır).
    let fwd_of = |lr: f64| {
        let mut d4 = d3.clone();
        d4.loads[0].kind = LoadKind::SeriesRlc { r: 0.0, l: lr, c: 0.0 };
        let s = solver::solve(&d4.build(), f).unwrap();
        (to_db(gain(&s, PI / 2.0, 0.0)), s)
    };
    let (lr, (fwd, s3)) = (1..=40).map(|k| k as f64 * 0.5e-6).map(|lr| (lr, fwd_of(lr))).max_by(|a, b| a.1.0.partial_cmp(&b.1.0).unwrap()).unwrap();
    let sm = Pattern::compute(&s3, 3.0).summary(&s3);
    println!(
        "kısaltılmış 2 el. Yagi: yansıtıcı bobini {:.1} µH, Z {:.1}, ileri {:.2} dBi, Ö/A {:.1} dB",
        lr * 1e6,
        s3.impedance(),
        fwd,
        sm.front_to_back_db
    );
    assert!(sm.max_phi.cos() > 0.95, "ışın ileri bakmıyor");
    assert!(fwd > 5.0);
}

#[test]
fn bad_load_positions() {
    let mut gp = templates::ground_plane(145e6, 45.0, &perfect());
    // Beş telin birleştiği uç: tek bir taban yok.
    gp.loads.push(Load { wire: 0, position: 0.0, kind: LoadKind::Fixed { r: 1.0, x: 0.0 } });
    assert_eq!(solver::solve(&gp, 145e6).unwrap_err(), SolveError::BadLoad(0));
    // Serbest uç.
    let mut d = templates::dipole(F, 0.5, &perfect());
    d.loads.push(Load { wire: 0, position: 1.0, kind: LoadKind::Fixed { r: 1.0, x: 0.0 } });
    assert_eq!(solver::solve(&d, F).unwrap_err(), SolveError::BadLoad(0));
}

#[test]
fn trap_impedance_and_nec_cards() {
    let trap = LoadKind::Trap { l: 8.2e-6, resonance: 7.1e6, q: 150.0 };
    // Rezonansta neredeyse gerçel ve çok büyük (≈ Q·ω0·L).
    let z = trap.impedance(7.1e6);
    let rp = 150.0 * 2.0 * PI * 7.1e6 * 8.2e-6;
    println!("tuzak @7.1 MHz: {z:.0}, Q·ω·L = {rp:.0}");
    assert!((z.re - rp).abs() / rp < 0.01 && z.im.abs() < 0.02 * rp);
    // 3.6 MHz'de endüktif.
    assert!(trap.impedance(3.6e6).im > 0.0);
    let s = nec::export(&trap_dipole(&Tube::aluminum(0.002, None)), None);
    assert_eq!(s.lines().filter(|l| l.starts_with("LD 1 ")).count(), 2);
    let _ = Complex64::new(0.0, 0.0);
}
