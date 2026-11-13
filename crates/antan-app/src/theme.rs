//! Renkler, koyu tema ve renk haritası.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(64, 190, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(30, 92, 128);
pub const WARM: Color32 = Color32::from_rgb(245, 166, 35);
pub const GOOD: Color32 = Color32::from_rgb(90, 205, 125);
pub const BAD: Color32 = Color32::from_rgb(235, 90, 75);
pub const PLOT_BG: Color32 = Color32::from_rgb(16, 19, 25);
pub const GRID: Color32 = Color32::from_rgb(52, 60, 74);
pub const GRID_STRONG: Color32 = Color32::from_rgb(82, 94, 112);
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 150, 168);
pub const SERIES2: Color32 = Color32::from_rgb(240, 110, 160);

/// egui'nin gömülü yazı tiplerinde olmayan simgeler (→ ✕ ● ↶ …) için sistemdeki
/// sembol yazı tipini yedek olarak ekler. Bulunamazsa sessizce geçer.
fn install_symbol_fallback(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Apple Symbols.ttf",
    ];
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("symbols".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("symbols".into());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    install_symbol_fallback(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(27, 31, 39);
    v.window_fill = Color32::from_rgb(31, 36, 45);
    v.extreme_bg_color = PLOT_BG;
    v.faint_bg_color = Color32::from_rgb(33, 38, 48);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(45, 52, 64));
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(42, 48, 60);
    v.widgets.inactive.bg_fill = Color32::from_rgb(42, 48, 60);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 62, 78);
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(4);
    }
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        // egui varsayılanları (12.5 pt gövde) küçük ekranlarda zor okunuyor; bir kademe büyük.
        use egui::{FontFamily::*, FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Small, FontId::new(11.0, Proportional)),
            (TextStyle::Body, FontId::new(14.5, Proportional)),
            (TextStyle::Button, FontId::new(14.5, Proportional)),
            (TextStyle::Heading, FontId::new(20.0, Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, Monospace)),
        ]
        .into();
    });
}

/// Grafiklerin üzerine çizilen yazılar için boyut (pt). Tek yerden büyütülebilsin diye.
pub fn plot_font(size: f32) -> egui::FontId {
    egui::FontId::proportional(size + 1.5)
}

/// Yazı boyutu (arayüz ölçeği) seçenekleri.
pub const TEXT_SCALES: [f32; 7] = [0.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0];

/// 0..1 → "turbo" benzeri renk haritası (mavi → camgöbeği → yeşil → sarı → kırmızı).
pub fn colormap(t: f32) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 6] = [
        (0.0, [38, 34, 98]),
        (0.2, [30, 110, 220]),
        (0.4, [26, 200, 200]),
        (0.6, [110, 230, 90]),
        (0.8, [250, 200, 40]),
        (1.0, [235, 60, 40]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in STOPS.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let f = (t - t0) / (t1 - t0);
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
            return Color32::from_rgb(mix(c0[0], c1[0]), mix(c0[1], c1[1]), mix(c0[2], c1[2]));
        }
    }
    let c = STOPS[5].1;
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Rengi `k` (0..) ile aydınlat/karart.
pub fn shade(c: Color32, k: f32) -> Color32 {
    let f = |x: u8| ((x as f32) * k).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

pub fn swr_color(swr: f64) -> Color32 {
    if swr < 1.5 {
        GOOD
    } else if swr < 2.0 {
        WARM
    } else {
        BAD
    }
}
