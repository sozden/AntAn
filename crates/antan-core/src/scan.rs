//! Yükseklik taraması: antenin zeminden yüksekliğine göre kazanç, kalkış açısı,
//! ön/arka ve besleme empedansı.

use num_complex::Complex64;

use crate::farfield::Pattern;
use crate::geometry::Antenna;
use crate::solver::solve;

#[derive(Clone, Copy, Debug)]
pub struct HeightRow {
    /// Antenin en alt noktasının yüksekliği (m).
    pub height: f64,
    pub gain_dbi: f64,
    /// Ana lobun yataydan açısı (derece).
    pub takeoff_deg: f64,
    pub front_to_back_db: f64,
    pub impedance: Complex64,
}

/// Anteni en alt noktası `h` yüksekliğinde olacak şekilde düşey kaydırır.
pub fn at_height(base: &Antenna, h: f64) -> Antenna {
    let lo = base.wires.iter().flat_map(|w| [w.start[2], w.end[2]]).fold(f64::INFINITY, f64::min);
    let dz = h - if lo.is_finite() { lo } else { 0.0 };
    let mut a = base.clone();
    for w in &mut a.wires {
        w.start[2] += dz;
        w.end[2] += dz;
    }
    a
}

/// Verilen yüksekliklerde (en alt nokta, m) çözer. Zemin `base.ground`'dan
/// alınır (serbest uzayda tarama anlamsızdır). Çözülemeyen yükseklikler atlanır.
/// Çekirdekler arasında paralel çalışır; `progress` her satırda çağrılır.
pub fn height_scan(base: &Antenna, heights: &[f64], progress: &(dyn Fn() + Sync)) -> Vec<HeightRow> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(heights.len().max(1));
    let mut rows: Vec<HeightRow> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    heights
                        .iter()
                        .skip(t)
                        .step_by(threads)
                        .filter_map(|&h| {
                            let a = at_height(base, h);
                            let sol = solve(&a, a.frequency).ok()?;
                            let p = Pattern::compute(&sol, 3.0).summary(&sol);
                            progress();
                            Some(HeightRow { height: h, gain_dbi: p.max_gain_dbi, takeoff_deg: 90.0 - p.max_theta.to_degrees(), front_to_back_db: p.front_to_back_db, impedance: sol.impedance() })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    rows.sort_by(|a, b| a.height.total_cmp(&b.height));
    rows
}
