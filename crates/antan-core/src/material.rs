//! İletken malzemeler ve boru/çubuk iç empedansı.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::MU0;

/// JSON sonsuz sayı taşıyamaz: sonsuz değer `null` olarak yazılıp okunur.
mod inf_as_null {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() { s.serialize_some(v) } else { s.serialize_none() }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::INFINITY))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    /// İletkenlik (S/m). `f64::INFINITY` kayıpsız iletken demektir.
    #[serde(with = "inf_as_null")]
    pub conductivity: f64,
    /// Yoğunluk (kg/m³).
    pub density: f64,
    /// Elastisite (Young) modülü (Pa).
    pub youngs_modulus: f64,
}

impl Material {
    pub fn new(name: &str, conductivity: f64, density: f64, youngs_modulus: f64) -> Self {
        Self { name: name.into(), conductivity, density, youngs_modulus }
    }
    pub fn aluminum_6063() -> Self {
        Self::new("Alüminyum 6063-T6", 3.0e7, 2700.0, 69.0e9)
    }
    pub fn aluminum_6061() -> Self {
        Self::new("Alüminyum 6061-T6", 2.5e7, 2700.0, 69.0e9)
    }
    pub fn copper() -> Self {
        Self::new("Bakır", 5.8e7, 8960.0, 117.0e9)
    }
    pub fn brass() -> Self {
        Self::new("Pirinç", 1.5e7, 8500.0, 100.0e9)
    }
    pub fn stainless_304() -> Self {
        Self::new("Paslanmaz çelik 304", 1.4e6, 8000.0, 193.0e9)
    }
    pub fn perfect() -> Self {
        Self::new("Kayıpsız iletken", f64::INFINITY, 2700.0, 69.0e9)
    }
    /// Arayüzde seçilebilen hazır malzemeler.
    pub fn presets() -> Vec<Material> {
        vec![
            Self::aluminum_6063(),
            Self::aluminum_6061(),
            Self::copper(),
            Self::brass(),
            Self::stainless_304(),
            Self::perfect(),
        ]
    }

    /// Deri kalınlığı (m).
    pub fn skin_depth(&self, freq_hz: f64) -> f64 {
        if !self.conductivity.is_finite() {
            return 0.0;
        }
        (1.0 / (std::f64::consts::PI * freq_hz * MU0 * self.conductivity)).sqrt()
    }

    /// Birim uzunluk başına iç empedans (ohm/m).
    ///
    /// `radius` dış yarıçap, `wall` et kalınlığıdır (`None` = içi dolu çubuk).
    /// Düzlemsel katman yaklaşımı kullanılır: Zs = (1+j)/(σδ)·coth((1+j)t/δ).
    /// Etkin kalınlık t = A/(2πa) alınır (A iletken kesit alanı); böylece hem
    /// DC sınırı (1/σA) hem de yüksek frekans sınırı (deri etkisi) doğru çıkar.
    /// Aradaki bölgede dolu çubuk için Bessel çözümüne göre küçük sapma vardır.
    pub fn internal_impedance(&self, freq_hz: f64, radius: f64, wall: Option<f64>) -> Complex64 {
        if !self.conductivity.is_finite() {
            return Complex64::new(0.0, 0.0);
        }
        let t = wall.map(|w| w.min(radius)).unwrap_or(radius);
        let t_eff = t * (2.0 * radius - t) / (2.0 * radius);
        let delta = self.skin_depth(freq_hz);
        let g = Complex64::new(1.0, 1.0) / delta;
        let x = g * t_eff;
        // coth(x) = (1 + e^{-2x}) / (1 - e^{-2x}); büyük x için kararlı.
        let e = (-2.0 * x).exp();
        let coth = (Complex64::new(1.0, 0.0) + e) / (Complex64::new(1.0, 0.0) - e);
        let zs = g / self.conductivity * coth;
        zs / (2.0 * std::f64::consts::PI * radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_limit_matches_resistance() {
        let m = Material::copper();
        let (a, t) = (0.005, 0.001);
        let z = m.internal_impedance(1.0e-3, a, Some(t));
        let area = std::f64::consts::PI * (a * a - (a - t) * (a - t));
        let r = 1.0 / (m.conductivity * area);
        assert!((z.re - r).abs() / r < 1e-3, "{} vs {}", z.re, r);
    }

    #[test]
    fn hf_limit_is_skin_effect() {
        let m = Material::aluminum_6063();
        let a = 0.005;
        let f = 144.0e6;
        let z = m.internal_impedance(f, a, Some(0.001));
        let rs = 1.0 / (m.conductivity * m.skin_depth(f));
        let expect = rs / (2.0 * std::f64::consts::PI * a);
        assert!((z.re - expect).abs() / expect < 1e-6);
        assert!((z.im - expect).abs() / expect < 1e-6);
    }
}
