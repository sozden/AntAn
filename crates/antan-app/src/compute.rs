//! Arka plan hesapları: frekans taraması (tüm çekirdeklerde paralel).

use std::sync::mpsc::{Receiver, channel};

use eframe::egui;

use antan_core::solver::{SolveError, SweepPoint, solve};
use antan_core::Antenna;

pub fn parallel_sweep(ant: &Antenna, f0: f64, f1: f64, n: usize) -> Result<Vec<SweepPoint>, SolveError> {
    let n = n.max(2);
    let freqs: Vec<f64> = (0..n).map(|i| f0 + (f1 - f0) * i as f64 / (n - 1) as f64).collect();
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4).min(n);
    let chunk = n.div_ceil(threads);
    let results: Vec<Result<Vec<SweepPoint>, SolveError>> = std::thread::scope(|s| {
        let handles: Vec<_> = freqs
            .chunks(chunk)
            .map(|fs| s.spawn(move || fs.iter().map(|&f| solve(ant, f).map(|sol| SweepPoint { frequency: f, impedance: sol.impedance() })).collect::<Result<Vec<_>, _>>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(Err(SolveError::Singular))).collect()
    });
    let mut out = Vec::with_capacity(n);
    for r in results {
        out.extend(r?);
    }
    Ok(out)
}

/// Tek bir bekleyen tarama işi; yenisi başlayınca eskisinin sonucu yok sayılır.
#[derive(Default)]
pub struct SweepJob {
    generation: u64,
    rx: Option<Receiver<(u64, Result<Vec<SweepPoint>, SolveError>)>>,
}

impl SweepJob {
    pub fn start(&mut self, ctx: &egui::Context, ant: Antenna, f0: f64, f1: f64, n: usize) {
        self.generation += 1;
        let generation = self.generation;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = parallel_sweep(&ant, f0, f1, n);
            let _ = tx.send((generation, r));
            ctx.request_repaint();
        });
    }
    pub fn pending(&self) -> bool {
        self.rx.is_some()
    }
    pub fn poll(&mut self) -> Option<Result<Vec<SweepPoint>, SolveError>> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok((g, r)) if g == self.generation => {
                self.rx = None;
                Some(r)
            }
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(_) => {
                self.rx = None;
                None
            }
        }
    }
}
