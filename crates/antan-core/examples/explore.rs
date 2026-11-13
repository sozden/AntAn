//! Şablon boyutlarını ayarlamak için keşif aracı: `cargo run --release -p antan-core --example explore`

use antan_core::farfield::Pattern;
use antan_core::templates::{self, Tube};
use antan_core::{Material, solver};

fn main() {
    let perfect = |d: f64| Tube { diameter: d, wall: None, material: Material::perfect() };

    println!("--- katlanmış dipol, parça yoğunluğu ---");
    let f = 300.0e6;
    for segs in [20usize, 40, 80, 160] {
        let mut fd = templates::folded_dipole(f, 0.02, &perfect(0.002));
        fd.wires[0].segments = segs;
        fd.wires[2].segments = segs;
        fd.wires[1].segments = 4;
        fd.wires[3].segments = 4;
        let z = solver::solve(&fd, f).unwrap().impedance();
        println!("{segs:4} parça: {z:.1}");
    }

    let f = 144.3e6;
    let lambda = antan_core::C0 / f;
    let t = Tube::aluminum(0.004, None);
    println!("--- tek kare halka, çevre taraması ---");
    for p in [0.98, 1.00, 1.02, 1.04, 1.06, 1.08] {
        let mut a = templates::quad(f, 2, &t);
        a.wires.drain(0..4);
        let h = p * lambda / 8.0;
        let x = a.wires[0].start[0];
        let c = |y: f64, z: f64| [x, y * h, z * h];
        let pts = [c(-1.0, -1.0), c(1.0, -1.0), c(1.0, 1.0), c(-1.0, 1.0)];
        for i in 0..4 {
            a.wires[i].start = pts[i];
            a.wires[i].end = pts[(i + 1) % 4];
        }
        a.sources[0].wire = 0;
        let z = solver::solve(&a, f).unwrap().impedance();
        println!("çevre {p:.2}λ: {z:.1}");
    }

    println!("--- 2 el. quad: sürülen/yansıtıcı çevresi, aralık ---");
    for (pd, pr, sp) in [(1.03, 1.07, 0.14), (1.04, 1.08, 0.14), (1.04, 1.09, 0.15), (1.05, 1.10, 0.15), (1.04, 1.10, 0.12)] {
        let a = quad_custom(f, &t, pd, pr, sp);
        let sol = solver::solve(&a, f).unwrap();
        let s = Pattern::compute(&sol, 3.0).summary(&sol);
        println!("Ps={pd} Pr={pr} s={sp}: Z={:.1} G={:.2} F/B={:.1}", sol.impedance(), s.max_gain_dbi, s.front_to_back_db);
    }

    println!("--- Moxon ---");
    let sol = solver::solve(&templates::moxon(f, &t), f).unwrap();
    let s = Pattern::compute(&sol, 3.0).summary(&sol);
    println!("Z={:.1} G={:.2} F/B={:.1}", sol.impedance(), s.max_gain_dbi, s.front_to_back_db);
    for f2 in [142.0e6, 143.0e6, 144.0e6, 145.0e6, 146.0e6, 147.0e6] {
        let sol = solver::solve(&templates::moxon(f, &t), f2).unwrap();
        let s = Pattern::compute(&sol, 3.0).summary(&sol);
        println!("  {:.0} MHz: Z={:.1} G={:.2} F/B={:.1}", f2 / 1e6, sol.impedance(), s.max_gain_dbi, s.front_to_back_db);
    }

    println!("--- delta ---");
    let sol = solver::solve(&templates::delta_loop(f, &t), f).unwrap();
    println!("Z={:.1}", sol.impedance());

    println!("--- enerji korunumu (kazanç − yönlülük, kayıpsız) ---");
    let pt = perfect(0.004);
    for a in [templates::quad(f, 2, &pt), templates::moxon(f, &pt), templates::ground_plane(f, 45.0, &pt), templates::folded_dipole(f, 0.02, &pt), templates::delta_loop(f, &pt)] {
        let sol = solver::solve(&a, f).unwrap();
        let s = Pattern::compute(&sol, 2.0).summary(&sol);
        println!("{:20}: G−D = {:+.3} dB", a.name, s.max_gain_dbi - s.directivity_dbi);
    }

    println!("--- katlanmış dipol: eşdeğer yarıçap modeli ---");
    let f3 = 300.0e6;
    let (a, s) = (0.001f64, 0.02f64);
    let ae = (a * s).sqrt();
    let zd = solver::solve(&templates::dipole(f3, 0.48, &perfect(2.0 * ae)), f3).unwrap().impedance();
    let z0 = 120.0 * (s / (2.0 * a)).acosh();
    let zt = num_complex::Complex64::new(0.0, z0 * (2.0 * std::f64::consts::PI * 0.24).tan());
    let model = (4.0 * zd) * (2.0 * zt) / (4.0 * zd + 2.0 * zt);
    println!("kalın dipol {zd:.1}, model {model:.1}");

    println!("--- quad ızgara ---");
    let mut best = Vec::new();
    for pd in [1.00, 1.01, 1.02, 1.03, 1.04] {
        for pr in [1.06, 1.08, 1.10, 1.12] {
            for sp in [0.10, 0.12, 0.14, 0.16] {
                let a = quad_custom(f, &t, pd, pr, sp);
                let sol = solver::solve(&a, f).unwrap();
                let s = Pattern::compute(&sol, 4.0).summary(&sol);
                let z = sol.impedance();
                let swr = antan_core::matching::swr(z, 50.0);
                best.push((s.front_to_back_db.min(25.0) + s.max_gain_dbi * 2.0 - 4.0 * swr, pd, pr, sp, z, s.max_gain_dbi, s.front_to_back_db, swr));
            }
        }
    }
    best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for b in best.iter().take(5) {
        println!("Ps={} Pr={} s={}: Z={:.1} G={:.2} F/B={:.1} SWR={:.2}", b.1, b.2, b.3, b.4, b.5, b.6, b.7);
    }

    println!("--- delta çevre ---");
    for p in [1.02, 1.06, 1.10, 1.14] {
        let mut a = templates::delta_loop(f, &t);
        let side = p * lambda / 3.0;
        let h = side * 3f64.sqrt() / 2.0;
        let (pa, pb, pc) = ([0.0, -0.5 * side, 0.0], [0.0, 0.5 * side, 0.0], [0.0, 0.0, h]);
        a.wires[0].start = pa;
        a.wires[0].end = pb;
        a.wires[1].start = pb;
        a.wires[1].end = pc;
        a.wires[2].start = pc;
        a.wires[2].end = pa;
        println!("{p}: {:.1}", solver::solve(&a, f).unwrap().impedance());
    }

    println!("--- Moxon ölçek ---");
    for k in [1.000, 1.008, 1.012, 1.016, 1.02] {
        let mut a = templates::moxon(f, &t);
        for w in &mut a.wires {
            for i in 0..3 {
                w.start[i] *= k;
                w.end[i] *= k;
            }
        }
        let sol = solver::solve(&a, f).unwrap();
        let s = Pattern::compute(&sol, 3.0).summary(&sol);
        println!("{k}: Z={:.1} G={:.2} F/B={:.1}", sol.impedance(), s.max_gain_dbi, s.front_to_back_db);
    }
}

fn quad_custom(f: f64, t: &Tube, pd: f64, pr: f64, sp: f64) -> antan_core::Antenna {
    let lambda = antan_core::C0 / f;
    let mut a = templates::quad(f, 2, t);
    let set = |a: &mut antan_core::Antenna, off: usize, x: f64, p: f64| {
        let h = p * lambda / 8.0;
        let c = |y: f64, z: f64| [x, y * h, z * h];
        let pts = [c(-1.0, -1.0), c(1.0, -1.0), c(1.0, 1.0), c(-1.0, 1.0)];
        for i in 0..4 {
            a.wires[off + i].start = pts[i];
            a.wires[off + i].end = pts[(i + 1) % 4];
        }
    };
    set(&mut a, 0, 0.0, pr);
    set(&mut a, 4, sp * lambda, pd);
    a
}
