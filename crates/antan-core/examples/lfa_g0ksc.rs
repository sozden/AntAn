//! G0KSC'nin yayımladığı LFA Yagi tasarımlarını (g0ksc.co.uk) birebir kurup AntAn
//! çözücüsüyle hesaplar ve yayımlanan (EZNEC/NEC4) değerlerle karşılaştırır.
//! `cargo run --release -p antan-core --example lfa_g0ksc`

use antan_core::farfield::Pattern;
use antan_core::geometry::Antenna;
use antan_core::matching::swr;
use antan_core::templates::{Tube, source, wire};
use antan_core::{C0, Material, solver};

/// Eleman: (bumdaki konum m, yarı boy m, çap m).
type El = (f64, f64, f64);

struct Design {
    name: &'static str,
    f: f64,
    /// Yansıtıcı + yönlendiriciler (halka hariç).
    parasitic: Vec<El>,
    /// Halka: arka (beslenen) ve ön kenar konumu, yarı boy, kenar çapı, uç borusu çapı.
    de1: f64,
    de2: f64,
    loop_half: f64,
    loop_d: f64,
    end_d: f64,
    /// Yayımlanan: kazanç dBi, Ö/A dB, SWR < 1.1 bandı (MHz).
    pub_gain: f64,
    pub_fb: f64,
    band: (f64, f64),
}

fn al(d: f64) -> Tube {
    Tube { diameter: d, wall: None, material: Material::aluminum_6063() }
}

fn build(d: &Design, seg_mult: usize) -> Antenna {
    let lambda = C0 / d.f;
    let mut w = Vec::new();
    // Arka kenar (beslenen) ilk tel.
    w.push(wire("DE1", [d.de1, -d.loop_half, 0.0], [d.de1, d.loop_half, 0.0], &al(d.loop_d)));
    w.push(wire("DE2", [d.de2, -d.loop_half, 0.0], [d.de2, d.loop_half, 0.0], &al(d.loop_d)));
    w.push(wire("end R", [d.de1, d.loop_half, 0.0], [d.de2, d.loop_half, 0.0], &al(d.end_d)));
    w.push(wire("end L", [d.de1, -d.loop_half, 0.0], [d.de2, -d.loop_half, 0.0], &al(d.end_d)));
    for (i, &(x, h, dia)) in d.parasitic.iter().enumerate() {
        w.push(wire(&format!("P{i}"), [x, -h, 0.0], [x, h, 0.0], &al(dia)));
    }
    for wi in &mut w {
        wi.segments = wi.segment_count(lambda) * seg_mult;
    }
    Antenna::new(d.name, d.f, w, vec![source(0, 0.5)])
}

fn main() {
    let designs = vec![
        Design {
            name: "10el 50 MHz LFA (EU)",
            f: 50.150e6,
            parasitic: vec![
                (0.0, 1.482, 0.010),
                (1.792, 1.386, 0.010),
                (3.45, 1.355, 0.010),
                (5.459, 1.331, 0.010),
                (7.768, 1.313, 0.010),
                (10.173, 1.297, 0.010),
                (12.611, 1.281, 0.010),
                (15.129, 1.256, 0.010),
                (17.332, 1.246, 0.010),
            ],
            de1: 0.385,
            de2: 0.945,
            loop_half: 1.228,
            loop_d: 0.013,
            end_d: 0.010,
            pub_gain: 15.11,
            pub_fb: 30.25,
            band: (49.9, 50.4),
        },
        Design {
            name: "11el 70 MHz LFA (metric)",
            f: 70.200e6,
            parasitic: vec![
                (0.0, 1.0555, 0.010),
                (1.214, 0.9925, 0.010),
                (2.321, 0.964, 0.010),
                (3.728, 0.947, 0.010),
                (5.334, 0.933, 0.010),
                (7.006, 0.921, 0.010),
                (8.749, 0.9065, 0.010),
                (10.516, 0.895, 0.010),
                (12.365, 0.8845, 0.010),
                (13.739, 0.888, 0.010),
            ],
            de1: 0.292,
            de2: 0.688,
            loop_half: 0.881,
            loop_d: 0.012,
            end_d: 0.010,
            pub_gain: 15.44,
            pub_fb: 29.76,
            band: (70.0, 70.7),
        },
        Design {
            name: "11el 144 MHz LFA (EU)",
            f: 144.100e6,
            parasitic: vec![
                (0.0, 0.5135, 0.006),
                (0.545, 0.479, 0.006),
                (0.906, 0.459, 0.006),
                (1.356, 0.4595, 0.006),
                (2.019, 0.4545, 0.006),
                (2.785, 0.4495, 0.006),
                (3.608, 0.445, 0.006),
                (4.455, 0.4385, 0.006),
                (5.331, 0.43, 0.006),
                (6.125, 0.4275, 0.006),
            ],
            de1: 0.149,
            de2: 0.309,
            loop_half: 0.4365,
            loop_d: 0.013,
            end_d: 0.010,
            pub_gain: 15.25,
            pub_fb: 33.9,
            band: (143.35, 144.85),
        },
        Design {
            name: "11el 222 MHz LFA (US, tablo kontrolsüz)",
            f: 222.050e6,
            parasitic: vec![
                (0.0, 0.3315, 0.00635),
                (0.356, 0.308, 0.00635),
                (0.594, 0.291, 0.00635),
                (0.861, 0.2925, 0.00635),
                (1.276, 0.2895, 0.00635),
                (1.780, 0.284, 0.00635),
                (2.325, 0.278, 0.00635),
                (2.896, 0.2705, 0.00635),
                (3.493, 0.2605, 0.00635),
                (4.025, 0.259, 0.00635),
            ],
            de1: 0.098,
            de2: 0.205,
            loop_half: 0.2585,
            loop_d: 0.0127,
            end_d: 0.009525,
            pub_gain: 14.92,
            pub_fb: 43.32,
            band: (221.0, 224.0),
        },
    ];

    for d in &designs {
        let lambda = C0 / d.f;
        println!("=== {}  ({:.3} MHz)", d.name, d.f / 1e6);
        println!(
            "    halka: uzun kenar {:.0} mm ({:.3} λ), genişlik {:.0} mm ({:.3} λ), çevre {:.3} λ",
            2.0 * d.loop_half * 1e3,
            2.0 * d.loop_half / lambda,
            (d.de2 - d.de1) * 1e3,
            (d.de2 - d.de1) / lambda,
            2.0 * (2.0 * d.loop_half + d.de2 - d.de1) / lambda
        );
        for m in [1usize, 2, 4] {
            let a = build(d, m);
            let s = solver::solve(&a, d.f).expect("çözülemedi");
            let p = Pattern::compute(&s, 1.0).summary(&s);
            let z = s.impedance();
            println!(
                "    AntAn ({} parça/λ, {} bilinmeyen): G = {:.2} dBi (yayımlanan {:.2}, fark {:+.2}),  Ö/A = {:.1} dB (yayımlanan {:.1}),  Z = {:.1} {:+.1}j Ω, SWR {:.2}",
                40 * m,
                s.basis.len(),
                p.max_gain_dbi,
                d.pub_gain,
                p.max_gain_dbi - d.pub_gain,
                p.front_to_back_db,
                d.pub_fb,
                z.re,
                z.im,
                swr(z, 50.0)
            );
        }
        // Yayımlanan SWR < 1.1 bandı boyunca SWR (50 Ω).
        let a = build(d, 1);
        let mut line = String::from("    SWR bant boyunca:");
        let mut worst: f64 = 0.0;
        for k in 0..=6 {
            let f = d.band.0 + (d.band.1 - d.band.0) * k as f64 / 6.0;
            let z = solver::solve(&a, f * 1e6).unwrap().impedance();
            let s = swr(z, 50.0);
            worst = worst.max(s);
            line += &format!(" {:.3}:{:.2}", f, s);
        }
        println!("{line}   (en kötü {:.2}; yayımlanan < 1.10)", worst);
        // Aynı frekansta en iyi SWR için ne kadar frekans kayması gerekiyor?
        let (mut bf, mut bs) = (0.0, f64::INFINITY);
        for k in -40..=40 {
            let f = d.f * (1.0 + k as f64 * 0.0005);
            let s = swr(solver::solve(&a, f).unwrap().impedance(), 50.0);
            if s < bs {
                (bf, bs) = (f, s);
            }
        }
        println!("    en düşük SWR {:.2} @ {:.3} MHz (tasarım frekansından %{:+.2})", bs, bf / 1e6, (bf / d.f - 1.0) * 100.0);
    }
}
