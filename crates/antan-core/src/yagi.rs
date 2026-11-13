//! Eleman bazlı Yagi-Uda tasarımı.
//!
//! Kullanıcı elemanları (bum konumu, uçtan uca boy) ve ortak bir boru
//! profilini düzenler; `build` bunu çözücünün anlayacağı tel modeline çevirir.
//! Kademeli (teleskopik) elemanlar: profil, ortadan dışa doğru bölümlerden
//! oluşur (her bölüm bir tarafta `half_length` uzunluğunda); kalan kısım uç
//! borusudur. Bölümler tel birleşimleriyle bağlanır.

use serde::{Deserialize, Serialize};

use crate::C0;
use crate::geometry::Antenna;
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
}

impl YagiDesign {
    /// `n` elemanlı (en az 2) genel geçer başlangıç tasarımı.
    pub fn standard(frequency: f64, n: usize, profile: Profile) -> Self {
        let lambda = C0 / frequency;
        let n = n.max(2);
        let mut elements = vec![
            YagiElement { name: "Yansıtıcı".into(), x: 0.0, length: 0.495 * lambda },
            YagiElement { name: "Sürülen".into(), x: 0.18 * lambda, length: 0.468 * lambda },
        ];
        let mut x = 0.18;
        for d in 0..n - 2 {
            x += if d == 0 { 0.12 } else { (0.20 + 0.03 * d as f64).min(0.32) };
            let len = (0.445 - 0.006 * d as f64).max(0.40);
            elements.push(YagiElement { name: format!("Yönlendirici {}", d + 1), x: x * lambda, length: len * lambda });
        }
        Self { name: format!("{n} elemanlı Yagi"), frequency, elements, driven: 1, profile }
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
        for (i, e) in self.elements.iter().enumerate() {
            let first = wires.len();
            let pieces = self.profile.pieces(0.5 * e.length.max(1e-4));
            let x = e.x;
            let (_, c_out, c_tube) = pieces[0];
            if i == self.driven {
                feed = wires.len();
            }
            wires.push(wire(&e.name, [x, -c_out, 0.0], [x, c_out, 0.0], c_tube));
            for (k, &(a, b, t)) in pieces.iter().enumerate().skip(1) {
                wires.push(wire(&format!("{} ·{} sağ", e.name, k + 1), [x, a, 0.0], [x, b, 0.0], t));
                wires.push(wire(&format!("{} ·{} sol", e.name, k + 1), [x, -a, 0.0], [x, -b, 0.0], t));
            }
            map.push(first..wires.len());
        }
        (Antenna { name: self.name.clone(), frequency: self.frequency, wires, sources: vec![source(feed, 0.5)] }, map)
    }

    pub fn build(&self) -> Antenna {
        self.build_with_map().0
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
