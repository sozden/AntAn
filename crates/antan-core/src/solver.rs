//! İnce tel moment yöntemi (MoM) çözücüsü — serbest uzay.
//!
//! Formülasyon: Harrington'ın karışık potansiyelli (vektör + skaler potansiyel)
//! darbe tabanlı / nokta eşlemeli yöntemi. Her tel M eşit parçaya bölünür;
//! bilinmeyenler iç düğümlerdeki akımlardır (uçlarda akım sıfır).
//!
//! n. tabanın (düğüm p_n, yön u, parça boyu Δ) üç kaynağı vardır:
//!   - akım parçası:  p_n − uΔ/2 … p_n + uΔ/2
//!   - yük parçası n+: p_n … p_n + uΔ      (yük +I/jω)
//!   - yük parçası n−: p_n − uΔ … p_n      (yük −I/jω)
//!
//!   Z_mn = jωμ (Δl_n·Δl_m) ψ(n,m)
//!        + 1/(jωε) [ψ(n+,m+) − ψ(n−,m+) − ψ(n+,m−) + ψ(n−,m−)]
//!
//!   ψ = 1/(4πΔ) ∫ e^{−jkR}/R ds,  R = √(ρ² + a² + (z−s)²)  (indirgenmiş çekirdek)
//!
//! Tekil kısım (1/R) analitik, kalan düzgün kısım Gauss-Legendre ile alınır.
//! Zaman bağımlılığı e^{jωt}; endüktif reaktans pozitiftir.
//!
//! Sınırlar (bkz. CLAUDE.md yol haritası): tel birleşimleri (junction) yok,
//! toprak yok, kademeli (teleskopik) eleman yok.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::geometry::{Antenna, Vec3, add, dot, scale, sub};
use crate::linalg::Lu;
use crate::{C0, EPS0, MU0};

#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    NoWires,
    NoSources,
    DegenerateWire(usize),
    BadSource(usize),
    Singular,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWires => write!(f, "Modelde eleman yok"),
            Self::NoSources => write!(f, "Besleme kaynağı tanımlı değil"),
            Self::DegenerateWire(i) => write!(f, "{}. elemanın boyu ya da çapı geçersiz", i + 1),
            Self::BadSource(i) => write!(f, "{}. kaynak olmayan bir elemana bağlı", i + 1),
            Self::Singular => write!(f, "Empedans matrisi tekil; geometri çözülemedi"),
        }
    }
}
impl std::error::Error for SolveError {}

/// Bir akım tabanı (iç düğüm).
#[derive(Clone, Copy, Debug)]
pub struct Basis {
    pub wire: usize,
    /// Düğümün tel boyunca konumu, 0..1.
    pub t: f64,
    pub pos: Vec3,
    /// Yön × parça boyu.
    pub dl: Vec3,
    pub radius: f64,
}

/// Bir besleme noktasındaki sonuç.
#[derive(Clone, Copy, Debug)]
pub struct Feed {
    pub basis: usize,
    pub voltage: Complex64,
    pub current: Complex64,
    pub impedance: Complex64,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub frequency: f64,
    pub basis: Vec<Basis>,
    /// Her tabanın akımı (A, tepe değer).
    pub currents: Vec<Complex64>,
    pub feeds: Vec<Feed>,
    /// Kaynaklardan giren toplam güç (W).
    pub input_power: f64,
    /// İletken kaybı (W).
    pub loss_power: f64,
}

impl Solution {
    /// İlk kaynağın giriş empedansı.
    pub fn impedance(&self) -> Complex64 {
        self.feeds[0].impedance
    }
    /// Işıma verimi (0..1), yalnız iletken kaybına göre.
    pub fn efficiency(&self) -> f64 {
        if self.input_power > 0.0 { (1.0 - self.loss_power / self.input_power).clamp(0.0, 1.0) } else { 0.0 }
    }
    /// Bir tel boyunca akım dağılımı: (konum 0..1, akım). Uçlar sıfırdır.
    pub fn wire_currents(&self, wire: usize) -> Vec<(f64, Complex64)> {
        let zero = Complex64::new(0.0, 0.0);
        let mut v = vec![(0.0, zero)];
        v.extend(self.basis.iter().zip(&self.currents).filter(|(b, _)| b.wire == wire).map(|(b, i)| (b.t, *i)));
        v.push((1.0, zero));
        v
    }
    /// Tüm tabanlardaki en büyük akım genliği.
    pub fn max_current(&self) -> f64 {
        self.currents.iter().map(|c| c.norm()).fold(0.0, f64::max)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SweepPoint {
    pub frequency: f64,
    pub impedance: Complex64,
}

const GL_X: [f64; 4] = [0.183_434_642_495_649_8, 0.525_532_409_916_329, 0.796_666_477_413_626_7, 0.960_289_856_497_536_3];
const GL_W: [f64; 4] = [0.362_683_783_378_362, 0.313_706_645_877_887_3, 0.222_381_034_453_374_5, 0.101_228_536_290_376_3];

/// ψ = 1/(4π·len) ∫₀^len e^{−jkR}/R ds; kaynak parçası `start`'tan `dir` yönünde.
fn psi(start: Vec3, dir: Vec3, len: f64, radius: f64, obs: Vec3, k: f64) -> Complex64 {
    let d = sub(obs, start);
    let z = dot(d, dir);
    let rho2 = (dot(d, d) - z * z).max(0.0) + radius * radius;
    let rho = rho2.sqrt();
    // ∫ ds/R, analitik.
    let singular = ((len - z) / rho).asinh() + (z / rho).asinh();
    // ∫ (e^{−jkR} − 1)/R ds, düzgün.
    let mut smooth = Complex64::new(0.0, 0.0);
    let h = 0.5 * len;
    for i in 0..4 {
        for sign in [-1.0, 1.0] {
            let s = h + sign * h * GL_X[i];
            let r = (rho2 + (z - s) * (z - s)).sqrt();
            let kr = k * r;
            // (cos kr − 1)/r küçük kr'de yuvarlama hatasına duyarlı: 2 sin²(kr/2) kullan.
            let re = -2.0 * (0.5 * kr).sin().powi(2) / r;
            let im = -kr.sin() / r;
            smooth += Complex64::new(re, im) * GL_W[i];
        }
    }
    (singular + smooth * h) / (4.0 * PI * len)
}

struct Mesh {
    basis: Vec<Basis>,
    /// Her tabanın birim yönü ve parça boyu.
    dirs: Vec<(Vec3, f64)>,
}

fn mesh(antenna: &Antenna) -> Result<Mesh, SolveError> {
    if antenna.wires.is_empty() {
        return Err(SolveError::NoWires);
    }
    let lambda = C0 / antenna.frequency;
    let mut basis = Vec::new();
    let mut dirs = Vec::new();
    for (wi, w) in antenna.wires.iter().enumerate() {
        let len = w.length();
        if !(len > 0.0) || !(w.diameter > 0.0) {
            return Err(SolveError::DegenerateWire(wi));
        }
        let m = w.segment_count(lambda);
        let u = w.direction();
        let delta = len / m as f64;
        for i in 1..m {
            let t = i as f64 / m as f64;
            basis.push(Basis { wire: wi, t, pos: add(w.start, scale(u, len * t)), dl: scale(u, delta), radius: w.radius() });
            dirs.push((u, delta));
        }
    }
    Ok(Mesh { basis, dirs })
}

/// Anteni `frequency` (Hz) frekansında çözer. Parçalama her zaman anten
/// tasarım frekansına göre yapılır; böylece frekans taraması pürüzsüz olur.
pub fn solve(antenna: &Antenna, frequency: f64) -> Result<Solution, SolveError> {
    if antenna.sources.is_empty() {
        return Err(SolveError::NoSources);
    }
    let Mesh { basis, dirs } = mesh(antenna)?;
    let n = basis.len();
    let omega = 2.0 * PI * frequency;
    let k = omega / C0;
    let j = Complex64::new(0.0, 1.0);
    let c_a = j * omega * MU0;
    let c_phi = 1.0 / (j * omega * EPS0);

    let mut z = vec![Complex64::new(0.0, 0.0); n * n];
    for m in 0..n {
        let (um, dm) = dirs[m];
        let pm = basis[m].pos;
        let m_plus = add(pm, scale(um, 0.5 * dm));
        let m_minus = sub(pm, scale(um, 0.5 * dm));
        for s in 0..n {
            let (us, ds) = dirs[s];
            let ps = basis[s].pos;
            let a = basis[s].radius;
            let cur_start = sub(ps, scale(us, 0.5 * ds));
            let minus_start = sub(ps, scale(us, ds));
            let vec_pot = psi(cur_start, us, ds, a, pm, k) * (dot(us, um) * ds * dm);
            let scalar = psi(ps, us, ds, a, m_plus, k) - psi(minus_start, us, ds, a, m_plus, k) - psi(ps, us, ds, a, m_minus, k)
                + psi(minus_start, us, ds, a, m_minus, k);
            z[m * n + s] = c_a * vec_pot + c_phi * scalar;
        }
    }
    // İletken kaybı: her tabana seri dağıtılmış iç empedans.
    let mut z_int = Vec::with_capacity(n);
    for (i, b) in basis.iter().enumerate() {
        let w = &antenna.wires[b.wire];
        let zi = w.material.internal_impedance(frequency, w.radius(), w.wall) * dirs[i].1;
        z[i * n + i] += zi;
        z_int.push(zi);
    }

    let mut v = vec![Complex64::new(0.0, 0.0); n];
    let mut feed_idx = Vec::new();
    for (si, src) in antenna.sources.iter().enumerate() {
        let w = antenna.wires.get(src.wire).ok_or(SolveError::BadSource(si))?;
        let m = w.segment_count(C0 / antenna.frequency);
        let node = ((src.position * m as f64).round() as usize).clamp(1, m - 1);
        let first = basis.iter().position(|b| b.wire == src.wire).ok_or(SolveError::BadSource(si))?;
        let idx = first + node - 1;
        v[idx] += src.voltage;
        feed_idx.push(idx);
    }

    let currents = Lu::factor(z, n).ok_or(SolveError::Singular)?.solve(&v);
    if currents.iter().any(|c| !c.re.is_finite() || !c.im.is_finite()) {
        return Err(SolveError::Singular);
    }

    let feeds: Vec<Feed> =
        feed_idx.iter().map(|&i| Feed { basis: i, voltage: v[i], current: currents[i], impedance: v[i] / currents[i] }).collect();
    let input_power = feeds.iter().map(|f| 0.5 * (f.voltage * f.current.conj()).re).sum();
    let loss_power = currents.iter().zip(&z_int).map(|(i, zi)| 0.5 * i.norm_sqr() * zi.re).sum();

    Ok(Solution { frequency, basis, currents, feeds, input_power, loss_power })
}

/// `start`..`stop` (Hz) arasında `points` noktada giriş empedansını hesaplar.
pub fn sweep(antenna: &Antenna, start: f64, stop: f64, points: usize) -> Result<Vec<SweepPoint>, SolveError> {
    let points = points.max(2);
    (0..points)
        .map(|i| {
            let f = start + (stop - start) * i as f64 / (points - 1) as f64;
            solve(antenna, f).map(|s| SweepPoint { frequency: f, impedance: s.impedance() })
        })
        .collect()
}
