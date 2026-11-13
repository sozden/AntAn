//! Karmaşık yoğun doğrusal sistem çözümü (kısmi pivotlu LU).

use num_complex::Complex64;

/// Yerinde çarpanlara ayrılmış matris; aynı matrisle birden çok sağ taraf çözülebilir.
pub struct Lu {
    n: usize,
    a: Vec<Complex64>,
    perm: Vec<usize>,
}

impl Lu {
    /// `a` satır öncelikli n×n matristir. Tekil ise `None` döner.
    pub fn factor(mut a: Vec<Complex64>, n: usize) -> Option<Self> {
        assert_eq!(a.len(), n * n);
        let mut perm: Vec<usize> = (0..n).collect();
        for k in 0..n {
            let (mut p, mut best) = (k, a[k * n + k].norm_sqr());
            for r in k + 1..n {
                let v = a[r * n + k].norm_sqr();
                if v > best {
                    best = v;
                    p = r;
                }
            }
            if best == 0.0 || !best.is_finite() {
                return None;
            }
            if p != k {
                for c in 0..n {
                    a.swap(k * n + c, p * n + c);
                }
                perm.swap(k, p);
            }
            let pivot = a[k * n + k];
            for r in k + 1..n {
                let f = a[r * n + k] / pivot;
                a[r * n + k] = f;
                if f.norm_sqr() != 0.0 {
                    for c in k + 1..n {
                        let t = a[k * n + c];
                        a[r * n + c] -= f * t;
                    }
                }
            }
        }
        Some(Self { n, a, perm })
    }

    pub fn solve(&self, b: &[Complex64]) -> Vec<Complex64> {
        let n = self.n;
        let mut x: Vec<Complex64> = self.perm.iter().map(|&i| b[i]).collect();
        for r in 1..n {
            let mut s = x[r];
            for c in 0..r {
                s -= self.a[r * n + c] * x[c];
            }
            x[r] = s;
        }
        for r in (0..n).rev() {
            let mut s = x[r];
            for c in r + 1..n {
                s -= self.a[r * n + c] * x[c];
            }
            x[r] = s / self.a[r * n + r];
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_small_system() {
        let c = |re, im| Complex64::new(re, im);
        let a = vec![c(0.0, 1.0), c(2.0, 0.0), c(3.0, -1.0), c(1.0, 1.0)];
        let x_true = [c(1.0, -2.0), c(0.5, 3.0)];
        let b = [a[0] * x_true[0] + a[1] * x_true[1], a[2] * x_true[0] + a[3] * x_true[1]];
        let x = Lu::factor(a, 2).unwrap().solve(&b);
        assert!((x[0] - x_true[0]).norm() < 1e-12 && (x[1] - x_true[1]).norm() < 1e-12);
    }
}
