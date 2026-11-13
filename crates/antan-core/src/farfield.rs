//! Uzak alan, kazanç ve ışıma diyagramı.
//!
//! Açı kuralı: θ +Z ekseninden ölçülür (0 = tepe), φ +X'ten +Y'ye doğru.
//! Yükseliş (elevation) = 90° − θ.

use num_complex::Complex64;
use std::f64::consts::PI;

use crate::geometry::{Vec3, dot};
use crate::solver::Solution;
use crate::{C0, ETA0, MU0};

/// Uzak alan bileşenleri (r·E, volt); e^{−jkr}/r çarpanı atılmıştır.
#[derive(Clone, Copy, Debug)]
pub struct FarField {
    pub e_theta: Complex64,
    pub e_phi: Complex64,
}

impl FarField {
    /// Işıma şiddeti (W/sr).
    pub fn intensity(&self) -> f64 {
        (self.e_theta.norm_sqr() + self.e_phi.norm_sqr()) / (2.0 * ETA0)
    }
}

pub fn direction(theta: f64, phi: f64) -> Vec3 {
    [theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()]
}

/// Fresnel yansıma katsayıları (dikey, yatay) — `elevation` zemine göre açı (rad).
/// Kusursuz iletkende Rv → +1, Rh → −1.
pub fn fresnel(elevation: f64, permittivity: f64, conductivity: f64, frequency: f64) -> (Complex64, Complex64) {
    let eps_c = Complex64::new(permittivity, -conductivity / (2.0 * PI * frequency * crate::EPS0));
    let s = elevation.sin();
    let root = (eps_c - elevation.cos().powi(2)).sqrt();
    let rv = (eps_c * s - root) / (eps_c * s + root);
    let rh = (s - root) / (s + root);
    (rv, rh)
}

pub fn field(sol: &Solution, theta: f64, phi: f64) -> FarField {
    let zero = Complex64::new(0.0, 0.0);
    let grounded = !sol.ground.is_free();
    let r = direction(theta, phi);
    // Zeminin altına ışıma yok.
    if grounded && r[2] < -1e-12 {
        return FarField { e_theta: zero, e_phi: zero };
    }
    let k = 2.0 * PI * sol.frequency / C0;
    let th = [theta.cos() * phi.cos(), theta.cos() * phi.sin(), -theta.sin()];
    let ph = [-phi.sin(), phi.cos(), 0.0];
    let (mut st, mut sp, mut it, mut ip) = (zero, zero, zero, zero);
    for &(b, mid, dl) in &sol.elements {
        let i = sol.currents[b];
        let w = i * Complex64::from_polar(1.0, k * dot(r, mid));
        st += w * dot(dl, th);
        sp += w * dot(dl, ph);
        if grounded {
            // Kusursuz zemin görüntüsü: aynalanmış konum, akım (−Ix, −Iy, +Iz).
            let m = crate::solver::mirror(mid);
            let dli = [-dl[0], -dl[1], dl[2]];
            let w = i * Complex64::from_polar(1.0, k * dot(r, m));
            it += w * dot(dli, th);
            ip += w * dot(dli, ph);
        }
    }
    match sol.ground {
        crate::geometry::Ground::Real { permittivity, conductivity } => {
            // Görüntü kusursuz zemin için kurulduğundan oranlar: Rv/(+1), Rh/(−1).
            let (rv, rh) = fresnel(PI / 2.0 - theta, permittivity, conductivity, sol.frequency);
            st += rv * it;
            sp += -rh * ip;
        }
        _ => {
            st += it;
            sp += ip;
        }
    }
    let c = Complex64::new(0.0, -2.0 * PI * sol.frequency * MU0 / (4.0 * PI));
    FarField { e_theta: c * st, e_phi: c * sp }
}

/// Doğrusal güç kazancı (izotropa göre).
pub fn gain(sol: &Solution, theta: f64, phi: f64) -> f64 {
    4.0 * PI * field(sol, theta, phi).intensity() / sol.input_power
}

pub fn to_db(x: f64) -> f64 {
    10.0 * x.max(1e-30).log10()
}

/// Küre üzerinde örneklenmiş kazanç.
#[derive(Clone, Debug)]
pub struct Pattern {
    /// θ örnek sayısı (0..=180°).
    pub n_theta: usize,
    /// φ örnek sayısı (0..360°, son nokta tekrarlanmaz).
    pub n_phi: usize,
    /// Doğrusal kazanç, `[it * n_phi + ip]`.
    pub gain: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct PatternSummary {
    pub max_gain_dbi: f64,
    /// En büyük kazancın yönü (rad).
    pub max_theta: f64,
    pub max_phi: f64,
    /// Ön/arka oranı (dB): ana huzme ile tam ters yön.
    pub front_to_back_db: f64,
    /// Yönlülük (dBi), diyagramın küre üzerinde integralinden.
    pub directivity_dbi: f64,
    /// −3 dB huzme genişliği (derece): yatay (θ sabit) ve düşey (φ sabit) kesit.
    pub beamwidth_az_deg: f64,
    pub beamwidth_el_deg: f64,
}

impl Pattern {
    /// `step_deg` derecelik ızgarada hesaplar (180'i tam bölmeli).
    pub fn compute(sol: &Solution, step_deg: f64) -> Self {
        let n_theta = (180.0 / step_deg).round() as usize + 1;
        let n_phi = (360.0 / step_deg).round() as usize;
        let mut g = Vec::with_capacity(n_theta * n_phi);
        for it in 0..n_theta {
            let theta = PI * it as f64 / (n_theta - 1) as f64;
            for ip in 0..n_phi {
                g.push(gain(sol, theta, 2.0 * PI * ip as f64 / n_phi as f64));
            }
        }
        Self { n_theta, n_phi, gain: g }
    }
    pub fn theta(&self, it: usize) -> f64 {
        PI * it as f64 / (self.n_theta - 1) as f64
    }
    pub fn phi(&self, ip: usize) -> f64 {
        2.0 * PI * ip as f64 / self.n_phi as f64
    }
    pub fn at(&self, it: usize, ip: usize) -> f64 {
        self.gain[it * self.n_phi + ip % self.n_phi]
    }
    pub fn max_gain(&self) -> f64 {
        self.gain.iter().copied().fold(0.0, f64::max)
    }

    pub fn summary(&self, sol: &Solution) -> PatternSummary {
        let (mut best, mut bi) = (0.0, 0);
        for (i, &g) in self.gain.iter().enumerate() {
            if g > best {
                best = g;
                bi = i;
            }
        }
        let max_theta = self.theta(bi / self.n_phi);
        let max_phi = self.phi(bi % self.n_phi);
        // Arka yön: serbest uzayda tam ters yön; zemin üzerinde aynı yükseliş
        // açısında ters azimut (EZNEC tanımı).
        let back = if sol.ground.is_free() { gain(sol, PI - max_theta, max_phi + PI) } else { gain(sol, max_theta, max_phi + PI) };

        // ∫ G dΩ / 4π = verim; yönlülük = G_max / verim.
        let dth = PI / (self.n_theta - 1) as f64;
        let dph = 2.0 * PI / self.n_phi as f64;
        let mut integral = 0.0;
        for it in 0..self.n_theta {
            let w = if it == 0 || it == self.n_theta - 1 { 0.5 } else { 1.0 };
            let row: f64 = (0..self.n_phi).map(|ip| self.at(it, ip)).sum();
            integral += w * row * self.theta(it).sin() * dth * dph;
        }
        let directivity = best * 4.0 * PI / integral;

        let az = |d: f64| gain(sol, max_theta, max_phi + d);
        let el = |d: f64| {
            // Ana huzmeden geçen düşey büyük çember boyunca.
            let t = max_theta + d;
            gain(sol, t, max_phi)
        };
        PatternSummary {
            max_gain_dbi: to_db(best),
            max_theta,
            max_phi,
            front_to_back_db: to_db(best) - to_db(back),
            directivity_dbi: to_db(directivity),
            beamwidth_az_deg: half_power_width(az, best),
            beamwidth_el_deg: half_power_width(el, best),
        }
    }
}

/// Tepe çevresinde kazancın yarıya düştüğü iki açı arasındaki genişlik (derece).
fn half_power_width(g: impl Fn(f64) -> f64, peak: f64) -> f64 {
    let edge = |sign: f64| {
        let step = PI / 720.0;
        let mut d = 0.0;
        while d < PI {
            if g(sign * (d + step)) < 0.5 * peak {
                // Doğrusal ara değerleme.
                let (g0, g1) = (g(sign * d), g(sign * (d + step)));
                return d + step * (g0 - 0.5 * peak) / (g0 - g1).max(1e-30);
            }
            d += step;
        }
        PI
    };
    (edge(1.0) + edge(-1.0)).to_degrees()
}

/// Yatay kesit: θ sabit, φ 0..360°. `(φ, doğrusal kazanç)` döner.
pub fn azimuth_cut(sol: &Solution, theta: f64, points: usize) -> Vec<(f64, f64)> {
    (0..=points)
        .map(|i| {
            let phi = 2.0 * PI * i as f64 / points as f64;
            (phi, gain(sol, theta, phi))
        })
        .collect()
}

/// Düşey kesit: `phi` yönünden geçen düzlemde yükseliş açısı α 0..360°
/// (0 = ufuk ileri, 90 = tepe, 180 = ufuk geri). `(α, doğrusal kazanç)` döner.
pub fn elevation_cut(sol: &Solution, phi: f64, points: usize) -> Vec<(f64, f64)> {
    (0..=points)
        .map(|i| {
            let alpha = 2.0 * PI * i as f64 / points as f64;
            // α → (θ, φ): θ = 90° − α, negatif θ karşı yarım düzleme düşer.
            let theta = PI / 2.0 - alpha;
            (alpha, gain(sol, theta, phi))
        })
        .collect()
}
