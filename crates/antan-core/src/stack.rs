//! Anten istifi: aynı antenin kopyaları dikey (z) ve/veya yatay (y, elemanlar
//! doğrultusunda) dizilir, hepsi aynı fazda ve aynı genlikte beslenir (eşit
//! boylu kablolar + ideal güç bölücü). Kopyalar arası karşılıklı etki çözücüde
//! doğal olarak hesaba girer.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::farfield::Pattern;
use crate::geometry::Antenna;
use crate::solver::{self, Solution};
use crate::C0;

/// İstif düzeni. `rows` dikeyde (üst üste), `cols` yatayda (yan yana) anten
/// sayısıdır; 1×1 istif yok demektir. Aralıklar metre cinsindendir, merkezden merkeze.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stack {
    pub rows: usize,
    pub cols: usize,
    pub v_spacing: f64,
    pub h_spacing: f64,
}

impl Default for Stack {
    fn default() -> Self {
        Self { rows: 1, cols: 1, v_spacing: 0.0, h_spacing: 0.0 }
    }
}

impl Stack {
    pub fn is_single(&self) -> bool {
        self.count() <= 1
    }
    pub fn count(&self) -> usize {
        self.rows.max(1) * self.cols.max(1)
    }
    /// Kopyaların konum kaydırmaları (dy, dz), metre. Alt sıra özgün yükseklikte
    /// kalır (zemin üzerindeki yükseklik = en alttaki antenin yüksekliği), yatayda
    /// dizi ortalanır.
    pub fn offsets(&self) -> Vec<(f64, f64)> {
        let (r, c) = (self.rows.max(1), self.cols.max(1));
        let mut v = Vec::with_capacity(r * c);
        for i in 0..r {
            for j in 0..c {
                v.push(((j as f64 - 0.5 * (c - 1) as f64) * self.h_spacing, i as f64 * self.v_spacing));
            }
        }
        v
    }
}

/// İstifin tel modeli: teller, kaynaklar ve yükler her kopya için tekrarlanır.
/// İlk kopyanın tel sıraları özgün antenle aynıdır.
pub fn apply(a: &Antenna, s: &Stack) -> Antenna {
    if s.is_single() {
        return a.clone();
    }
    let mut out = a.clone();
    out.wires.clear();
    out.sources.clear();
    out.loads.clear();
    for (dy, dz) in s.offsets() {
        let base = out.wires.len();
        for w in &a.wires {
            let mut w = w.clone();
            w.start[1] += dy;
            w.end[1] += dy;
            w.start[2] += dz;
            w.end[2] += dz;
            out.wires.push(w);
        }
        for src in &a.sources {
            let mut src = src.clone();
            src.wire += base;
            out.sources.push(src);
        }
        for ld in &a.loads {
            let mut ld = ld.clone();
            ld.wire += base;
            out.loads.push(ld);
        }
    }
    out
}

/// Eşit boylu `z0` kablolarla ideal N yollu güç bölücüye bağlı beslemelerin
/// bölücü girişinde görülen empedansı: Γ = ortalama(Γᵢ). Tek beslemede kendisi.
pub fn combined_impedance(feeds: &[Complex64], z0: f64) -> Complex64 {
    match feeds.len() {
        0 => Complex64::new(0.0, 0.0),
        1 => feeds[0],
        n => {
            let z0c = Complex64::new(z0, 0.0);
            let g: Complex64 = feeds.iter().map(|z| (z - z0c) / (z + z0c)).sum::<Complex64>() / n as f64;
            z0c * (1.0 + g) / (1.0 - g)
        }
    }
}

/// Bir çözümün beslemelerinin empedansları.
pub fn feed_impedances(sol: &Solution) -> Vec<Complex64> {
    sol.feeds.iter().map(|f| f.impedance).collect()
}

/// DL6WU kuralı: istif düzlemindeki −3 dB huzme genişliği θ için en uygun
/// aralık D = λ / (2·sin(θ/2)). Huzme ne kadar darsa aralık o kadar büyür.
pub fn dl6wu_spacing(lambda: f64, beamwidth_deg: f64) -> f64 {
    let half = (0.5 * beamwidth_deg.clamp(5.0, 179.0)).to_radians();
    lambda / (2.0 * half.sin())
}

/// Tek antenin (zemin dikkate alınmadan, serbest uzayda) huzme genişliklerinden
/// önerilen aralıklar (dikey, yatay), metre. Dikey istif yükseliş düzlemindeki,
/// yatay istif yatay düzlemdeki huzmeyi daraltır.
pub fn recommended_spacing(a: &Antenna) -> Option<(f64, f64)> {
    let mut free = a.clone();
    free.ground = crate::geometry::Ground::FreeSpace;
    let sol = solver::solve(&free, free.frequency).ok()?;
    let p = Pattern::compute(&sol, 3.0).summary(&sol);
    let lambda = C0 / a.frequency;
    Some((dl6wu_spacing(lambda, p.beamwidth_el_deg), dl6wu_spacing(lambda, p.beamwidth_az_deg)))
}

/// Aralık taramasının bir satırı.
#[derive(Clone, Copy, Debug)]
pub struct StackRow {
    /// Taranan aralık (m): dikey istifte dikey, yalnız yatayda yatay aralık.
    pub spacing: f64,
    pub gain_dbi: f64,
    pub front_to_back_db: f64,
    /// En yüksek yan/arka lobun ana huzmeye göre seviyesi (dB, negatif).
    pub sidelobe_db: f64,
    /// Güç bölücü girişindeki empedans.
    pub impedance: Complex64,
    /// Beslemelerin empedansları arasındaki en büyük fark (Ω), karşılıklı etkinin ölçüsü.
    pub feed_spread: f64,
}

/// İstif aralığını tarar. Dikeyde birden çok sıra varsa dikey aralık taranır ve
/// yatay aralık aynı oranda değişir; yoksa yatay aralık taranır. `step`
/// paralel iş parçacıklarında her biten satırdan sonra çağrılır.
pub fn spacing_scan(a: &Antenna, s: &Stack, spacings: &[f64], z0: f64, step: &(dyn Fn() + Sync)) -> Vec<StackRow> {
    let vertical = s.rows > 1;
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4).min(spacings.len().max(1));
    let chunk = spacings.len().div_ceil(threads).max(1);
    let mut rows: Vec<StackRow> = std::thread::scope(|sc| {
        let hs: Vec<_> = spacings
            .chunks(chunk)
            .map(|part| {
                sc.spawn(move || {
                    part.iter()
                        .filter_map(|&d| {
                            let mut st = *s;
                            if vertical {
                                let k = if s.v_spacing > 0.0 { d / s.v_spacing } else { 1.0 };
                                st.v_spacing = d;
                                st.h_spacing = if s.v_spacing > 0.0 { s.h_spacing * k } else { s.h_spacing };
                            } else {
                                st.h_spacing = d;
                            }
                            let r = evaluate(a, &st, z0).map(|mut r| {
                                r.spacing = d;
                                r
                            });
                            step();
                            r
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    rows.sort_by(|x, y| x.spacing.total_cmp(&y.spacing));
    rows
}

/// Bir istif düzenini çözer ve özetler.
pub fn evaluate(a: &Antenna, s: &Stack, z0: f64) -> Option<StackRow> {
    let st = apply(a, s);
    let sol = solver::solve(&st, st.frequency).ok()?;
    let pat = Pattern::compute(&sol, 2.0);
    let sum = pat.summary(&sol);
    let feeds = feed_impedances(&sol);
    let spread = feeds.iter().map(|z| z.re).fold(f64::NEG_INFINITY, f64::max) - feeds.iter().map(|z| z.re).fold(f64::INFINITY, f64::min);
    Some(StackRow {
        spacing: if s.rows > 1 { s.v_spacing } else { s.h_spacing },
        gain_dbi: sum.max_gain_dbi,
        front_to_back_db: sum.front_to_back_db,
        sidelobe_db: sidelobe_level(&pat, &sum),
        impedance: combined_impedance(&feeds, z0),
        feed_spread: spread,
    })
}

/// Ana huzmenin dışındaki en yüksek lob (dB, ana huzmeye göre). Basit ve sağlam
/// olması için ana yönden, iki düzlemdeki yarım güç genişliğinin büyüğü kadar
/// açıdan (≈ ilk sıfırlar) uzaktaki en yüksek kazanç alınır.
fn sidelobe_level(p: &Pattern, sum: &crate::farfield::PatternSummary) -> f64 {
    let main = crate::farfield::direction(sum.max_theta, sum.max_phi);
    // Ana huzme konisi: iki düzlemdeki genişliklerin büyüğü kadar (≈ ilk sıfırlar).
    let cone = sum.beamwidth_az_deg.max(sum.beamwidth_el_deg).to_radians().min(std::f64::consts::PI);
    let peak = p.max_gain();
    let mut best = 0.0_f64;
    for it in 0..p.n_theta {
        for ip in 0..p.n_phi {
            let d = crate::farfield::direction(p.theta(it), p.phi(ip));
            let ang = crate::geometry::dot(d, main).clamp(-1.0, 1.0).acos();
            if ang > cone {
                best = best.max(p.at(it, ip));
            }
        }
    }
    crate::farfield::to_db(best) - crate::farfield::to_db(peak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{self, Tube};

    #[test]
    fn combined_of_equal_feeds_is_the_feed() {
        let z = Complex64::new(32.0, -3.0);
        let c = combined_impedance(&[z, z, z, z], 50.0);
        assert!((c - z).norm() < 1e-9);
    }

    #[test]
    fn offsets_layout() {
        let s = Stack { rows: 2, cols: 2, v_spacing: 2.0, h_spacing: 3.0 };
        assert_eq!(s.offsets(), vec![(-1.5, 0.0), (1.5, 0.0), (-1.5, 2.0), (1.5, 2.0)]);
    }

    #[test]
    fn two_and_four_yagis_gain() {
        let f = 144.3e6;
        let lambda = C0 / f;
        let a = templates::yagi(f, 5, &Tube::aluminum(0.010, Some(0.001)));
        let single = {
            let s = solver::solve(&a, f).unwrap();
            Pattern::compute(&s, 2.0).summary(&s).max_gain_dbi
        };
        let two = evaluate(&a, &Stack { rows: 2, cols: 1, v_spacing: 1.25 * lambda, h_spacing: 0.0 }, 50.0).unwrap();
        let four = evaluate(&a, &Stack { rows: 2, cols: 2, v_spacing: 1.25 * lambda, h_spacing: 1.25 * lambda }, 50.0).unwrap();
        let (g2, g4) = (two.gain_dbi - single, four.gain_dbi - single);
        assert!((2.6..3.4).contains(&g2), "2 istif +{g2:.2} dB");
        assert!((5.5..6.6).contains(&g4), "2×2 istif +{g4:.2} dB");
        assert!(two.sidelobe_db < -3.0);
        let (v, h) = recommended_spacing(&a).unwrap();
        assert!(v > 0.6 * lambda && v < 2.0 * lambda && h > 0.6 * lambda && h < 2.0 * lambda, "{v} {h}");
    }
}
