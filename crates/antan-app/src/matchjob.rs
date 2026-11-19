//! "Z₀'a eşle": sürülen eleman ile ilk yönlendiricinin boy ve konumunu arka
//! planda (Nelder-Mead) ayarlar; arayüz donmaz.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;

use antan_core::optimize::{self, Goals, Score};
use antan_core::yagi::YagiDesign;

/// Bitmiş iş: yeni tasarım, önceki ve sonraki skor.
pub type MatchResult = (YagiDesign, Score, Score);

#[derive(Default)]
pub struct MatchJob {
    result: Arc<Mutex<Option<MatchResult>>>,
    iter: Arc<AtomicUsize>,
    running: bool,
}

impl MatchJob {
    pub fn busy(&self) -> bool {
        self.running
    }
    pub fn iteration(&self) -> usize {
        self.iter.load(Ordering::Relaxed)
    }
    pub fn start(&mut self, ctx: &egui::Context, d: YagiDesign, z0: f64) {
        if self.running {
            return;
        }
        self.running = true;
        self.iter.store(0, Ordering::Relaxed);
        let (result, iter, ctx) = (self.result.clone(), self.iter.clone(), ctx.clone());
        std::thread::spawn(move || {
            let goals = Goals { z0, ..Goals::default() };
            let before = optimize::evaluate(&d, &goals).unwrap_or_default();
            let ctx2 = ctx.clone();
            let (m, _) = optimize::match_feed(&d, z0, 250, |i, _, _| {
                iter.store(i, Ordering::Relaxed);
                ctx2.request_repaint();
                true
            });
            // Karşılaştırma aynı (varsayılan) hedeflerle.
            let after = optimize::evaluate(&m, &goals).unwrap_or_default();
            *result.lock().unwrap() = Some((m, before, after));
            ctx.request_repaint();
        });
    }
    /// İş bittiyse sonucu bir kez verir.
    pub fn poll(&mut self) -> Option<MatchResult> {
        let r = self.result.lock().unwrap().take();
        if r.is_some() {
            self.running = false;
        }
        r
    }
}
