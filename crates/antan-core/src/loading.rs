//! Yükleme yardımcıları: bobini rezonansa ayarlama ve yüklü anten şablonları.

use num_complex::Complex64;

use crate::C0;
use crate::geometry::{Antenna, Ground, Load, LoadKind};
use crate::solver::solve;
use crate::templates::{Tube, source, wire};

/// `load` sıradaki yükün endüktansını, besleme reaktansı `target_x` olana kadar
/// ayarlar (sekant, gerçek çözücüyle). Seri ve paralel RLC'de çalışır.
/// Başarılıysa yeni endüktans (H) ve son empedans döner.
pub fn tune_load_inductance(antenna: &Antenna, load: usize, target_x: f64) -> Option<(f64, Complex64)> {
    let f = antenna.frequency;
    let l_start = inductance(&antenna.loads.get(load)?.kind)?;
    tune_inductance(l_start, target_x, |l| {
        let mut a = antenna.clone();
        set_inductance(&mut a.loads[load].kind, l);
        solve(&a, f).ok().map(|s| s.impedance())
    })
}

/// Birkaç yükün (ör. simetrik bobin çifti) endüktansını birlikte, aynı değerde ayarlar.
pub fn tune_loads_inductance(antenna: &Antenna, loads: &[usize], target_x: f64) -> Option<(f64, Complex64)> {
    let f = antenna.frequency;
    let l_start = inductance(&antenna.loads.get(*loads.first()?)?.kind)?;
    tune_inductance(l_start, target_x, |l| {
        let mut a = antenna.clone();
        for &i in loads {
            set_inductance(&mut a.loads.get_mut(i)?.kind, l);
        }
        solve(&a, f).ok().map(|s| s.impedance())
    })
}

/// Aynı endüktanslı, aynı teldeki ve tel ortasına göre simetrik yükü bulur.
pub fn symmetric_partner(antenna: &Antenna, load: usize) -> Option<usize> {
    let ld = antenna.loads.get(load)?;
    antenna.loads.iter().enumerate().position(|(j, o)| {
        j != load && o.wire == ld.wire && (o.position - (1.0 - ld.position)).abs() < 1e-6 && inductance(&o.kind).is_some() && inductance(&o.kind) == inductance(&ld.kind)
    })
}

/// Seri/paralel RLC yükünün endüktansı (diğer türlerde `None`).
pub fn inductance(k: &LoadKind) -> Option<f64> {
    match *k {
        LoadKind::SeriesRlc { l, .. } | LoadKind::ParallelRlc { l, .. } => Some(l),
        _ => None,
    }
}

pub fn set_inductance(k: &mut LoadKind, value: f64) {
    if let LoadKind::SeriesRlc { l, .. } | LoadKind::ParallelRlc { l, .. } = k {
        *l = value;
    }
}

/// Genel endüktans ayarı: `eval(L)` besleme empedansını verir; X = `target_x`
/// olana kadar sekant yöntemiyle yinelenir.
pub fn tune_inductance(l_start: f64, target_x: f64, eval: impl Fn(f64) -> Option<Complex64>) -> Option<(f64, Complex64)> {
    let with = &eval;
    let mut l0 = if l_start > 0.0 { l_start } else { 1e-6 };
    let mut f0 = with(l0)?.im - target_x;
    let mut l1 = l0 * 1.1;
    for _ in 0..40 {
        let z1 = with(l1)?;
        let f1 = z1.im - target_x;
        if f1.abs() < 0.05 {
            return Some((l1, z1));
        }
        let slope = (f1 - f0) / (l1 - l0);
        if slope.abs() < 1e-30 {
            return None;
        }
        let mut next = l1 - f1 / slope;
        // Endüktans pozitif kalsın; adımı sınırla.
        next = next.clamp(0.2 * l1, 5.0 * l1);
        (l0, f0) = (l1, f1);
        l1 = next;
    }
    None
}

/// W3DZZ tarzı 40/80 m tuzaklı dipol: iç kollar 40 m'de rezonans, tuzaklar
/// 7.1 MHz'de (8.2 µH), dış parçalar 80 m'ye uzatır. Tek tel, ince parçalama.
pub fn trap_dipole(tube: &Tube) -> Antenna {
    let (inner, outer) = (10.07, 6.7);
    let half = inner + outer;
    let mut w = wire(crate::i18n::t("Trap dipole", "Tuzaklı dipol"), [0.0, -half, 0.0], [0.0, half, 0.0], tube);
    w.segments = 168;
    let trap = LoadKind::Trap { l: 8.2e-6, resonance: 7.1e6, q: 150.0 };
    let t = |y: f64| (y + half) / (2.0 * half);
    let mut a = Antenna::new(crate::i18n::t("Trap dipole 40/80 m", "Tuzaklı dipol 40/80 m"), 3.7e6, vec![w], vec![source(0, 0.5)]);
    a.loads = vec![Load { wire: 0, position: t(-inner), kind: trap }, Load { wire: 0, position: t(inner), kind: trap }];
    a.ground = Ground::Real { permittivity: 13.0, conductivity: 0.005 };
    for w in &mut a.wires {
        w.start[2] = 15.0;
        w.end[2] = 15.0;
    }
    a
}

/// Bobinle kısaltılmış dipol: toplam boy `length_wl` (λ), her kolun ortasında
/// birer bobin (Q = 200), bobinler rezonansa ayarlanır. Serbest uzayda.
pub fn loaded_dipole(frequency: f64, length_wl: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let half = 0.5 * length_wl * lambda;
    let mut w = wire(crate::i18n::t("Shortened dipole", "Kısaltılmış dipol"), [0.0, -half, 0.0], [0.0, half, 0.0], tube);
    w.segments = 40;
    let mut a = Antenna::new(crate::tf!("Coil-loaded dipole ({:.0} % length)", "Bobin yüklü dipol (boy %{:.0})", length_wl / 0.48 * 100.0), frequency, vec![w], vec![source(0, 0.5)]);
    let coil = LoadKind::SeriesRlc { r: 0.0, l: 2.0 * lambda / C0 * 50.0, c: 0.0 };
    a.loads = vec![Load { wire: 0, position: 0.25, kind: coil }, Load { wire: 0, position: 0.75, kind: coil }];
    if let Some((l, _)) = tune_loads_inductance(&a, &[0, 1], 0.0) {
        let r = 2.0 * std::f64::consts::PI * frequency * l / 200.0;
        for ld in &mut a.loads {
            ld.kind = LoadKind::SeriesRlc { r, l, c: 0.0 };
        }
    }
    a
}

/// Kısa (λ/8) dikey, ortadan bobin yüklü, kusursuz zemin üzerinde tabandan besleme
/// (ör. araç anteni). Bobin rezonansa ayarlanır.
pub fn loaded_vertical(frequency: f64, tube: &Tube) -> Antenna {
    let lambda = C0 / frequency;
    let h = 0.125 * lambda;
    let mut a = Antenna::new(crate::i18n::t("Coil-loaded short vertical", "Bobin yüklü kısa dikey"), frequency, vec![wire(crate::i18n::t("Vertical", "Dikey"), [0.0; 3], [0.0, 0.0, h], tube)], vec![source(0, 0.0)]);
    a.ground = Ground::Perfect;
    a.loads = vec![Load { wire: 0, position: 0.5, kind: LoadKind::SeriesRlc { r: 0.0, l: 5e-6, c: 0.0 } }];
    if let Some((l, _)) = tune_load_inductance(&a, 0, 0.0) {
        // Gerçekçi bobin kaybı: Q = 200.
        let r = 2.0 * std::f64::consts::PI * frequency * l / 200.0;
        a.loads[0].kind = LoadKind::SeriesRlc { r, l, c: 0.0 };
    }
    a
}
