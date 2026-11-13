//! Empedans eşleme hesapları: SWR, hairpin (beta), gamma ve L devresi.

use num_complex::Complex64;
use std::f64::consts::PI;

use crate::C0;

/// Yansıma katsayısı.
pub fn reflection(z: Complex64, z0: f64) -> Complex64 {
    (z - z0) / (z + z0)
}

pub fn swr(z: Complex64, z0: f64) -> f64 {
    let g = reflection(z, z0).norm();
    if g >= 1.0 { f64::INFINITY } else { (1.0 + g) / (1.0 - g) }
}

/// Geri dönüş kaybı (dB, pozitif).
pub fn return_loss_db(z: Complex64, z0: f64) -> f64 {
    -20.0 * reflection(z, z0).norm().max(1e-15).log10()
}

/// Paralel iki iletkenli hattın karakteristik empedansı (ohm).
/// `spacing` merkezden merkeze, `d1`/`d2` iletken çapları.
pub fn two_wire_z0(spacing: f64, d1: f64, d2: f64) -> f64 {
    let x = (4.0 * spacing * spacing - d1 * d1 - d2 * d2) / (2.0 * d1 * d2);
    60.0 * x.max(1.0).acosh()
}

// ---------------------------------------------------------------- Hairpin

/// Hairpin (beta) eşleme sonucu.
///
/// Sürülen eleman rezonanstan kısa (kapasitif) tutulur; besleme uçlarına
/// paralel bağlanan kısa devre saplama (hairpin) endüktansı ile birlikte
/// L devresi oluşturur.
#[derive(Clone, Copy, Debug)]
pub struct Hairpin {
    /// Besleme noktasında gereken eleman reaktansı (ohm, negatif = kapasitif).
    pub required_reactance: f64,
    /// Mevcut reaktansa eklenmesi gereken miktar (ohm). Negatifse eleman kısaltılmalı.
    pub reactance_change: f64,
    /// Hairpin'in endüktif reaktansı (ohm) ve endüktansı (H).
    pub shunt_reactance: f64,
    pub inductance: f64,
    /// Hairpin hattının karakteristik empedansı (ohm) ve boyu (m).
    pub line_z0: f64,
    pub length: f64,
}

/// `z_feed`: sürülen elemanın besleme empedansı. `rod_diameter` ve `spacing`
/// hairpin çubuklarının çapı ve merkezden merkeze aralığıdır.
/// Direnç kısmı `z0`'dan küçük olmalıdır; değilse `None`.
pub fn hairpin(z_feed: Complex64, z0: f64, frequency: f64, rod_diameter: f64, spacing: f64) -> Option<Hairpin> {
    let r = z_feed.re;
    if !(r > 0.0 && r < z0) || spacing <= rod_diameter {
        return None;
    }
    let xc = (r * (z0 - r)).sqrt();
    let xl = r * z0 / xc;
    let line_z0 = two_wire_z0(spacing, rod_diameter, rod_diameter);
    let beta = 2.0 * PI * frequency / C0;
    Some(Hairpin {
        required_reactance: -xc,
        reactance_change: -xc - z_feed.im,
        shunt_reactance: xl,
        inductance: xl / (2.0 * PI * frequency),
        line_z0,
        length: (xl / line_z0).atan() / beta,
    })
}

// ------------------------------------------------------------------ Gamma

#[derive(Clone, Copy, Debug)]
pub struct GammaParams {
    /// Sürülen elemanın çapı (m).
    pub element_diameter: f64,
    /// Gamma çubuğunun çapı (m).
    pub rod_diameter: f64,
    /// Eleman ile çubuk arası merkezden merkeze açıklık (m).
    pub spacing: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Gamma {
    /// Gamma çubuğunun boyu: merkezden kısa devre kelepçesine (m).
    pub length: f64,
    /// Seri kondansatör (F).
    pub capacitance: f64,
    /// Empedans yükseltme oranı (1+α)².
    pub step_up: f64,
    /// Gamma bölümünün hat empedansı (ohm).
    pub line_z0: f64,
}

/// Gamma eşleme (Healey yöntemi).
///
/// `z_center`: sürülen eleman ortadan bölünüp dengeli beslenseydi görülecek
/// empedans. Model: anten yarısı Z_a/2, kelepçe noktasına 1/cos²(kl) ile
/// taşınır, (1+α)² ile yükseltilir, kısa devre saplama jZ₀tan(kl) ile
/// paraleldir; kalan endüktif reaktansı seri kondansatör götürür.
///
/// NOT: Bu yaklaşık bir modeldir; ölçüm/NEC ile doğrulanması yol haritasındadır.
pub fn gamma(z_center: Complex64, z0: f64, frequency: f64, p: GammaParams) -> Option<Gamma> {
    let (d1, d2, s) = (p.rod_diameter, p.element_diameter, p.spacing);
    if s <= 0.5 * (d1 + d2) || d1 <= 0.0 || d2 <= 0.0 {
        return None;
    }
    let alpha = ((4.0 * s * s - d1 * d1 + d2 * d2) / (4.0 * s * d1)).acosh() / ((4.0 * s * s + d1 * d1 - d2 * d2) / (4.0 * s * d2)).acosh();
    let step_up = (1.0 + alpha) * (1.0 + alpha);
    let line_z0 = two_wire_z0(s, d1, d2);
    let k = 2.0 * PI * frequency / C0;
    let input = |l: f64| {
        let za = z_center * 0.5 * step_up / (k * l).cos().powi(2);
        let zs = Complex64::new(0.0, line_z0 * (k * l).tan());
        za * zs / (za + zs)
    };
    // Re(Z_in) kısa çubukta 0'dan başlayıp artar; z0'ı ilk kestiği boyu bul.
    let max_len = 0.2 * C0 / frequency;
    let steps = 400;
    let mut prev = 1e-6;
    for i in 1..=steps {
        let l = max_len * i as f64 / steps as f64;
        if input(l).re >= z0 {
            let (mut lo, mut hi) = (prev, l);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if input(mid).re >= z0 { hi = mid } else { lo = mid }
            }
            let l = 0.5 * (lo + hi);
            let x = input(l).im;
            if x <= 0.0 {
                return None;
            }
            return Some(Gamma { length: l, capacitance: 1.0 / (2.0 * PI * frequency * x), step_up, line_z0 });
        }
        prev = l;
    }
    None
}

// --------------------------------------------------------------- L devresi

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reactive {
    /// Endüktans (H).
    Inductor(f64),
    /// Kapasite (F).
    Capacitor(f64),
}

impl Reactive {
    fn from_reactance(x: f64, frequency: f64) -> Self {
        let w = 2.0 * PI * frequency;
        if x >= 0.0 { Self::Inductor(x / w) } else { Self::Capacitor(-1.0 / (w * x)) }
    }
}

/// L devresi çözümü. Yük tarafından kaynağa doğru sıra `shunt_at_load`
/// ile belirlenir: doğruysa paralel eleman yük uçlarında, seri eleman
/// hat tarafındadır; yanlışsa seri eleman yük tarafındadır.
#[derive(Clone, Copy, Debug)]
pub struct LNetwork {
    pub shunt_at_load: bool,
    pub series: Reactive,
    pub shunt: Reactive,
}

/// `z_load`'u gerçel `z0`'a eşleyen bütün L devrelerini döndürür (0–4 çözüm).
pub fn l_network(z_load: Complex64, z0: f64, frequency: f64) -> Vec<LNetwork> {
    let mut out = Vec::new();
    let (r, x) = (z_load.re, z_load.im);
    if r <= 0.0 {
        return out;
    }
    // Seri eleman yük tarafında: R + j(X + Xs) paralel jXp = z0.
    if r < z0 {
        let q = (r * (z0 - r)).sqrt();
        for sign in [1.0, -1.0] {
            let xt = sign * q;
            let xp = -(r * r + xt * xt) / xt;
            out.push(LNetwork {
                shunt_at_load: false,
                series: Reactive::from_reactance(xt - x, frequency),
                shunt: Reactive::from_reactance(xp, frequency),
            });
        }
    }
    // Paralel eleman yük tarafında: (Y_L + jB) → gerçel kısmı 1/z0 olan seri eşdeğer.
    let y = Complex64::new(1.0, 0.0) / z_load;
    let (g, b) = (y.re, y.im);
    if g < 1.0 / z0 {
        let q = (g * (1.0 / z0 - g)).sqrt();
        for sign in [1.0, -1.0] {
            let bt = sign * q;
            // bt ≈ b ise yükün kendi süseptansı zaten hedefe eşittir; paralel
            // eleman sonsuz reaktansa (devresi açık, eleman yok) karşılık
            // gelir ve Reactive ile temsil edilemez — bu kenar durumu atla.
            if (bt - b).abs() < 1e-12 * q.max(b.abs()).max(1.0) {
                continue;
            }
            let z = Complex64::new(1.0, 0.0) / Complex64::new(g, bt);
            out.push(LNetwork {
                shunt_at_load: true,
                series: Reactive::from_reactance(-z.im, frequency),
                shunt: Reactive::from_reactance(-1.0 / (bt - b), frequency),
            });
        }
    }
    out
}

/// Bir L devresi takıldığında hat tarafından görülen empedans (doğrulama için).
pub fn l_network_input(z_load: Complex64, net: &LNetwork, frequency: f64) -> Complex64 {
    let w = 2.0 * PI * frequency;
    let jx = |c: Reactive| match c {
        Reactive::Inductor(l) => Complex64::new(0.0, w * l),
        Reactive::Capacitor(c) => Complex64::new(0.0, -1.0 / (w * c)),
    };
    let par = |a: Complex64, b: Complex64| a * b / (a + b);
    if net.shunt_at_load { par(z_load, jx(net.shunt)) + jx(net.series) } else { par(z_load + jx(net.series), jx(net.shunt)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swr_basics() {
        assert!((swr(Complex64::new(50.0, 0.0), 50.0) - 1.0).abs() < 1e-12);
        assert!((swr(Complex64::new(100.0, 0.0), 50.0) - 2.0).abs() < 1e-12);
        assert!((swr(Complex64::new(25.0, 0.0), 50.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn hairpin_matches_to_z0() {
        let f = 144.3e6;
        let h = hairpin(Complex64::new(22.0, -5.0), 50.0, f, 0.004, 0.03).unwrap();
        let z = Complex64::new(22.0, h.required_reactance);
        let xl = Complex64::new(0.0, h.line_z0 * (2.0 * PI * f / C0 * h.length).tan());
        let zin = z * xl / (z + xl);
        assert!((zin - 50.0).norm() < 1e-6, "{zin}");
    }

    #[test]
    fn l_networks_match_to_z0() {
        let f = 14.2e6;
        for zl in [Complex64::new(20.0, -30.0), Complex64::new(120.0, 45.0), Complex64::new(50.0, 80.0)] {
            let nets = l_network(zl, 50.0, f);
            assert!(!nets.is_empty());
            for n in &nets {
                let zin = l_network_input(zl, n, f);
                assert!((zin - 50.0).norm() < 1e-6, "{zl} -> {zin} ({n:?})");
            }
        }
    }

    #[test]
    fn gamma_finds_solution() {
        let p = GammaParams { element_diameter: 0.010, rod_diameter: 0.004, spacing: 0.03 };
        let g = gamma(Complex64::new(25.0, 0.0), 50.0, 144.3e6, p).unwrap();
        assert!(g.length > 0.01 && g.length < 0.3, "{}", g.length);
        assert!(g.capacitance > 1e-12 && g.capacitance < 1e-9, "{}", g.capacitance);
        assert!(g.step_up > 4.0);
    }
}
