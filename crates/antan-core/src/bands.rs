//! Amatör bantları (IARU Bölge 1): ad, alt/üst sınır, varsayılan merkez (Hz).

pub struct Band {
    pub name: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub center: f64,
}

const fn b(name: &'static str, lo: f64, hi: f64, center: f64) -> Band {
    Band { name, lo: lo * 1e6, hi: hi * 1e6, center: center * 1e6 }
}

pub const BANDS: [Band; 16] = [
    b("160 m", 1.810, 2.000, 1.850),
    b("80 m", 3.500, 3.800, 3.650),
    b("60 m", 5.3515, 5.3665, 5.360),
    b("40 m", 7.000, 7.200, 7.100),
    b("30 m", 10.100, 10.150, 10.125),
    b("20 m", 14.000, 14.350, 14.175),
    b("17 m", 18.068, 18.168, 18.118),
    b("15 m", 21.000, 21.450, 21.225),
    b("12 m", 24.890, 24.990, 24.940),
    b("10 m", 28.000, 29.700, 28.500),
    b("6 m", 50.000, 52.000, 50.150),
    b("4 m", 70.000, 70.500, 70.200),
    b("2 m", 144.000, 146.000, 144.300),
    b("70 cm", 430.000, 440.000, 432.200),
    b("23 cm", 1240.000, 1300.000, 1296.200),
    b("13 cm", 2300.000, 2450.000, 2320.200),
];

/// Frekansın içinde olduğu bant (yoksa `None`).
pub fn band_of(f: f64) -> Option<&'static Band> {
    BANDS.iter().find(|b| f >= b.lo - 1.0 && f <= b.hi + 1.0)
}

/// Frekansa en yakın bant (oransal uzaklıkla).
pub fn nearest(f: f64) -> &'static Band {
    BANDS.iter().min_by(|a, b| (f / a.center).ln().abs().total_cmp(&(f / b.center).ln().abs())).unwrap()
}

/// Programın kabul ettiği frekans aralığı (Hz).
pub const MIN_FREQ: f64 = 0.1e6;
pub const MAX_FREQ: f64 = 3000.0e6;
