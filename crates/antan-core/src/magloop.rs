//! Manyetik loop (küçük verici halka): kalın borudan çokgen halka, tepesinde
//! ayar kondansatörü, altında küçük kuplaj halkası (Faraday halkası).
//!
//! Hesap yaklaşımı: ana halka ve kuplaj halkası birlikte, karşılıklı etkileriyle
//! moment yöntemiyle çözülür. Kondansatörün bulunduğu nokta ikinci kapı sayılır;
//! iki uyarımla (besleme ve kondansatör kapısı) iki kapılı ağın Y matrisi bulunur.
//! Böylece her kondansatör değeri için giriş empedansı yeniden çözmeden, tam
//! olarak hesaplanır: ayar ve eşleme anında yapılır.

use std::f64::consts::PI;

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::geometry::{Antenna, Load, LoadKind, Source, Wire};
use crate::material::Material;
use crate::solver::{self, SolveError};
use crate::templates::{Tube, wire};
use crate::C0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MagLoop {
    /// Ana halkanın çapı (m), köşelerden geçen daire.
    pub diameter: f64,
    /// Çokgen kenar sayısı (çift; 8 = sekizgen, 16+ ≈ daire).
    pub sides: usize,
    pub tube: Tube,
    /// Kuplaj halkası çapının ana halkaya oranı; 0 = doğrudan besleme (yalnız analiz).
    pub coupling_ratio: f64,
    /// Kuplaj halkasının tel çapı (m), bakır.
    pub coupling_wire: f64,
    /// Kuplaj halkası ile ana halkanın alt kenarı arasındaki açıklık (m).
    pub coupling_gap: f64,
    /// Ayar kondansatörü (F).
    pub capacitance: f64,
    /// Kondansatörün seri kayıp direnci (Ω). Kelebek/vakum ≈ 0.005–0.01, döner plakalı ≈ 0.02–0.1.
    pub cap_esr: f64,
    /// Halkanın alt noktasının z konumu (m); zemin varsa yükseklik.
    pub bottom: f64,
}

impl Default for MagLoop {
    fn default() -> Self {
        Self {
            diameter: 1.0,
            sides: 16,
            tube: Tube { diameter: 0.022, wall: Some(0.001), material: Material::copper() },
            coupling_ratio: 0.2,
            coupling_wire: 0.004,
            coupling_gap: 0.01,
            capacitance: 100e-12,
            cap_esr: 0.01,
            bottom: 0.0,
        }
    }
}

/// İki kapılı ağın admitans matrisi (kapı 1 besleme, kapı 2 kondansatör).
#[derive(Clone, Copy, Debug)]
pub struct TwoPort {
    pub frequency: f64,
    pub y11: Complex64,
    pub y12: Complex64,
    pub y21: Complex64,
    pub y22: Complex64,
}

impl TwoPort {
    /// Kapı 2'ye `zl` yükü bağlıyken kapı 1'den görülen empedans.
    pub fn input(&self, zl: Complex64) -> Complex64 {
        let yin = self.y11 - self.y12 * self.y21 * zl / (1.0 + self.y22 * zl);
        1.0 / yin
    }
    /// Kapı 1'e `zs` bağlıyken kapı 2'den (kondansatörün yerinden) görülen empedans.
    pub fn seen_by_cap(&self, zs: Complex64) -> Complex64 {
        let yout = self.y22 - self.y12 * self.y21 * zs / (1.0 + self.y11 * zs);
        1.0 / yout
    }
}

/// Kondansatörün empedansı (ESR dahil).
pub fn cap_impedance(c: f64, esr: f64, f: f64) -> Complex64 {
    Complex64::new(esr, -1.0 / (2.0 * PI * f * c))
}

/// Halka için çift kenar sayısı: istenen `wanted`, ancak her kenar iki parçaya
/// (parça ≥ 2·tel yarıçapı = tel çapı) bölünebilsin diye en çok çevre / (2·çap); en az `min`.
fn polygon_sides(wanted: usize, ring_radius: f64, wire_diameter: f64, min: usize) -> usize {
    // %5 pay: çokgen kenarı 2πR/n'den biraz kısadır.
    let limit = (2.0 * PI * ring_radius / (2.1 * wire_diameter.max(1e-6))).floor() as usize;
    let n = wanted.max(4).min(limit).max(min);
    n + n % 2
}

impl MagLoop {
    pub fn radius(&self) -> f64 {
        0.5 * self.diameter
    }
    /// Kenar sayısı (çift). Kenar en az iki parçaya bölünebilecek kadar uzun kalsın diye
    /// (parça ≥ 2·yarıçap) kalın boruda azaltılır, ama sekizgenin altına inmez.
    fn sides(&self) -> usize {
        polygon_sides(self.sides, self.radius(), self.tube.diameter, 8)
    }
    /// Kondansatörün takıldığı (üst kenar) tel sırası.
    pub fn cap_wire(&self) -> usize {
        self.sides() / 2 - 1
    }
    pub fn coupled(&self) -> bool {
        self.coupling_ratio > 0.0
    }
    /// Ana halkanın çevresi (m).
    pub fn circumference(&self) -> f64 {
        let n = self.sides() as f64;
        n * self.diameter * (PI / n).sin()
    }

    /// xz düzleminde (dikey) halka; alt ve üst kenar düz.
    fn polygon(name: &str, r: f64, zc: f64, n: usize, t: &Tube) -> Vec<Wire> {
        let p = |k: usize| {
            let a = -PI / 2.0 + PI / n as f64 + 2.0 * PI * k as f64 / n as f64;
            [r * a.cos(), 0.0, zc + r * a.sin()]
        };
        (0..n).map(|k| wire(&format!("{name} {}", k + 1), p(k), p((k + 1) % n), t)).collect()
    }

    /// Kondansatörsüz geometri ve iki kaynak: [besleme, kondansatör kapısı].
    fn geometry(&self, f: f64) -> Antenna {
        let n = self.sides();
        let r = self.radius();
        let apothem = r * (PI / n as f64).cos();
        let zc = self.bottom + apothem;
        let mut wires = Self::polygon(crate::i18n::t("Loop", "Halka"), r, zc, n, &self.tube);
        let feed = if self.coupled() {
            let rc = (r * self.coupling_ratio).max(0.01);
            let m = polygon_sides(12, rc, self.coupling_wire, 6);
            let ka = rc * (PI / m as f64).cos();
            let small = Tube { diameter: self.coupling_wire, wall: None, material: Material::copper() };
            // Alt kenarlar arasında `coupling_gap` açıklık (yüzeyden yüzeye).
            let gap = self.coupling_gap.max(0.0) + 0.5 * (self.tube.diameter + self.coupling_wire);
            let zk = self.bottom + gap + ka;
            wires.extend(Self::polygon(crate::i18n::t("Coupling", "Kuplaj"), rc, zk, m, &small));
            n + m - 1
        } else {
            n - 1
        };
        let sources = vec![
            Source { wire: feed, position: 0.5, voltage: Complex64::new(1.0, 0.0) },
            Source { wire: self.cap_wire(), position: 0.5, voltage: Complex64::new(0.0, 0.0) },
        ];
        Antenna::new(crate::i18n::t("Magnetic loop", "Manyetik loop"), f, wires, sources)
    }

    /// Tam anten modeli: kondansatör yük olarak takılı, tek besleme.
    pub fn build(&self, f: f64) -> Antenna {
        let mut a = self.geometry(f);
        a.sources.truncate(1);
        a.loads.push(Load { wire: self.cap_wire(), position: 0.5, kind: LoadKind::SeriesRlc { r: self.cap_esr, l: 0.0, c: self.capacitance } });
        a
    }

    /// İki uyarımla iki kapılı Y matrisi (kondansatörsüz geometri). `ground` modele uygulanır.
    pub fn two_port(&self, f: f64, ground: crate::geometry::Ground) -> Result<TwoPort, SolveError> {
        let mut a = self.geometry(f);
        a.ground = ground;
        let s1 = solver::solve(&a, f)?;
        a.sources[0].voltage = Complex64::new(0.0, 0.0);
        a.sources[1].voltage = Complex64::new(1.0, 0.0);
        let s2 = solver::solve(&a, f)?;
        Ok(TwoPort { frequency: f, y11: s1.feeds[0].current, y21: s1.feeds[1].current, y12: s2.feeds[0].current, y22: s2.feeds[1].current })
    }

    /// `f` frekansında en iyi kondansatör değeri: kuplajlı halkada en düşük SWR,
    /// doğrudan beslemede X = 0. Önce kondansatör kapısından görülen reaktansla
    /// rezonans bulunur, sonra çevresinde SWR en küçüklenir.
    pub fn tune(&self, tp: &TwoPort, z0: f64) -> Option<f64> {
        let f = tp.frequency;
        let w = 2.0 * PI * f;
        let zs = if self.coupled() { Complex64::new(z0, 0.0) } else { Complex64::new(0.0, 0.0) };
        let x = tp.seen_by_cap(zs).im;
        if x <= 0.0 {
            return None; // halka kendi rezonansının üstünde: kondansatörle ayarlanamaz
        }
        let c0 = 1.0 / (w * x);
        let cost = |c: f64| {
            let z = tp.input(cap_impedance(c, self.cap_esr, f));
            if self.coupled() { crate::matching::reflection(z, z0).norm() } else { z.im.abs() / z.norm().max(1e-12) }
        };
        // Yoğun tarama (kapalı biçim, ucuz) ve altın oranla daraltma.
        let (lo, hi) = (c0 * 0.97, c0 * 1.03);
        let n = 3000;
        let mut best = (f64::MAX, c0);
        for i in 0..=n {
            let c = lo + (hi - lo) * i as f64 / n as f64;
            let v = cost(c);
            if v < best.0 {
                best = (v, c);
            }
        }
        let step = (hi - lo) / n as f64;
        let (mut a, mut b) = (best.1 - step, best.1 + step);
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..60 {
            let (c1, c2) = (b - g * (b - a), a + g * (b - a));
            if cost(c1) < cost(c2) { b = c2 } else { a = c1 }
        }
        Some(0.5 * (a + b))
    }

    /// Kuplaj halkasının boyunu (ve açıklığını) en düşük SWR için ayarlar.
    /// Dönüş: (yeni tasarım, SWR).
    pub fn match_coupling(&self, f: f64, z0: f64, ground: crate::geometry::Ground) -> Option<(MagLoop, f64)> {
        let eval = |ratio: f64, gap: f64| -> Option<(f64, f64)> {
            let mut m = self.clone();
            m.coupling_ratio = ratio;
            m.coupling_gap = gap;
            let tp = m.two_port(f, ground).ok()?;
            let c = m.tune(&tp, z0)?;
            let z = tp.input(cap_impedance(c, m.cap_esr, f));
            Some((crate::matching::swr(z, z0), c))
        };
        // Önce oran (açıklık sabit) için kaba tarama, sonra iki boyutta küçük Nelder–Mead.
        let gap0 = self.coupling_gap.max(0.002);
        let mut best = (f64::MAX, 0.2, gap0, self.capacitance);
        for i in 0..=16 {
            let r = 0.08 + 0.27 * i as f64 / 16.0;
            if let Some((s, c)) = eval(r, gap0) {
                if s < best.0 {
                    best = (s, r, gap0, c);
                }
            }
        }
        let mut simplex = [(best.1, best.2), (best.1 * 1.08, best.2), (best.1, best.2 * 1.6 + 0.003)];
        let score = |p: (f64, f64)| if p.0 < 0.04 || p.0 > 0.6 || p.1 < 0.0 || p.1 > 0.3 * self.diameter { f64::MAX } else { eval(p.0, p.1).map(|v| v.0).unwrap_or(f64::MAX) };
        let mut vals = simplex.map(score);
        for _ in 0..40 {
            let mut idx = [0, 1, 2];
            idx.sort_by(|&i, &j| vals[i].total_cmp(&vals[j]));
            let (b, m, w) = (idx[0], idx[1], idx[2]);
            let cen = (0.5 * (simplex[b].0 + simplex[m].0), 0.5 * (simplex[b].1 + simplex[m].1));
            let refl = (2.0 * cen.0 - simplex[w].0, 2.0 * cen.1 - simplex[w].1);
            let vr = score(refl);
            if vr < vals[b] {
                let exp = (3.0 * cen.0 - 2.0 * simplex[w].0, 3.0 * cen.1 - 2.0 * simplex[w].1);
                let ve = score(exp);
                (simplex[w], vals[w]) = if ve < vr { (exp, ve) } else { (refl, vr) };
            } else if vr < vals[m] {
                (simplex[w], vals[w]) = (refl, vr);
            } else {
                let con = (0.5 * (cen.0 + simplex[w].0), 0.5 * (cen.1 + simplex[w].1));
                let vc = score(con);
                if vc < vals[w] {
                    (simplex[w], vals[w]) = (con, vc);
                } else {
                    for i in [m, w] {
                        simplex[i] = (0.5 * (simplex[i].0 + simplex[b].0), 0.5 * (simplex[i].1 + simplex[b].1));
                        vals[i] = score(simplex[i]);
                    }
                }
            }
            if vals.iter().cloned().fold(f64::MIN, f64::max) - vals.iter().cloned().fold(f64::MAX, f64::min) < 1e-4 {
                break;
            }
        }
        let bi = (0..3).min_by(|&i, &j| vals[i].total_cmp(&vals[j]))?;
        let (ratio, gap) = if vals[bi] < best.0 { simplex[bi] } else { (best.1, best.2) };
        let (s, c) = eval(ratio, gap)?;
        let mut m = self.clone();
        m.coupling_ratio = ratio;
        m.coupling_gap = gap;
        m.capacitance = c;
        Some((m, s))
    }
}

/// Loop'a özgü sonuçlar (çözümden, verilen verici gücüyle).
#[derive(Clone, Copy, Debug)]
pub struct LoopReport {
    pub capacitance: f64,
    /// Halkanın kendi (yüksüz) Q'su: depolanan enerji / harcanan güç (ışıma + kayıp).
    pub q: f64,
    /// Eşleşmiş vericiyle −3 dB bant genişliği (Hz): yüklü Q = Q/2 → 2·f/Q.
    pub bandwidth_3db: f64,
    /// Kondansatör üzerindeki tepe gerilim (V) ve halka akımı (A, tepe), `power` W'ta.
    pub cap_voltage: f64,
    pub loop_current: f64,
    /// Toplam verim: ışıyan güç / giriş gücü (iletken + kondansatör kaybı düşülmüş).
    pub efficiency: f64,
    /// Kondansatör kaybının giriş gücüne oranı.
    pub cap_loss: f64,
}

/// Çözülmüş loop modelinden rapor. Kondansatör, modeldeki seri RLC yüküdür.
pub fn report(sol: &crate::solver::Solution, power: f64) -> Option<LoopReport> {
    let ld = sol.loads.first()?;
    let pin = sol.input_power;
    if pin <= 0.0 {
        return None;
    }
    let f = sol.frequency;
    let w = 2.0 * PI * f;
    let xc = -ld.impedance.im;
    if xc <= 0.0 {
        return None;
    }
    let c = 1.0 / (w * xc);
    let k = (power / pin).sqrt();
    let i = ld.current.norm() * k;
    let v = i * xc;
    // Depolanan enerji (tepe değerlerle): W = ½·C·V²; Q = ω·W / P.
    let q = w * 0.5 * c * v * v / power;
    // `loss_power` iletken ve yük kayıplarını birlikte içerir.
    Some(LoopReport {
        capacitance: c,
        q,
        bandwidth_3db: 2.0 * f / q,
        cap_voltage: v,
        loop_current: i,
        efficiency: sol.efficiency(),
        cap_loss: ld.power / pin,
    })
}

/// Dairesel halka için el formülleri (karşılaştırma ve doğrulama için):
/// (endüktans H, ışıma direnci Ω, iletken kayıp direnci Ω).
pub fn analytic(diameter: f64, conductor_d: f64, conductivity: f64, f: f64) -> (f64, f64, f64) {
    let mu0 = crate::MU0;
    let r = 0.5 * diameter;
    let a = 0.5 * conductor_d;
    let lambda = C0 / f;
    let l = mu0 * r * ((8.0 * r / a).ln() - 2.0);
    let area = PI * r * r;
    let r_rad = 320.0 * PI.powi(4) * area * area / lambda.powi(4);
    let rs = (PI * f * mu0 / conductivity).sqrt();
    let r_loss = diameter / conductor_d * rs;
    (l, r_rad, r_loss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Ground;

    #[test]
    fn thick_tube_small_loop_stays_thin_wire_valid() {
        // 15 m, 0.45 m halka, ≈ daire, 22 mm boru, 10 mm kuplaj teli.
        let m = MagLoop { diameter: 0.45, sides: 24, coupling_ratio: 0.117, coupling_wire: 0.010, ..Default::default() };
        let a = m.build(21.225e6);
        assert!(a.thin_wire_issues().is_empty(), "{:?}", a.thin_wire_issues());
        assert!(solver::solve(&a, 21.225e6).is_ok());
    }

    #[test]
    fn two_port_matches_full_solve() {
        let f = 7.1e6;
        let mut m = MagLoop::default();
        let tp = m.two_port(f, Ground::FreeSpace).unwrap();
        m.capacitance = m.tune(&tp, 50.0).unwrap();
        let z_tp = tp.input(cap_impedance(m.capacitance, m.cap_esr, f));
        let z_full = solver::solve(&m.build(f), f).unwrap().impedance();
        assert!((z_tp - z_full).norm() < 1e-3 * z_full.norm().max(1.0), "{z_tp} vs {z_full}");
    }

    #[test]
    fn direct_feed_agrees_with_formulas() {
        let f = 7.1e6;
        let mut m = MagLoop { coupling_ratio: 0.0, cap_esr: 0.0, ..Default::default() };
        let tp = m.two_port(f, Ground::FreeSpace).unwrap();
        m.capacitance = m.tune(&tp, 50.0).unwrap();
        let z = solver::solve(&m.build(f), f).unwrap().impedance();
        let (l, rr, rl) = analytic(1.0, 0.022, 5.8e7, f);
        let c_an = 1.0 / ((2.0 * PI * f).powi(2) * l);
        assert!((m.capacitance / c_an - 1.0).abs() < 0.05, "C {:.1} pF vs {:.1} pF", m.capacitance * 1e12, c_an * 1e12);
        assert!((z.re / (rr + rl) - 1.0).abs() < 0.08, "R {:.4} vs {:.4}", z.re, rr + rl);
        assert!(z.im.abs() < 0.01 * z.re.max(1e-3) + 1e-3, "X {}", z.im);
    }

    #[test]
    fn coupling_match_reaches_low_swr() {
        let f = 7.1e6;
        let m = MagLoop::default();
        let (m2, swr) = m.match_coupling(f, 50.0, Ground::FreeSpace).unwrap();
        assert!(swr < 1.3, "SWR {swr:.2}, oran {:.3}, açıklık {:.3}", m2.coupling_ratio, m2.coupling_gap);
        let sol = solver::solve(&m2.build(f), f).unwrap();
        let r = report(&sol, 100.0).unwrap();
        assert!(r.q > 500.0 && r.q < 5000.0, "Q {}", r.q);
        assert!(r.cap_voltage > 2000.0 && r.cap_voltage < 20000.0, "V {}", r.cap_voltage);
        assert!(r.efficiency > 0.08 && r.efficiency < 0.2, "verim {}", r.efficiency);
        eprintln!("oran {:.3} açıklık {:.1} mm C {:.2} pF SWR {swr:.3} Q {:.0} V {:.0} verim {:.3}", m2.coupling_ratio, m2.coupling_gap * 1e3, m2.capacitance * 1e12, r.q, r.cap_voltage, r.efficiency);
    }
}
