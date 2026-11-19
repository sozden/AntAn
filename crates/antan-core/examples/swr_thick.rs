//! Kalın eleman etkisi: şablon boylarını topluca kısaltınca 23 cm Yagi düzeliyor mu?
//! Ayrıca yalıtılmış dipolün rezonans boyunun çap/λ ile değişimi; parça boyu
//! yarıçapın 2.5 katından kısa olmasın diye sınırlanmış ve sınırlanmamış.
//! `cargo run --release -p antan-core --example swr_thick`

use antan_core::farfield::Pattern;
use antan_core::geometry::Antenna;
use antan_core::matching::swr;
use antan_core::solver;
use antan_core::templates::{Tube, dipole};
use antan_core::yagi::{Profile, YagiDesign};
use antan_core::C0;

/// Parça boyu ≥ 2.5·yarıçap olacak şekilde parça sayısını sınırlar.
fn limit(mut a: Antenna) -> Antenna {
    let lambda = C0 / a.frequency;
    for w in &mut a.wires {
        let m = w.segment_count(lambda);
        let max = ((w.length() / (2.5 * w.radius())).floor() as usize).max(3);
        if m > max {
            w.segments = max;
        }
    }
    a
}

fn main() {
    println!("--- yalıtılmış dipol: rezonans boyu (X = 0) çap/λ'ya göre");
    let f = 300e6;
    let lambda = C0 / f;
    for lim in [false, true] {
        for dl in [0.001, 0.0048, 0.0144, 0.0432] {
            let t = Tube::aluminum(dl * lambda, None);
            let x = |k: f64| {
                let a = dipole(f, k, &t);
                let a = if lim { limit(a) } else { a };
                solver::solve(&a, f).unwrap().impedance().im
            };
            let (mut a, mut b) = (0.30, 0.50);
            for _ in 0..40 {
                let m = 0.5 * (a + b);
                if x(m) > 0.0 { b = m } else { a = m }
            }
            println!("    {} d = {dl:.4} λ: rezonans boyu {:.4} λ", if lim { "sınırlı " } else { "varsayılan" }, 0.5 * (a + b));
        }
    }

    println!("--- 23 cm 9 eleman Ø10 mm: bütün boylar × k (sınırlı parça)");
    let f = 1296.2e6;
    for k in [1.0, 0.96, 0.94, 0.92, 0.90, 0.88] {
        let mut d = YagiDesign::standard(f, 9, Profile::uniform(Tube::aluminum(0.010, Some(0.001))));
        for e in &mut d.elements {
            e.length *= k;
        }
        let s = solver::solve(&limit(d.build()), f).unwrap();
        let p = Pattern::compute(&s, 2.0).summary(&s);
        let z = s.impedance();
        println!("    k = {k:.2}: Z = {:6.1} {:+6.1}j  SWR {:6.2}  G {:5.2} dBi  Ö/A {:4.1} dB", z.re, z.im, swr(z, 50.0), p.max_gain_dbi, p.front_to_back_db);
    }
}
