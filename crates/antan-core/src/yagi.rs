//! Eleman bazlı Yagi-Uda tasarımı.
//!
//! Kullanıcı elemanları (bum konumu, uçtan uca boy) ve ortak bir boru
//! profilini düzenler; `build` bunu çözücünün anlayacağı tel modeline çevirir.
//! Kademeli (teleskopik) elemanlar: profil, ortadan dışa doğru bölümlerden
//! oluşur (her bölüm bir tarafta `half_length` uzunluğunda); kalan kısım uç
//! borusudur. Bölümler tel birleşimleriyle bağlanır.

use serde::{Deserialize, Serialize};

use crate::C0;
use crate::geometry::{Antenna, Ground, Load, LoadKind};
use crate::material::Material;
use crate::templates::{Tube, source, wire};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaperSection {
    /// Bölümün bir taraftaki uzunluğu (m).
    pub half_length: f64,
    pub tube: Tube,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Ortadan dışa doğru iç bölümler (boşsa eleman tek borudur).
    pub sections: Vec<TaperSection>,
    /// Son (uç) boru; elemanın geri kalanını doldurur.
    pub tip: Tube,
}

impl Profile {
    pub fn uniform(tube: Tube) -> Self {
        Self { sections: Vec::new(), tip: tube }
    }
    /// Elemanın yarı boyu `half` için parçalar: (iç uzaklık, dış uzaklık, boru).
    pub fn pieces(&self, half: f64) -> Vec<(f64, f64, &Tube)> {
        let mut out = Vec::new();
        let mut at = 0.0;
        for s in &self.sections {
            if at + s.half_length >= half - 1e-9 {
                // Profil elemandan uzun: kesildiği bölümün borusu uca kadar sürer.
                out.push((at, half, &s.tube));
                return out;
            }
            out.push((at, at + s.half_length, &s.tube));
            at += s.half_length;
        }
        out.push((at, half, &self.tip));
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YagiElement {
    pub name: String,
    /// Bum üzerindeki konum (m), yansıtıcı genelde 0.
    pub x: f64,
    /// Uçtan uca boy (m).
    pub length: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YagiDesign {
    pub name: String,
    pub frequency: f64,
    pub elements: Vec<YagiElement>,
    /// Sürülen elemanın sırası.
    pub driven: usize,
    pub profile: Profile,
    /// Bumun zeminden yüksekliği (m); yalnız zemin varken anlamlıdır.
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub ground: Ground,
    /// Bum ve elemanların bağlanış biçimi (yalnız kesim boyunu etkiler).
    #[serde(default)]
    pub boom: Boom,
    /// Elemanlara yerleştirilen toplu yükler (bobin, tuzak…).
    #[serde(default)]
    pub loads: Vec<ElementLoad>,
    /// LFA (G0KSC "Loop Fed Array"): sürülen eleman, eleman düzleminde yatan
    /// dikdörtgen halkadır. Değer halkanın bum boyunca genişliği (m); eleman
    /// boyu halkanın uzun kenarıdır. Besleme arka kenarın ortasından.
    #[serde(default)]
    pub driven_loop: Option<f64>,
}

/// Bir Yagi elemanına simetrik yerleştirilen yük.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementLoad {
    pub element: usize,
    /// Ortadan uca konum, yarı boyun kesri (0 = orta: tek yük; > 0: iki kolda birer yük).
    pub position: f64,
    pub kind: LoadKind,
}

impl YagiDesign {
    /// `n` elemanlı (en az 2) genel geçer başlangıç tasarımı.
    pub fn standard(frequency: f64, n: usize, profile: Profile) -> Self {
        let lambda = C0 / frequency;
        let n = n.max(2);
        let mut elements = vec![
            YagiElement { name: crate::i18n::t("Reflector", "Yansıtıcı").into(), x: 0.0, length: 0.495 * lambda },
            YagiElement { name: crate::i18n::t("Driven", "Sürülen").into(), x: 0.18 * lambda, length: 0.468 * lambda },
        ];
        let mut x = 0.18;
        for d in 0..n - 2 {
            x += if d == 0 { 0.12 } else { (0.20 + 0.03 * d as f64).min(0.32) };
            let len = (0.445 - 0.006 * d as f64).max(0.40);
            elements.push(YagiElement { name: crate::tf!("Director {}", "Yönlendirici {}", d + 1), x: x * lambda, length: len * lambda });
        }
        Self { name: crate::tf!("{n}-element Yagi", "{n} elemanlı Yagi"), frequency, elements, driven: 1, profile, height: lambda, ground: Ground::FreeSpace, boom: Boom::default(), loads: Vec::new(), driven_loop: None }
    }

    /// `n` elemanlı LFA Yagi başlangıç tasarımı (G0KSC tarzı halka beslemeli).
    /// Halka boyutları ve aralıklar bu çözücüyle ~50 Ω'a taranmıştır; en iyi
    /// sonuç için İyileştirme sekmesiyle ince ayar yapılması önerilir.
    pub fn lfa(frequency: f64, n: usize, profile: Profile) -> Self {
        let lambda = C0 / frequency;
        let mut d = Self::standard(frequency, n, profile);
        d.name = crate::tf!("{n}-element LFA Yagi", "{n} elemanlı LFA Yagi");
        let p = lfa_params(d.elements.len());
        d.elements[1].x = p.0 * lambda;
        d.elements[1].length = p.1 * lambda;
        d.elements[1].name = crate::i18n::t("Driven loop", "Sürülen halka").into();
        d.driven_loop = Some(p.2 * lambda);
        let shift = p.3 * lambda - d.elements.get(2).map(|e| e.x).unwrap_or(0.0);
        for e in d.elements.iter_mut().skip(2) {
            e.x += shift;
        }
        d
    }

    pub fn boom_length(&self) -> f64 {
        let (lo, hi) = self.elements.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), e| (lo.min(e.x), hi.max(e.x)));
        if lo.is_finite() { hi - lo } else { 0.0 }
    }

    /// Tel modeli. Döndürülen ikinci değer, her elemanın tellerinin aralığıdır.
    pub fn build_with_map(&self) -> (Antenna, Vec<std::ops::Range<usize>>) {
        let mut wires = Vec::new();
        let mut map = Vec::new();
        let mut feed = 0;
        let mut loads = Vec::new();
        for (i, e) in self.elements.iter().enumerate() {
            let first = wires.len();
            let pieces = self.profile.pieces(0.5 * e.length.max(1e-4));
            let x = e.x;
            let z = if self.ground.is_free() { 0.0 } else { self.height };
            let (_, c_out, c_tube) = pieces[0];
            if i == self.driven {
                feed = wires.len();
                if let Some(w) = self.driven_loop.filter(|w| *w > 0.0) {
                    // LFA halkası: arka kenar (beslenen), ön kenar ve iki kısa uç.
                    let (xr, xf, h) = (x - 0.5 * w, x + 0.5 * w, 0.5 * e.length.max(1e-4));
                    let t = c_tube;
                    wires.push(wire(&crate::tf!("{} rear (fed)", "{} arka (beslenen)", e.name), [xr, -h, z], [xr, h, z], t));
                    wires.push(wire(&crate::tf!("{} front", "{} ön", e.name), [xf, -h, z], [xf, h, z], t));
                    wires.push(wire(&crate::tf!("{} end right", "{} sağ uç", e.name), [xr, h, z], [xf, h, z], t));
                    wires.push(wire(&crate::tf!("{} end left", "{} sol uç", e.name), [xr, -h, z], [xf, -h, z], t));
                    map.push(first..wires.len());
                    continue;
                }
            }
            wires.push(wire(&e.name, [x, -c_out, z], [x, c_out, z], c_tube));
            for (k, &(a, b, t)) in pieces.iter().enumerate().skip(1) {
                wires.push(wire(&crate::tf!("{} ·{} right", "{} ·{} sağ", e.name, k + 1), [x, a, z], [x, b, z], t));
                wires.push(wire(&crate::tf!("{} ·{} left", "{} ·{} sol", e.name, k + 1), [x, -a, z], [x, -b, z], t));
            }
            map.push(first..wires.len());
            for ld in self.loads.iter().filter(|l| l.element == i) {
                let d = ld.position.clamp(0.0, 1.0) * 0.5 * e.length;
                if d <= c_out + 1e-12 {
                    if d < 1e-9 {
                        loads.push(Load { wire: first, position: 0.5, kind: ld.kind });
                    } else {
                        let dt = d / (2.0 * c_out);
                        loads.push(Load { wire: first, position: 0.5 + dt, kind: ld.kind });
                        loads.push(Load { wire: first, position: 0.5 - dt, kind: ld.kind });
                    }
                } else if let Some(k) = pieces.iter().position(|&(a, b, _)| d > a && d <= b + 1e-12) {
                    let (a, b, _) = pieces[k];
                    let t = (d - a) / (b - a);
                    loads.push(Load { wire: first + 2 * k - 1, position: t, kind: ld.kind });
                    loads.push(Load { wire: first + 2 * k, position: t, kind: ld.kind });
                }
            }
        }
        (Antenna { name: self.name.clone(), frequency: self.frequency, wires, sources: vec![source(feed, 0.5)], ground: self.ground, loads }, map)
    }

    pub fn build(&self) -> Antenna {
        self.build_with_map().0
    }

    /// LFA halkasının genişliği (sürülen eleman halka değilse `None`).
    pub fn loop_width(&self) -> Option<f64> {
        self.driven_loop.filter(|w| *w > 0.0)
    }

    /// Sürülen elemanın boyunu, besleme reaktansı hedefe eşit olana kadar ayarlar
    /// (sekant yöntemi, gerçek çözücüyle). `target(Z)` istenen reaktansı verir.
    /// Başarılıysa yeni tasarım ve son empedans döner.
    pub fn tune_driven(&self, target: impl Fn(num_complex::Complex64) -> Option<f64>) -> Option<(YagiDesign, num_complex::Complex64)> {
        let f = self.frequency;
        let eval = |len: f64| -> Option<(f64, num_complex::Complex64)> {
            let mut d = self.clone();
            d.elements[self.driven].length = len;
            let z = crate::solver::solve(&d.build(), f).ok()?.impedance();
            Some((z.im - target(z)?, z))
        };
        let mut l0 = self.elements.get(self.driven)?.length;
        let (mut f0, _) = eval(l0)?;
        let mut l1 = l0 * if f0 > 0.0 { 0.99 } else { 1.01 };
        for _ in 0..25 {
            let (f1, z1) = eval(l1)?;
            if f1.abs() < 0.05 {
                let mut d = self.clone();
                d.elements[self.driven].length = l1;
                return Some((d, z1));
            }
            let slope = (f1 - f0) / (l1 - l0);
            if slope.abs() < 1e-12 {
                return None;
            }
            let step = (-f1 / slope).clamp(-0.05 * l1, 0.05 * l1);
            (l0, f0) = (l1, f1);
            l1 += step;
        }
        None
    }

    /// Tüm boyutları `k` ile ölçekler (boru çapları hariç).
    pub fn scale(&mut self, k: f64) {
        for e in &mut self.elements {
            e.x *= k;
            e.length *= k;
        }
        for s in &mut self.profile.sections {
            s.half_length *= k;
        }
    }

    /// Elemanları bum konumuna göre sıralar (sürülen eleman izlenir).
    pub fn sort(&mut self) {
        let driven_name = self.elements.get(self.driven).map(|e| (e.name.clone(), e.x, e.length));
        self.elements.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((n, x, l)) = driven_name {
            self.driven = self.elements.iter().position(|e| e.name == n && e.x == x && e.length == l).unwrap_or(0);
        }
    }
}

// ------------------------------------------------------------------ bum

/// Elemanın bumla bağlantısı.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BoomMount {
    /// Bum yalıtkan (fiberglas, ahşap) ya da eleman bumdan uzakta: düzeltme yok.
    None,
    /// Eleman metal bumun içinden geçer ve bumla elektriksel temastadır.
    ThroughBonded,
    /// Eleman metal bumun içinden yalıtkan burçla geçer.
    ThroughInsulated,
    /// Kullanıcının verdiği düzeltme (m), ör. bum üstüne yalıtkanla monte.
    Custom(f64),
}

impl BoomMount {
    pub fn label(&self) -> &'static str {
        match self {
            BoomMount::None => crate::i18n::t("No correction (insulating boom / clear of boom)", "Düzeltme yok (yalıtkan bum / uzakta)"),
            BoomMount::ThroughBonded => crate::i18n::t("Through boom, bonded", "Bumdan geçen, temaslı"),
            BoomMount::ThroughInsulated => crate::i18n::t("Through boom, insulated", "Bumdan geçen, yalıtılmış"),
            BoomMount::Custom(_) => crate::i18n::t("User defined", "Kullanıcı tanımlı"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boom {
    /// Dış çap (kare bumda kenar), m.
    pub diameter: f64,
    pub square: bool,
    /// Et kalınlığı (m); kütle hesabı için.
    pub wall: f64,
    pub material: Material,
    /// Parazitik elemanların bağlanışı.
    pub mount: BoomMount,
    /// Sürülen elemanın bağlanışı (çoğunlukla yalıtılmış ya da bumun üstünde).
    pub driven_mount: BoomMount,
    /// Bumun uç elemanlardan dışarı taşan kısmı, her uçta (m).
    pub overhang: f64,
    /// Kademeli borularda geçme payı (m): dış bölüm içtekine bu kadar girer.
    pub overlap: f64,
}

impl Default for Boom {
    fn default() -> Self {
        Self {
            diameter: 0.025,
            square: false,
            wall: 0.002,
            material: Material::aluminum_6063(),
            mount: BoomMount::ThroughBonded,
            driven_mount: BoomMount::ThroughInsulated,
            overhang: 0.03,
            overlap: 0.03,
        }
    }
}

/// G3SEK'in DL6WU ölçümlerine uydurduğu bum düzeltmesi: bumdan geçen ve bumla
/// temaslı elemanda tüm eleman boyuna eklenecek miktar, bum çapının kesri olarak.
/// `b` bum çapı (dalga boyu). Ölçümler b < 0.055 λ için geçerlidir.
/// Yalıtılmış geçişte bunun yarısı kullanılır (12.5975·b − 114.5·b², SM5BSZ).
pub fn dl6wu_fraction(b: f64) -> f64 {
    (25.195 * b - 229.0 * b * b).max(0.0)
}

/// DL6WU/G3SEK eğrisinin geçerli olduğu en büyük bum çapı (λ).
pub const DL6WU_MAX_BOOM_WL: f64 = 0.055;

impl Boom {
    /// Bir bağlanış için tüm eleman boyuna eklenecek düzeltme (m).
    pub fn correction(&self, mount: BoomMount, frequency: f64) -> f64 {
        let b = self.diameter / (C0 / frequency);
        let full = dl6wu_fraction(b) * self.diameter;
        match mount {
            BoomMount::None => 0.0,
            BoomMount::ThroughBonded => full,
            BoomMount::ThroughInsulated => 0.5 * full,
            BoomMount::Custom(m) => m,
        }
    }
    /// Bum çapı formülün ölçüm aralığında mı?
    pub fn in_range(&self, frequency: f64) -> bool {
        self.diameter / (C0 / frequency) <= DL6WU_MAX_BOOM_WL
    }
    /// Bum kütlesi (kg) ve rüzgâr alanı (m²), `length` bum boyu için.
    pub fn mass_and_area(&self, length: f64) -> (f64, f64) {
        let d = self.diameter;
        let inner = (d - 2.0 * self.wall).max(0.0);
        let section = if self.square { d * d - inner * inner } else { std::f64::consts::PI / 4.0 * (d * d - inner * inner) };
        (section * length * self.material.density, d * length)
    }
}

/// Kesim listesinin bir satırı.
#[derive(Clone, Debug, PartialEq)]
pub struct CutPiece {
    /// Parça açıklaması (ör. crate::i18n::t("center", "orta"), "2. bölüm", crate::i18n::t("tip", "uç")).
    pub label: String,
    /// Adet (orta parça 1, yan parçalar 2).
    pub count: usize,
    pub tube: Tube,
    /// Kesilecek boy (m), geçme payı ve uçta bum düzeltmesi dahil.
    pub length: f64,
}

impl YagiDesign {
    /// Elemanın bum düzeltmesi (m).
    pub fn boom_correction(&self, i: usize) -> f64 {
        if i == self.driven && self.loop_width().is_some() {
            return 0.0;
        }
        let mount = if i == self.driven { self.boom.driven_mount } else { self.boom.mount };
        self.boom.correction(mount, self.frequency)
    }
    /// Kesilecek uçtan uca boy: elektriksel boy + bum düzeltmesi.
    pub fn cut_length(&self, i: usize) -> f64 {
        self.elements[i].length + self.boom_correction(i)
    }
    /// Bum boyu (uç elemanlar arası + iki taraftaki taşma).
    pub fn boom_total_length(&self) -> f64 {
        self.boom_length() + 2.0 * self.boom.overhang
    }
    /// Bir elemanın boru kesim listesi. Düzeltme uç borularına eşit bölünür;
    /// ortadan sonraki her bölüm geçme payı kadar uzun kesilir.
    pub fn cut_list(&self, i: usize) -> Vec<CutPiece> {
        let e = &self.elements[i];
        let pieces = self.profile.pieces(0.5 * e.length);
        if let (true, Some(w)) = (i == self.driven, self.loop_width()) {
            // LFA halkası: bum düzeltmesi uygulanmaz (halka genelde yalıtkanla monte edilir).
            let t = pieces[0].2.clone();
            return vec![
                CutPiece { label: crate::i18n::t("loop long side (rear one split at feed)", "halka uzun kenar (arkadaki beslemede bölünür)").into(), count: 2, tube: t.clone(), length: e.length },
                CutPiece { label: crate::i18n::t("loop end", "halka kısa kenar").into(), count: 2, tube: t, length: w },
            ];
        }
        let half_corr = 0.5 * self.boom_correction(i);
        let last = pieces.len() - 1;
        pieces
            .iter()
            .enumerate()
            .map(|(k, &(a, b, t))| {
                let extra = if k == last { half_corr } else { 0.0 };
                if k == 0 {
                    // Orta boru tek parçadır; tek bölümlü elemanda düzeltme iki uca da eklenir.
                    CutPiece { label: if last == 0 { crate::i18n::t("one piece", "tek parça").into() } else { crate::i18n::t("center", "orta").into() }, count: 1, tube: t.clone(), length: 2.0 * (b + extra) }
                } else {
                    let label = if k == last { crate::i18n::t("tip", "uç").to_string() } else { crate::tf!("section {}", "{}. bölüm", k + 1) };
                    CutPiece { label, count: 2, tube: t.clone(), length: b - a + extra + self.boom.overlap }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod boom_tests {
    use super::*;

    #[test]
    fn dl6wu_reference_values() {
        // Kaynak kontrolü: 0.01 λ bumda temaslı düzeltme bum çapının %22.9'u.
        assert!((dl6wu_fraction(0.01) - 0.22905).abs() < 1e-9);
        // SM5BSZ tablosu: 19.05 mm bum, 144 MHz, yalıtılmış → %10.56 → +2.01 mm.
        let boom = Boom { diameter: 0.01905, mount: BoomMount::ThroughInsulated, ..Boom::default() };
        let c = boom.correction(BoomMount::ThroughInsulated, 144.0e6);
        assert!((c * 1000.0 - 2.01).abs() < 0.02, "{}", c * 1000.0);
        assert!(boom.in_range(144.0e6));
        assert!(!Boom { diameter: 0.2, ..Boom::default() }.in_range(144.0e6));
    }

    #[test]
    fn cut_list_adds_correction_and_overlap() {
        let tube = Tube::aluminum(0.012, Some(0.001));
        let thin = Tube::aluminum(0.010, Some(0.001));
        let mut d = YagiDesign::standard(144.3e6, 3, Profile { sections: vec![TaperSection { half_length: 0.2, tube }], tip: thin });
        d.boom.overlap = 0.03;
        let corr = d.boom_correction(0);
        assert!(corr > 0.0);
        let list = d.cut_list(0);
        assert_eq!(list.len(), 2);
        assert!((list[0].length - 0.4).abs() < 1e-12);
        // Görünen toplam = elektriksel boy + düzeltme.
        let visible = list[0].length + 2.0 * (list[1].length - d.boom.overlap);
        assert!((visible - d.cut_length(0)).abs() < 1e-12);
        // Tek parçalı eleman.
        let u = YagiDesign::standard(144.3e6, 3, Profile::uniform(Tube::aluminum(0.01, None)));
        let l = u.cut_list(2);
        assert_eq!(l.len(), 1);
        assert!((l[0].length - u.cut_length(2)).abs() < 1e-12);
        // Sürülen eleman varsayılan olarak yalıtılmış: düzeltmesi parazitiğin yarısı.
        assert!((u.boom_correction(1) - 0.5 * u.boom_correction(0)).abs() < 1e-15);
    }
}

/// LFA başlangıç boyutları (λ): (halka merkezinin bum konumu, halka uzun kenarı,
/// halka genişliği, ilk yönlendiricinin konumu). Eleman sayısına göre bu
/// çözücüyle taranmıştır (examples/lfa_scan.rs): ~45–55 Ω, yönlendiriciler
/// standart tasarımdan; Ö/A için İyileştirme sekmesi önerilir.
fn lfa_params(n: usize) -> (f64, f64, f64, f64) {
    match n {
        0..=3 => (0.12, 0.42, 0.08, 0.30),
        4 => (0.08, 0.42, 0.06, 0.22),
        5 => (0.12, 0.40, 0.10, 0.26),
        6 => (0.10, 0.44, 0.04, 0.30),
        7 | 8 => (0.08, 0.42, 0.06, 0.22),
        _ => (0.10, 0.42, 0.06, 0.30),
    }
}
