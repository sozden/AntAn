//! Yagi iyileştirici: Nelder-Mead (simpleks) yöntemiyle eleman boylarını ve
//! aralıklarını hedeflere göre ayarlar.
//!
//! Hedef fonksiyonu (küçültülür):
//!   −w_g·G[dBi] − w_fb·min(F/B, tavan) + w_swr·(SWR−1)
//! Bant verilirse kazanç ve F/B bant içindeki en kötü değerdir, SWR en kötü değer.
//! Değişkenler dalga boyu cinsindendir; yansıtıcı x = 0'da sabit kalır.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::C0;
use crate::farfield::{gain, to_db};
use crate::matching::swr;
use crate::solver::solve;
use crate::yagi::YagiDesign;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    pub w_gain: f64,
    pub w_fb: f64,
    pub w_swr: f64,
    pub fb_cap_db: f64,
    pub z0: f64,
    /// Bant yarı genişliği (merkez frekansın oranı, ör. 0.007 = ±%0.7). 0 = tek frekans.
    pub half_band: f64,
    /// Bum boyu sabit tutulsun mu (yalnız iç elemanların konumu değişir)?
    pub keep_boom: bool,
    /// Eleman konumları değişsin mi (yoksa yalnız boylar)?
    pub move_elements: bool,
}

impl Default for Goals {
    fn default() -> Self {
        Self { w_gain: 1.0, w_fb: 0.25, w_swr: 2.0, fb_cap_db: 25.0, z0: 50.0, half_band: 0.0, keep_boom: true, move_elements: true }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Score {
    pub gain_dbi: f64,
    pub fb_db: f64,
    pub swr: f64,
    pub cost: f64,
}

/// Tasarımı hedeflere göre değerlendirir (ileri yön +X, ufuk düzlemi).
pub fn evaluate(d: &YagiDesign, g: &Goals) -> Option<Score> {
    let ant = d.build();
    let freqs: Vec<f64> = if g.half_band > 0.0 { vec![d.frequency * (1.0 - g.half_band), d.frequency, d.frequency * (1.0 + g.half_band)] } else { vec![d.frequency] };
    let (mut gmin, mut fbmin, mut swrmax) = (f64::INFINITY, f64::INFINITY, 0.0f64);
    for f in freqs {
        let sol = solve(&ant, f).ok()?;
        let fwd = gain(&sol, PI / 2.0, 0.0);
        let back = gain(&sol, PI / 2.0, PI);
        gmin = gmin.min(to_db(fwd));
        fbmin = fbmin.min(to_db(fwd) - to_db(back));
        swrmax = swrmax.max(swr(sol.impedance(), g.z0).min(50.0));
    }
    let cost = -g.w_gain * gmin - g.w_fb * fbmin.min(g.fb_cap_db) + g.w_swr * (swrmax - 1.0);
    Some(Score { gain_dbi: gmin, fb_db: fbmin, swr: swrmax, cost })
}

/// Değişken eşlemesi: tasarım ↔ vektör (λ cinsinden).
struct Vars {
    lambda: f64,
    /// Konumu değişen elemanlar.
    movable: Vec<usize>,
}

impl Vars {
    fn new(d: &YagiDesign, g: &Goals) -> Self {
        let n = d.elements.len();
        let movable = if !g.move_elements {
            vec![]
        } else if g.keep_boom {
            (1..n.saturating_sub(1)).collect()
        } else {
            (1..n).collect()
        };
        Self { lambda: C0 / d.frequency, movable }
    }
    fn to_vec(&self, d: &YagiDesign) -> Vec<f64> {
        let mut v: Vec<f64> = d.elements.iter().map(|e| e.length / self.lambda).collect();
        v.extend(self.movable.iter().map(|&i| d.elements[i].x / self.lambda));
        v
    }
    fn apply(&self, base: &YagiDesign, v: &[f64]) -> YagiDesign {
        let mut d = base.clone();
        let n = d.elements.len();
        for i in 0..n {
            d.elements[i].length = v[i] * self.lambda;
        }
        for (k, &i) in self.movable.iter().enumerate() {
            d.elements[i].x = v[n + k] * self.lambda;
        }
        d
    }
    fn steps(&self, n: usize) -> Vec<f64> {
        let mut s = vec![0.01; n];
        s.extend(self.movable.iter().map(|_| 0.02));
        s
    }
}

/// Geometri kuralları dışındaki tasarımlara büyük ceza.
fn penalty(d: &YagiDesign) -> f64 {
    let lambda = C0 / d.frequency;
    let mut p = 0.0;
    for e in &d.elements {
        let l = e.length / lambda;
        if !(0.30..=0.65).contains(&l) {
            p += 1000.0;
        }
    }
    let mut xs: Vec<f64> = d.elements.iter().map(|e| e.x / lambda).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for w in xs.windows(2) {
        if w[1] - w[0] < 0.04 {
            p += 1000.0;
        }
    }
    p
}

/// Nelder-Mead. `progress(iter, en iyi skor, en iyi tasarım)` false dönerse durur.
pub fn optimize(start: &YagiDesign, goals: &Goals, max_iter: usize, mut progress: impl FnMut(usize, &Score, &YagiDesign) -> bool) -> (YagiDesign, Score) {
    let vars = Vars::new(start, goals);
    let x0 = vars.to_vec(start);
    let dim = x0.len();
    let f = |x: &[f64]| -> (f64, Score) {
        let d = vars.apply(start, x);
        let pen = penalty(&d);
        if pen > 0.0 {
            return (1e6 + pen, Score { cost: 1e6, ..Default::default() });
        }
        match evaluate(&d, goals) {
            Some(s) => (s.cost, s),
            None => (1e9, Score { cost: 1e9, ..Default::default() }),
        }
    };

    let steps = vars.steps(start.elements.len());
    let mut simplex: Vec<(Vec<f64>, f64, Score)> = Vec::with_capacity(dim + 1);
    let (c0, s0) = f(&x0);
    simplex.push((x0.clone(), c0, s0));
    for i in 0..dim {
        let mut x = x0.clone();
        x[i] += steps[i];
        let (c, s) = f(&x);
        simplex.push((x, c, s));
    }

    for iter in 0..max_iter {
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if !progress(iter, &simplex[0].2, &vars.apply(start, &simplex[0].0)) {
            break;
        }
        // Yakınsama: köşe maliyetleri arasındaki fark çok küçük.
        if (simplex[dim].1 - simplex[0].1).abs() < 1e-4 {
            break;
        }
        let centroid: Vec<f64> = (0..dim).map(|j| simplex[..dim].iter().map(|p| p.0[j]).sum::<f64>() / dim as f64).collect();
        let worst = simplex[dim].clone();
        let lerp = |t: f64| -> Vec<f64> { (0..dim).map(|j| centroid[j] + t * (worst.0[j] - centroid[j])).collect() };

        let xr = lerp(-1.0);
        let (cr, sr) = f(&xr);
        if cr < simplex[0].1 {
            let xe = lerp(-2.0);
            let (ce, se) = f(&xe);
            simplex[dim] = if ce < cr { (xe, ce, se) } else { (xr, cr, sr) };
        } else if cr < simplex[dim - 1].1 {
            simplex[dim] = (xr, cr, sr);
        } else {
            let (xc, (cc, sc)) = if cr < worst.1 {
                let x = lerp(-0.5);
                let r = f(&x);
                (x, r)
            } else {
                let x = lerp(0.5);
                let r = f(&x);
                (x, r)
            };
            if cc < worst.1.min(cr) {
                simplex[dim] = (xc, cc, sc);
            } else {
                // Küçült.
                let best = simplex[0].0.clone();
                for p in simplex.iter_mut().skip(1) {
                    let x: Vec<f64> = (0..dim).map(|j| best[j] + 0.5 * (p.0[j] - best[j])).collect();
                    let (c, s) = f(&x);
                    *p = (x, c, s);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let best = vars.apply(start, &simplex[0].0);
    (best, simplex[0].2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::Tube;
    use crate::yagi::Profile;

    #[test]
    fn optimizer_improves_cost() {
        let d = YagiDesign::standard(144.3e6, 3, Profile::uniform(Tube::aluminum(0.01, None)));
        let g = Goals::default();
        let before = evaluate(&d, &g).unwrap();
        let (best, after) = optimize(&d, &g, 60, |_, _, _| true);
        println!("önce {before:?}\nsonra {after:?}");
        assert!(after.cost < before.cost);
        assert_eq!(best.elements[0].x, d.elements[0].x);
        // Bum boyu korunmalı.
        assert!((best.boom_length() - d.boom_length()).abs() < 1e-12);
    }
}
