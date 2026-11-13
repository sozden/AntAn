//! İnce tel moment yöntemi (MoM) çözücüsü — serbest uzay ya da zemin üzerinde.
//!
//! Formülasyon: Harrington'ın karışık potansiyelli (vektör + skaler potansiyel)
//! darbe tabanlı / nokta eşlemeli yöntemi. Her tel M eşit parçaya bölünür;
//! bilinmeyenler düğüm akımlarıdır.
//!
//! Her taban (düğüm p) iki parçaya dayanır: p'de biten "eksi" parça ve p'den
//! başlayan "artı" parça. Tel içindeki düğümlerde ikisi aynı teldedir; tel
//! birleşimlerinde (köşe, T, radyal) farklı tellerdedir. N telin buluştuğu bir
//! noktada N−1 taban kurulur (hepsi ilk tele bağlı); böylece Kirchhoff akım
//! yasası kendiliğinden sağlanır. Serbest uçlarda akım sıfırdır.
//!
//!   Z_mn = jωμ Σ_yarım (û·Δl_m) ∫g ds/4π
//!        + 1/(jωε) [ψ(n+,m+) − ψ(n−,m+) − ψ(n+,m−) + ψ(n−,m−)]
//!
//!   ψ = 1/(4πΔ) ∫ e^{−jkR}/R ds,  R = √(ρ² + a² + (z−s)²)  (indirgenmiş çekirdek)
//!
//! Tekil kısım (1/R) analitik, kalan düzgün kısım Gauss-Legendre ile alınır.
//! Zaman bağımlılığı e^{jωt}; endüktif reaktans pozitiftir.
//!
//! Zemin (z = 0): kusursuz iletken zeminin görüntü yöntemi. Her kaynak tabanın
//! z'ye göre aynası, akımı ve yükü ters işaretli olarak eklenir (yatay akım
//! ters, düşey akım aynı yönde görünür). Zemine değen tel uçlarında yalnız
//! gerçek yarıdan oluşan bir "zemin tabanı" kurulur; eksik yarıyı görüntüsü
//! tamamlar, böylece akım zemine akabilir ve uç beslenebilir (monopol).
//! Gerçek zemin akımlar için kusursuz kabul edilir; farkı uzak alanda Fresnel
//! katsayılarıyla girer (bkz. `farfield`).
//!
//! Sınırlar: birleşimde çapı farklı teller indirgenmiş çekirdekle yaklaşık modellenir.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::geometry::{Antenna, Ground, Vec3, add, dot, norm, scale, sub};
use crate::linalg::Lu;
use crate::{C0, EPS0, MU0};

#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    NoWires,
    NoSources,
    DegenerateWire(usize),
    BadSource(usize),
    BelowGround(usize),
    BadLoad(usize),
    Singular,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWires => write!(f, "Modelde eleman yok"),
            Self::NoSources => write!(f, "Besleme kaynağı tanımlı değil"),
            Self::DegenerateWire(i) => write!(f, "{}. telin boyu ya da çapı geçersiz", i + 1),
            Self::BadSource(i) => write!(f, "{}. kaynak geçerli bir noktada değil (serbest uç ya da olmayan tel)", i + 1),
            Self::BelowGround(i) => write!(f, "{}. tel zeminin (z = 0) altına iniyor; yüksekliği artırın", i + 1),
            Self::BadLoad(i) => write!(f, "{}. yük geçerli bir noktada değil (serbest uç, çoklu birleşim ya da olmayan tel)", i + 1),
            Self::Singular => write!(f, "Empedans matrisi tekil; geometri çözülemedi"),
        }
    }
}
impl std::error::Error for SolveError {}

/// Düz bir tel parçası.
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub start: Vec3,
    /// Birim yön.
    pub dir: Vec3,
    pub len: f64,
    pub radius: f64,
    pub wire: usize,
}

impl Seg {
    pub fn point(&self, s: f64) -> Vec3 {
        add(self.start, scale(self.dir, s))
    }
    pub fn mid(&self) -> Vec3 {
        self.point(0.5 * self.len)
    }
    /// z = 0 düzlemine göre ayna görüntüsü.
    pub fn mirror(&self) -> Seg {
        Seg { start: mirror(self.start), dir: mirror(self.dir), ..*self }
    }
}

pub fn mirror(p: Vec3) -> Vec3 {
    [p[0], p[1], -p[2]]
}

/// Bir tabanın bir tele dokunduğu yer: (tel, düğümün tel üzerindeki konumu 0..1,
/// akım yönü işareti: +1 tel yönünde, −1 ters). Tel sırası `usize::MAX` ise boştur.
pub type Touch = (usize, f64, f64);

/// Bir akım tabanı.
#[derive(Clone, Copy, Debug)]
pub struct Basis {
    pub pos: Vec3,
    /// p'de biten parça (yönü p'ye doğru). Zemin tabanında kullanılmaz.
    pub minus: Seg,
    /// p'den başlayan parça (yönü p'den uzağa).
    pub plus: Seg,
    /// Eksi ve artı tarafın dokunduğu teller. Tel içi tabanlarda ikisi aynıdır.
    pub touch: [Touch; 2],
    /// Tel ucunda (birleşim ya da zemin).
    pub junction: bool,
    /// Zemine bağlı yarım taban (yalnız artı yarı gerçektir).
    pub ground: bool,
}

impl Basis {
    fn half_plus(&self) -> Seg {
        Seg { len: 0.5 * self.plus.len, ..self.plus }
    }
    fn half_minus(&self) -> Seg {
        let h = 0.5 * self.minus.len;
        Seg { start: sub(self.pos, scale(self.minus.dir, h)), len: h, ..self.minus }
    }
    /// Akım taşıyan gerçek yarım parçalar.
    pub fn current_halves(&self) -> Vec<Seg> {
        if self.ground { vec![self.half_plus()] } else { vec![self.half_minus(), self.half_plus()] }
    }
    /// Test yolu vektörü (gerçek yarıların toplamı).
    pub fn dl(&self) -> Vec3 {
        self.current_halves().iter().fold([0.0; 3], |acc, s| add(acc, scale(s.dir, s.len)))
    }
}

/// Uzak alan için akım elemanı: (taban, orta nokta, yön × uzunluk).
pub type CurrentElement = (usize, Vec3, Vec3);

/// Bir besleme noktasındaki sonuç.
#[derive(Clone, Debug)]
pub struct Feed {
    pub source: usize,
    pub voltage: Complex64,
    pub current: Complex64,
    pub impedance: Complex64,
    /// Besleme düğümünün konumu.
    pub pos: Vec3,
}

/// Bir yükün sonucu.
#[derive(Clone, Debug)]
pub struct LoadResult {
    pub pos: Vec3,
    /// Yük empedansı (bu frekansta).
    pub impedance: Complex64,
    /// Yükten geçen akım.
    pub current: Complex64,
    /// Yükte harcanan güç (W).
    pub power: f64,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub frequency: f64,
    pub basis: Vec<Basis>,
    /// Her tabanın akımı (A, tepe değer).
    pub currents: Vec<Complex64>,
    /// Gerçek akım elemanları (görüntüler hariç).
    pub elements: Vec<CurrentElement>,
    pub feeds: Vec<Feed>,
    /// Kaynaklardan giren toplam güç (W).
    pub input_power: f64,
    /// İletken kaybı (W).
    pub loss_power: f64,
    /// Tel sayısı.
    pub wire_count: usize,
    pub ground: Ground,
    pub loads: Vec<LoadResult>,
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
    /// Bir tel boyunca akım dağılımı: (konum 0..1, akım; tel yönünde pozitif).
    /// Serbest uçlarda sıfır, birleşim/zemin uçlarında oradaki akım.
    pub fn wire_currents(&self, wire: usize) -> Vec<(f64, Complex64)> {
        let zero = Complex64::new(0.0, 0.0);
        let (mut i0, mut i1) = (zero, zero);
        let mut v = Vec::new();
        for (b, i) in self.basis.iter().zip(&self.currents) {
            if !b.junction {
                if b.touch[0].0 == wire {
                    v.push((b.touch[0].1, *i));
                }
                continue;
            }
            for &(w, t, s) in &b.touch {
                if w == wire {
                    if t < 0.5 { i0 += *i * s } else { i1 += *i * s }
                }
            }
        }
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        v.insert(0, (0.0, i0));
        v.push((1.0, i1));
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

fn psi_seg(s: &Seg, obs: Vec3, k: f64) -> Complex64 {
    psi(s.start, s.dir, s.len, s.radius, obs, k)
}

/// Parçalanmış model.
pub struct Mesh {
    pub basis: Vec<Basis>,
    /// Her telin ilk iç tabanının sırası ve parça sayısı.
    pub wire_first: Vec<(usize, usize)>,
}

/// Tel uç noktalarının çakışması için tolerans (m).
fn junction_tolerance(antenna: &Antenna, lambda: f64) -> f64 {
    let shortest = antenna.wires.iter().map(|w| w.length() / w.segment_count(lambda) as f64).fold(f64::INFINITY, f64::min);
    (1e-3 * shortest).max(1e-9)
}

const NONE_TOUCH: Touch = (usize::MAX, 0.0, 0.0);

pub fn mesh(antenna: &Antenna) -> Result<Mesh, SolveError> {
    if antenna.wires.is_empty() {
        return Err(SolveError::NoWires);
    }
    let lambda = C0 / antenna.frequency;
    let tol = junction_tolerance(antenna, lambda);
    let grounded = !antenna.ground.is_free();
    let mut basis = Vec::new();
    let mut wire_first = Vec::new();
    // Uç parçaları: (tel, uç: 0 başlangıç / 1 bitiş, uç noktası, telin yönü, parça boyu, yarıçap)
    let mut ends: Vec<(usize, u8, Vec3, Vec3, f64, f64)> = Vec::new();
    for (wi, w) in antenna.wires.iter().enumerate() {
        let len = w.length();
        if !(len > 0.0) || !(w.diameter > 0.0) || !len.is_finite() {
            return Err(SolveError::DegenerateWire(wi));
        }
        if grounded && (w.start[2] < -tol || w.end[2] < -tol) {
            return Err(SolveError::BelowGround(wi));
        }
        let m = w.segment_count(lambda);
        let u = w.direction();
        let delta = len / m as f64;
        let a = w.radius();
        wire_first.push((basis.len(), m));
        let seg = |k: usize| Seg { start: add(w.start, scale(u, delta * k as f64)), dir: u, len: delta, radius: a, wire: wi };
        for i in 1..m {
            let t = i as f64 / m as f64;
            basis.push(Basis {
                pos: add(w.start, scale(u, len * t)),
                minus: seg(i - 1),
                plus: seg(i),
                touch: [(wi, t, 1.0), (wi, t, 1.0)],
                junction: false,
                ground: false,
            });
        }
        ends.push((wi, 0, w.start, u, delta, a));
        ends.push((wi, 1, w.end, u, delta, a));
    }

    // Bir ucu düğümden dışa doğru yönelmiş parça ve dokunma bilgisi.
    let outward = |e: &(usize, u8, Vec3, Vec3, f64, f64), node: Vec3| -> (Seg, Touch) {
        let (w, end, _, u, d, a) = *e;
        if end == 0 { (Seg { start: node, dir: u, len: d, radius: a, wire: w }, (w, 0.0, 1.0)) } else { (Seg { start: node, dir: scale(u, -1.0), len: d, radius: a, wire: w }, (w, 1.0, -1.0)) }
    };

    // Birleşimler: çakışan uçları grupla.
    let mut used = vec![false; ends.len()];
    for i in 0..ends.len() {
        if used[i] {
            continue;
        }
        let mut group = vec![i];
        for j in i + 1..ends.len() {
            if !used[j] && norm(sub(ends[i].2, ends[j].2)) < tol {
                group.push(j);
            }
        }
        let node = ends[i].2;
        let on_ground = grounded && node[2].abs() < tol;
        if group.len() < 2 && !on_ground {
            continue;
        }
        for &g in &group {
            used[g] = true;
        }
        let node = if on_ground { [node[0], node[1], 0.0] } else { node };
        // Referans uç: eksi taraf (düğüme doğru akan).
        let (out0, t_out) = outward(&ends[i], node);
        let minus = Seg { start: out0.point(out0.len), dir: scale(out0.dir, -1.0), ..out0 };
        let touch0 = (t_out.0, t_out.1, -t_out.2);
        for &g in &group[1..] {
            let (plus, touch1) = outward(&ends[g], node);
            basis.push(Basis { pos: node, minus, plus, touch: [touch0, touch1], junction: true, ground: false });
        }
        if on_ground {
            // Referans telden zemine akım: yalnız gerçek artı yarı.
            basis.push(Basis { pos: node, minus: out0, plus: out0, touch: [t_out, NONE_TOUCH], junction: true, ground: true });
        }
    }
    Ok(Mesh { basis, wire_first })
}

/// Bir kaynağın uyardığı tabanlar ve işaretleri.
fn source_bases(antenna: &Antenna, mesh: &Mesh, si: usize) -> Result<(Vec<(usize, f64)>, Vec3), SolveError> {
    let src = &antenna.sources[si];
    let w = antenna.wires.get(src.wire).ok_or(SolveError::BadSource(si))?;
    let (first, m) = mesh.wire_first[src.wire];
    let node = (src.position.clamp(0.0, 1.0) * m as f64).round_ties_even() as usize;
    if node >= 1 && node < m {
        let idx = first + node - 1;
        return Ok((vec![(idx, 1.0)], mesh.basis[idx].pos));
    }
    // Tel ucunda: o uca dokunan birleşim/zemin tabanlarının hepsi.
    let t_end = if node == 0 { 0.0 } else { 1.0 };
    let mut out = Vec::new();
    for (i, b) in mesh.basis.iter().enumerate().filter(|(_, b)| b.junction) {
        for &(tw, tt, s) in &b.touch {
            if tw == src.wire && (tt - t_end).abs() < 1e-9 {
                out.push((i, s));
            }
        }
    }
    if out.is_empty() {
        return Err(SolveError::BadSource(si));
    }
    Ok((out, if node == 0 { w.start } else { w.end }))
}

/// Bir yükün oturduğu taban: tel içi düğüm ya da iki telin birleştiği uç.
fn load_basis(antenna: &Antenna, mesh: &Mesh, li: usize) -> Result<(usize, f64), SolveError> {
    let ld = &antenna.loads[li];
    if ld.wire >= antenna.wires.len() {
        return Err(SolveError::BadLoad(li));
    }
    let (first, m) = mesh.wire_first[ld.wire];
    let node = (ld.position.clamp(0.0, 1.0) * m as f64).round_ties_even() as usize;
    if node >= 1 && node < m {
        return Ok((first + node - 1, 1.0));
    }
    let t_end = if node == 0 { 0.0 } else { 1.0 };
    let hits: Vec<(usize, f64)> = mesh
        .basis
        .iter()
        .enumerate()
        .filter(|(_, b)| b.junction && !b.ground)
        .flat_map(|(i, b)| b.touch.iter().filter(|t| t.0 == ld.wire && (t.1 - t_end).abs() < 1e-9).map(move |t| (i, t.2)).collect::<Vec<_>>())
        .collect();
    match hits.as_slice() {
        [one] => Ok(*one),
        _ => Err(SolveError::BadLoad(li)),
    }
}

/// Kaynak tarafı: akım yarıları ve yük parçaları (işaretli).
struct Source {
    halves: Vec<(Seg, f64)>,
    charges: Vec<(Seg, f64)>,
}

impl Source {
    fn of(b: &Basis) -> Self {
        if b.ground {
            Self { halves: vec![(b.half_plus(), 1.0)], charges: vec![(b.plus, 1.0)] }
        } else {
            Self { halves: vec![(b.half_minus(), 1.0), (b.half_plus(), 1.0)], charges: vec![(b.plus, 1.0), (b.minus, -1.0)] }
        }
    }
    /// Kusursuz zemin görüntüsü: aynalanmış geometri, ters işaretli akım ve yük.
    fn image(&self) -> Self {
        let m = |v: &Vec<(Seg, f64)>| v.iter().map(|(s, w)| (s.mirror(), -w)).collect();
        Self { halves: m(&self.halves), charges: m(&self.charges) }
    }
}

/// Anteni `frequency` (Hz) frekansında çözer. Parçalama her zaman anten
/// tasarım frekansına göre yapılır; böylece frekans taraması pürüzsüz olur.
pub fn solve(antenna: &Antenna, frequency: f64) -> Result<Solution, SolveError> {
    if antenna.sources.is_empty() {
        return Err(SolveError::NoSources);
    }
    let mesh = mesh(antenna)?;
    let basis = &mesh.basis;
    let n = basis.len();
    if n == 0 {
        return Err(SolveError::Singular);
    }
    let grounded = !antenna.ground.is_free();
    let omega = 2.0 * PI * frequency;
    let k = omega / C0;
    let j = Complex64::new(0.0, 1.0);
    let c_a = j * omega * MU0;
    let c_phi = 1.0 / (j * omega * EPS0);

    let mut sources: Vec<Source> = basis.iter().map(Source::of).collect();
    if grounded {
        // Görüntüleri aynı listeye ekle: gerçek + görüntü katkısı birlikte toplanır.
        for s in &mut sources {
            let img = s.image();
            s.halves.extend(img.halves);
            s.charges.extend(img.charges);
        }
    }

    let mut z = vec![Complex64::new(0.0, 0.0); n * n];
    for m in 0..n {
        let bm = &basis[m];
        let pm = bm.pos;
        let dl_m = bm.dl();
        let t_plus = bm.plus.mid();
        let t_minus = if bm.ground { bm.pos } else { bm.minus.mid() };
        for (s, src) in sources.iter().enumerate() {
            let mut vec_pot = Complex64::new(0.0, 0.0);
            for (h, w) in &src.halves {
                vec_pot += psi_seg(h, pm, k) * (w * h.len * dot(h.dir, dl_m));
            }
            let mut scalar = Complex64::new(0.0, 0.0);
            for (c, w) in &src.charges {
                scalar += (psi_seg(c, t_plus, k) - psi_seg(c, t_minus, k)) * *w;
            }
            z[m * n + s] = c_a * vec_pot + c_phi * scalar;
        }
    }
    // İletken kaybı: her tabana seri dağıtılmış iç empedans (yalnız gerçek yarılar).
    let mut z_int = Vec::with_capacity(n);
    for (i, b) in basis.iter().enumerate() {
        let zi: Complex64 = b
            .current_halves()
            .iter()
            .map(|s| {
                let w = &antenna.wires[s.wire];
                w.material.internal_impedance(frequency, w.radius(), w.wall) * s.len
            })
            .sum();
        z[i * n + i] += zi;
        z_int.push(zi);
    }

    // Toplu yükler: düğümde seri empedans → öz empedansa eklenir.
    let mut load_map = Vec::with_capacity(antenna.loads.len());
    for li in 0..antenna.loads.len() {
        let (idx, sign) = load_basis(antenna, &mesh, li)?;
        let zl = antenna.loads[li].kind.impedance(frequency);
        z[idx * n + idx] += zl;
        load_map.push((idx, sign, zl));
    }

    let mut v = vec![Complex64::new(0.0, 0.0); n];
    let mut feed_map = Vec::new();
    for si in 0..antenna.sources.len() {
        let (bases, pos) = source_bases(antenna, &mesh, si)?;
        for &(idx, s) in &bases {
            v[idx] += antenna.sources[si].voltage * s;
        }
        feed_map.push((bases, pos));
    }

    let currents = Lu::factor(z, n).ok_or(SolveError::Singular)?.solve(&v);
    if currents.iter().any(|c| !c.re.is_finite() || !c.im.is_finite()) {
        return Err(SolveError::Singular);
    }

    let feeds: Vec<Feed> = feed_map
        .into_iter()
        .enumerate()
        .map(|(si, (bases, pos))| {
            let voltage = antenna.sources[si].voltage;
            let current: Complex64 = bases.iter().map(|&(i, s)| currents[i] * s).sum();
            Feed { source: si, voltage, current, impedance: voltage / current, pos }
        })
        .collect();
    let input_power = feeds.iter().map(|f| 0.5 * (f.voltage * f.current.conj()).re).sum();
    let loads: Vec<LoadResult> = load_map
        .iter()
        .map(|&(idx, sign, zl)| {
            let current = currents[idx] * sign;
            LoadResult { pos: basis[idx].pos, impedance: zl, current, power: 0.5 * current.norm_sqr() * zl.re }
        })
        .collect();
    let loss_power = currents.iter().zip(&z_int).map(|(i, zi)| 0.5 * i.norm_sqr() * zi.re).sum::<f64>() + loads.iter().map(|l| l.power).sum::<f64>();
    let elements = basis.iter().enumerate().flat_map(|(i, b)| b.current_halves().into_iter().map(move |s| (i, s.mid(), scale(s.dir, s.len)))).collect();

    Ok(Solution { frequency, basis: mesh.basis, currents, elements, feeds, input_power, loss_power, wire_count: antenna.wires.len(), ground: antenna.ground, loads })
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
