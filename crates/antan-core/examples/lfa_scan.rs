//! LFA (halka beslemeli Yagi) başlangıç boyutlarını taramak için:
//! `cargo run --release -p antan-core --example lfa_scan`

use antan_core::C0;
use antan_core::farfield::Pattern;
use antan_core::solver;
use antan_core::templates::Tube;
use antan_core::yagi::{Profile, YagiDesign};

fn main() {
    let f = 144.3e6;
    let lambda = C0 / f;
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let mut rows = Vec::new();
    for dx in [0.06, 0.08, 0.10, 0.12, 0.14, 0.16] {
        for ll in [0.38, 0.40, 0.42, 0.44, 0.46, 0.48] {
            for w in [0.04, 0.06, 0.08, 0.10, 0.12, 0.14] {
                for d1 in [0.22, 0.26, 0.30] {
                    if d1 - dx - 0.5 * w < 0.03 || dx - 0.5 * w < 0.03 {
                        continue;
                    }
                    let mut d = YagiDesign::standard(f, n, Profile::uniform(Tube::aluminum(0.010, Some(0.001))));
                    d.elements[1].x = dx * lambda;
                    d.elements[1].length = ll * lambda;
                    d.driven_loop = Some(w * lambda);
                    let shift = d1 * lambda - d.elements[2].x;
                    for e in d.elements.iter_mut().skip(2) {
                        e.x += shift;
                    }
                    let Ok(s) = solver::solve(&d.build(), f) else { continue };
                    let z = s.impedance();
                    let p = Pattern::compute(&s, 5.0).summary(&s);
                    rows.push((dx, ll, w, d1, z, p.max_gain_dbi, p.front_to_back_db));
                }
            }
        }
    }
    // 50 Ω'a yakın ve rezonansa yakın olanlar, sonra Ö/A.
    rows.retain(|r| (r.4.re - 50.0).abs() < 12.0 && r.4.im.abs() < 15.0);
    rows.sort_by(|a, b| (b.6.min(25.0) + b.5).partial_cmp(&(a.6.min(25.0) + a.5)).unwrap());
    for r in rows.iter().take(20) {
        println!("dx {:.2} L {:.2} w {:.2} d1 {:.2}  Z {:.1}{:+.1}j  G {:.2} dBi  F/B {:.1} dB", r.0, r.1, r.2, r.3, r.4.re, r.4.im, r.5, r.6);
    }
    println!("{} aday", rows.len());
}
