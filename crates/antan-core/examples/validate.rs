//! Karşılaştırma (doğrulama) için deney kümesi: her anten için AntAn sonuçlarını
//! ve modeli JSON olarak, NEC-2 kart dosyasıyla birlikte yazar. Aynı modeller
//! `tools/validation/compare.py` ile NEC-2 (nec2c) ve MININEC (pymininec) ile
//! hesaplanıp karşılaştırılır.
//!
//! `cargo run --release -p antan-core --example validate -- <çıktı klasörü>`

use std::f64::consts::PI;
use std::fmt::Write as _;

use antan_core::farfield::{direction, gain, to_db};
use antan_core::geometry::{Antenna, Ground};
use antan_core::magloop::MagLoop;
use antan_core::solver::{self, Solution};
use antan_core::stack::{self, Stack};
use antan_core::templates::{self, Kind, Tube};
use antan_core::yagi::{Profile, YagiDesign};
use antan_core::{C0, Material};

/// Desen 2° ızgarada; en büyük kazanç, yönü ve ön/arka (serbest uzayda tam ters
/// yön, zeminde aynı yükseliş ters azimut — EZNEC/NEC kullanımıyla aynı).
fn pattern(sol: &Solution, grounded: bool) -> (f64, f64, f64, f64) {
    let step = 2.0_f64;
    let nt = if grounded { 46 } else { 91 };
    let (mut best, mut bt, mut bp) = (0.0, 0.0, 0.0);
    for it in 0..nt {
        let t = (it as f64 * step).to_radians();
        for ip in 0..180 {
            let p = (ip as f64 * step).to_radians();
            let g = gain(sol, t, p);
            if g > best {
                (best, bt, bp) = (g, t, p);
            }
        }
    }
    let back = if grounded { gain(sol, bt, bp + PI) } else { gain(sol, PI - bt, bp + PI) };
    let _ = direction(bt, bp);
    (to_db(best), bt.to_degrees(), bp.to_degrees(), to_db(best) - to_db(back))
}

fn lossless(mut a: Antenna) -> Antenna {
    for w in &mut a.wires {
        w.material = Material::perfect();
    }
    a
}

fn yagi(f: f64, n: usize, d: f64) -> Antenna {
    let p = Profile::uniform(Tube { diameter: d, wall: None, material: Material::perfect() });
    let y = YagiDesign::standard(f, n, p);
    let y = y.tune_driven(|_| Some(0.0)).map(|(t, _)| t).unwrap_or(y);
    // Bum düzeltmesi tasarım boyuna dahil; karşılaştırmada elektriksel boylar kullanılır.
    y.build()
}

struct Case {
    id: &'static str,
    title_en: &'static str,
    title_tr: &'static str,
    antenna: Antenna,
}

fn cases() -> Vec<Case> {
    let pec = |d: f64| Tube { diameter: d, wall: None, material: Material::perfect() };
    let cu = |d: f64| Tube { diameter: d, wall: None, material: Material::copper() };
    let mut v = Vec::new();
    let mut push = |id, en, tr, a: Antenna| v.push(Case { id, title_en: en, title_tr: tr, antenna: a });

    push("dipole", "Half-wave dipole, 14.2 MHz, Ø2 mm, free space", "Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay", templates::dipole(14.2e6, 0.485, &pec(0.002)));
    push("dipole_cu", "Same dipole, copper wire (loss)", "Aynı dipol, bakır tel (kayıplı)", templates::dipole(14.2e6, 0.485, &cu(0.002)));
    let mut d = templates::dipole(14.2e6, 0.485, &pec(0.002));
    d.ground = Ground::Perfect;
    push("dipole_pg", "Dipole 10 m over perfect ground", "Dipol, kusursuz zeminden 10 m yüksekte", antan_core::scan::at_height(&d, 10.0));
    let mut d = templates::dipole(14.2e6, 0.485, &pec(0.002));
    d.ground = Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    push("dipole_rg", "Dipole 10 m over average ground (εr 13, σ 0.005)", "Dipol, ortalama topraktan 10 m yüksekte (εr 13, σ 0,005)", antan_core::scan::at_height(&d, 10.0));
    push("invv", "Inverted V 7.1 MHz, 110°, Ø2 mm", "Ters V 7,1 MHz, 110°, Ø2 mm", lossless(templates::inverted_v(7.1e6, 110.0, &pec(0.002))));
    push("folded", "Folded dipole 144.3 MHz, Ø4 mm", "Katlanmış dipol 144,3 MHz, Ø4 mm", templates::folded_dipole(144.3e6, 0.02, &pec(0.004)));
    let mut gv = Kind::GroundVertical.build(14.2e6, &pec(0.01));
    gv.ground = Ground::Perfect;
    push("vertical_pg", "λ/4 vertical on perfect ground, 14.2 MHz, Ø10 mm", "λ/4 dikey, kusursuz zemin, 14,2 MHz, Ø10 mm", gv);
    push("gp", "Ground plane, 4 radials 45° down, 145 MHz", "Ground plane, 4 radyal 45° eğik, 145 MHz", templates::ground_plane(145e6, 45.0, &pec(0.008)));
    push("quad2", "2-element quad 144.3 MHz, Ø4 mm", "2 elemanlı quad 144,3 MHz, Ø4 mm", templates::quad(144.3e6, 2, &pec(0.004)));
    push("delta", "Delta loop 14.2 MHz, Ø4 mm", "Delta loop 14,2 MHz, Ø4 mm", templates::delta_loop(14.2e6, &pec(0.004)));
    push("moxon", "Moxon rectangle 144.3 MHz, Ø4 mm", "Moxon dikdörtgeni 144,3 MHz, Ø4 mm", templates::moxon(144.3e6, &pec(0.004)));
    push("yagi3", "3-element Yagi 144.3 MHz, Ø10 mm", "3 elemanlı Yagi 144,3 MHz, Ø10 mm", yagi(144.3e6, 3, 0.010));
    push("yagi5", "5-element Yagi 144.3 MHz, Ø10 mm", "5 elemanlı Yagi 144,3 MHz, Ø10 mm", yagi(144.3e6, 5, 0.010));
    push("yagi7_70", "7-element Yagi 432.2 MHz, Ø6 mm", "7 elemanlı Yagi 432,2 MHz, Ø6 mm", yagi(432.2e6, 7, 0.006));
    let lfa = YagiDesign::lfa(144.3e6, 7, Profile::uniform(pec(0.010)));
    push("lfa7", "7-element LFA Yagi 144.3 MHz, Ø10 mm", "7 elemanlı LFA Yagi 144,3 MHz, Ø10 mm", lfa.build());
    push("loaded", "Coil-loaded dipole, half size, 7.1 MHz (2 coils, Q 200)", "Bobinle kısaltılmış dipol, yarı boy, 7,1 MHz (2 bobin, Q 200)", lossless(antan_core::loading::loaded_dipole(7.1e6, 0.24, &pec(0.002))));
    // Manyetik loop: doğrudan besleme (halkanın kendi direnci) ve kuplaj halkasıyla.
    let f = 7.1e6;
    let mut m = MagLoop { coupling_ratio: 0.0, cap_esr: 0.0, ..Default::default() };
    if let Ok(tp) = m.two_port(f, Ground::FreeSpace) {
        m.capacitance = m.tune(&tp, 50.0).unwrap_or(m.capacitance);
    }
    push("magloop_direct", "Magnetic loop Ø1 m, Ø22 mm copper, 7.1 MHz, direct feed", "Manyetik loop Ø1 m, Ø22 mm bakır, 7,1 MHz, doğrudan besleme", m.build(f));
    let m = MagLoop { cap_esr: 0.0, ..Default::default() };
    let (m, _) = m.match_coupling(f, 50.0, Ground::FreeSpace).unwrap();
    push("magloop", "Magnetic loop with coupling loop (matched in AntAn)", "Kuplaj halkalı manyetik loop (AntAn'da eşlenmiş)", m.build(f));
    let one = yagi(144.3e6, 5, 0.010);
    let lambda = C0 / 144.3e6;
    push("stack2", "Two 5-el. Yagis stacked vertically, 1.25 λ", "İki 5 el. Yagi dikey istif, 1,25 λ", stack::apply(&one, &Stack { rows: 2, cols: 1, v_spacing: 1.25 * lambda, h_spacing: 0.0 }));
    push("stack4", "Four 5-el. Yagis, 2 × 2, 1.25 λ", "Dört 5 el. Yagi, 2 × 2, 1,25 λ", stack::apply(&one, &Stack { rows: 2, cols: 2, v_spacing: 1.25 * lambda, h_spacing: 1.25 * lambda }));
    v
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "validation-out".into());
    std::fs::create_dir_all(&out).unwrap();
    let mut index = String::from("[\n");
    // CASES=dipole,yagi5 ile yalnız seçilenler.
    let only: Option<Vec<String>> = std::env::var("CASES").ok().map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let list: Vec<Case> = cases().into_iter().filter(|c| only.as_ref().map(|o| o.iter().any(|x| x == c.id)).unwrap_or(true)).collect();
    for (k, c) in list.iter().enumerate() {
        let mut a = c.antenna.clone();
        let f = a.frequency;
        let lambda = C0 / f;
        // Parça sayıları açıkça yazılsın: diğer programlar aynısını kullanır.
        // SEGMULT=2|4 ile yakınsama denemesi için çoğaltılır (çift kalır).
        let mult: usize = std::env::var("SEGMULT").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        for w in &mut a.wires {
            w.segments = w.segment_count(lambda) * mult;
        }
        let t0 = std::time::Instant::now();
        let sol = solver::solve(&a, f).expect(c.id);
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let grounded = !a.ground.is_free();
        let (g, th, ph, fb) = pattern(&sol, grounded);
        let feeds: Vec<String> = sol.feeds.iter().map(|fd| format!("[{:.6}, {:.6}]", fd.impedance.re, fd.impedance.im)).collect();
        let src_pos: Vec<String> = sol.feeds.iter().map(|fd| format!("[{:.6}, {:.6}, {:.6}]", fd.pos[0], fd.pos[1], fd.pos[2])).collect();
        let load_pos: Vec<String> = sol.loads.iter().map(|l| format!("[{:.6}, {:.6}, {:.6}]", l.pos[0], l.pos[1], l.pos[2])).collect();
        // Rezonans kayması için tarama: loop'ta dar (yüksek Q), diğerlerinde ±%5.
        let (span, n) = if c.id.starts_with("magloop") { (0.012, 481) } else { (0.05, 101) };
        let sw = solver::sweep(&a, f * (1.0 - span), f * (1.0 + span), n).unwrap_or_default();
        let sweep_json: Vec<String> = sw.iter().map(|p| format!("[{:.3}, {:.6}, {:.6}]", p.frequency, p.impedance.re, p.impedance.im)).collect();
        let nec = antan_core::nec::export(&a, None);
        std::fs::write(format!("{out}/{}.nec", c.id), nec).unwrap();
        let mut s = String::new();
        let _ = write!(
            s,
            "{{\"id\": \"{}\", \"title_en\": \"{}\", \"title_tr\": \"{}\", \"frequency\": {f}, \"unknowns\": {}, \"ms\": {ms:.2}, \"antan\": {{\"feeds\": [{}], \"gain_dbi\": {g:.4}, \"theta\": {th}, \"phi\": {ph}, \"fb_db\": {fb:.4}, \"efficiency\": {:.6}, \"sweep\": [{}]}}, \"sweep_span\": {span}, \"sweep_n\": {n}, \"source_pos\": [{}], \"load_pos\": [{}], \"model\": {}}}",
            c.id,
            c.title_en,
            c.title_tr,
            sol.basis.len(),
            feeds.join(", "),
            sol.efficiency(),
            sweep_json.join(", "),
            src_pos.join(", "),
            load_pos.join(", "),
            serde_json::to_string(&a).unwrap()
        );
        index.push_str(&s);
        index.push_str(if k + 1 < list.len() { ",\n" } else { "\n" });
        println!("{:16} {:5} bilinmeyen  Z = {}  G {g:6.2} dBi  Ö/A {fb:5.1}", c.id, sol.basis.len(), feeds.join(" "));
    }
    index.push(']');
    std::fs::write(format!("{out}/cases.json"), index).unwrap();
}
