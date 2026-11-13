//! NEC-2 kart dosyası dışa aktarımı (4nec2, EZNEC içe aktarma, xnec2c ile açılabilir).
//!
//! Yaklaşımlar: NEC kaynakları parça ortasına konur; AntAn ise düğüme (parça
//! sınırına) koyar. Bu yüzden beslenen tellerin parça sayısı tek yapılır ve
//! kaynak ortadaki parçaya yerleştirilir. Tel ucundaki (birleşim) beslemeler
//! o telin uçtaki ilk/son parçasına konur. Malzeme kaybı LD 5 kartıyla yazılır
//! (NEC'te boru et kalınlığı yoktur; dolu iletken kabul edilir).

use std::fmt::Write;

use crate::C0;
use crate::geometry::Antenna;

pub fn export(antenna: &Antenna, sweep: Option<(f64, f64, usize)>) -> String {
    let lambda = C0 / antenna.frequency;
    let mut out = String::new();
    let _ = writeln!(out, "CM {}", antenna.name);
    let _ = writeln!(out, "CM AntAn tarafından oluşturuldu. Birimler: metre.");
    let _ = writeln!(out, "CE");
    let mut segs = Vec::new();
    for (i, w) in antenna.wires.iter().enumerate() {
        let mut m = w.segment_count(lambda);
        let fed_mid = antenna.sources.iter().any(|s| s.wire == i && s.position > 0.0 && s.position < 1.0);
        if fed_mid {
            m += 1;
        }
        segs.push(m);
        let _ = writeln!(
            out,
            "GW {} {} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6}",
            i + 1,
            m,
            w.start[0],
            w.start[1],
            w.start[2],
            w.end[0],
            w.end[1],
            w.end[2],
            w.radius()
        );
    }
    let _ = writeln!(out, "GE 0");
    for (i, w) in antenna.wires.iter().enumerate() {
        if w.material.conductivity.is_finite() {
            let _ = writeln!(out, "LD 5 {} 0 0 {:.4e}", i + 1, w.material.conductivity);
        }
    }
    for s in &antenna.sources {
        let m = segs[s.wire];
        let seg = ((s.position * m as f64).ceil() as usize).clamp(1, m);
        let _ = writeln!(out, "EX 0 {} {} 0 {:.4} {:.4}", s.wire + 1, seg, s.voltage.re, s.voltage.im);
    }
    match sweep {
        Some((f0, f1, n)) if n > 1 => {
            let _ = writeln!(out, "FR 0 {} 0 0 {:.6} {:.6}", n, f0 / 1e6, (f1 - f0) / (n - 1) as f64 / 1e6);
        }
        _ => {
            let _ = writeln!(out, "FR 0 1 0 0 {:.6} 0", antenna.frequency / 1e6);
        }
    }
    let _ = writeln!(out, "RP 0 37 73 1000 0 0 5 5");
    let _ = writeln!(out, "EN");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{self, Tube};

    #[test]
    fn exports_cards() {
        let a = templates::yagi(144.3e6, 3, &Tube::aluminum(0.01, None));
        let s = export(&a, None);
        assert_eq!(s.lines().filter(|l| l.starts_with("GW")).count(), 3);
        assert!(s.contains("EX 0 2 "));
        assert!(s.trim_end().ends_with("EN"));
    }
}
