//! Standart Yagi şablonunun farklı bantlardaki SWR'si ve yalnız boy/aralık
//! değiştirerek 50 Ω'a ulaşılıp ulaşılamadığı.
//! `cargo run --release -p antan-core --example swr_bands`

use antan_core::farfield::Pattern;
use antan_core::matching::swr;
use antan_core::optimize::{Goals, optimize};
use antan_core::solver;
use antan_core::templates::Tube;
use antan_core::yagi::{Profile, YagiDesign};
use antan_core::C0;

fn report(tag: &str, d: &YagiDesign) {
    let s = solver::solve(&d.build(), d.frequency).unwrap();
    let p = Pattern::compute(&s, 2.0).summary(&s);
    let z = s.impedance();
    println!("    {tag:<34} Z = {:6.1} {:+6.1}j Ω  SWR {:5.2}  G {:5.2} dBi  Ö/A {:4.1} dB", z.re, z.im, swr(z, 50.0), p.max_gain_dbi, p.front_to_back_db);
}

fn main() {
    let tube = Tube::aluminum(0.010, Some(0.001));
    for (f, label) in [(144.3e6, "2 m"), (432.2e6, "70 cm"), (1296.2e6, "23 cm")] {
        for n in [5usize, 9] {
            let lambda = C0 / f;
            println!("=== {label} {n} eleman, Ø10 mm = {:.4} λ", 0.010 / lambda);
            let d = YagiDesign::standard(f, n, Profile::uniform(tube.clone()));
            report("şablon", &d);
            if let Some((t, _)) = d.tune_driven(|_| Some(0.0)) {
                report("sürülen eleman rezonansa", &t);
            }
            let g = Goals { w_swr: 4.0, ..Goals::default() };
            let (o, _) = optimize(&d, &g, 400, |_, _, _| true);
            report("iyileştirici (boy+aralık, SWR×4)", &o);
            let g2 = Goals { w_gain: 0.5, w_swr: 20.0, ..Goals::default() };
            let (o2, _) = optimize(&d, &g2, 400, |_, _, _| true);
            report("iyileştirici (SWR öncelikli)", &o2);
        }
    }
}
