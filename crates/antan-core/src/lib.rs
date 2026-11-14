//! AntAn çekirdeği.
//!
//! Arayüzden bağımsızdır; yalnızca hesap yapar. Birimler her yerde SI'dır
//! (metre, Hz, ohm). Eksenler: +X bum yönü (ileri), Y eleman ekseni, +Z yukarı.
//!
//! Akış: [`Antenna`] → [`solver::solve`] → [`Solution`] → [`farfield`] / [`matching`].

pub mod bands;
pub mod farfield;
pub mod geometry;
pub mod i18n;
pub mod linalg;
pub mod loading;
pub mod matching;
pub mod material;
pub mod mechanics;
pub mod nec;
pub mod optimize;
pub mod scan;
pub mod solver;
pub mod templates;
pub mod yagi;

pub use farfield::{FarField, Pattern, PatternSummary};
pub use geometry::{Antenna, Source, Vec3, Wire};
pub use material::Material;
pub use num_complex::Complex64;
pub use solver::{Solution, SolveError, SweepPoint};

/// Işık hızı (m/s).
pub const C0: f64 = 299_792_458.0;
/// Boşluğun manyetik geçirgenliği (H/m).
pub const MU0: f64 = 4.0e-7 * std::f64::consts::PI;
/// Boşluğun dielektrik sabiti (F/m).
pub const EPS0: f64 = 1.0 / (MU0 * C0 * C0);
/// Boşluğun dalga empedansı (ohm).
pub const ETA0: f64 = MU0 * C0;

/// Dalga boyu (m).
pub fn wavelength(freq_hz: f64) -> f64 {
    C0 / freq_hz
}
