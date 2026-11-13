//! Arayüz dili (English / Türkçe). Uygulama genelinde tek bir ayar vardır;
//! çekirdekteki etiketler, şablon adları ve hata mesajları da buna uyar.
//!
//! Kullanım: düz metin için `t("English", "Türkçe")`, biçimli metin için
//! `tf!("Gain {:.2}", "Kazanç {:.2}", g)`.

use std::sync::atomic::{AtomicBool, Ordering};

static TURKISH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    English,
    Turkish,
}

pub fn set_language(l: Language) {
    TURKISH.store(l == Language::Turkish, Ordering::Relaxed);
}

pub fn language() -> Language {
    if is_tr() { Language::Turkish } else { Language::English }
}

pub fn is_tr() -> bool {
    TURKISH.load(Ordering::Relaxed)
}

/// Seçili dildeki metin.
pub fn t(en: &'static str, tr: &'static str) -> &'static str {
    if is_tr() { tr } else { en }
}

/// Seçili dilde `format!`.
#[macro_export]
macro_rules! tf {
    ($en:literal, $tr:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_tr() { format!($tr $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}
