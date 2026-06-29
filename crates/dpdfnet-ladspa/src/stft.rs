use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const N_FFT: usize = 960;
pub const HOP: usize = 480;
pub const FREQ_BINS: usize = 481; // N_FFT/2 + 1
pub const SPEC_LEN: usize = FREQ_BINS * 2; // 962, interleaved re/im

/// Vorbis (sin-of-sin^2) window, COLA at 50% overlap.
pub fn vorbis_window() -> [f32; N_FFT] {
    let h = (N_FFT as f32) / 2.0;
    let mut w = [0f32; N_FFT];
    for (n, wn) in w.iter_mut().enumerate() {
        let s = (0.5 * std::f32::consts::PI * (n as f32 + 0.5) / h).sin();
        *wn = (0.5 * std::f32::consts::PI * s * s).sin();
    }
    w
}

/// Causal analysis STFT (center = false). Keeps a 960-sample ring; each hop shifts
/// in 480 new samples, windows the full 960, and emits one interleaved re/im frame.
pub struct Analysis {
    window: [f32; N_FFT],
    ring: [f32; N_FFT],
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}

impl Analysis {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            window: vorbis_window(),
            ring: [0f32; N_FFT],
            fft: planner.plan_fft_forward(N_FFT),
            scratch: vec![Complex32::new(0.0, 0.0); N_FFT],
        }
    }

    pub fn reset(&mut self) {
        self.ring = [0f32; N_FFT];
    }

    pub fn push_hop(&mut self, in_hop: &[f32], out_spec: &mut [f32; SPEC_LEN]) {
        debug_assert_eq!(in_hop.len(), HOP);
        // shift left by HOP, append new HOP samples
        self.ring.copy_within(HOP.., 0);
        self.ring[N_FFT - HOP..].copy_from_slice(in_hop);
        // windowed full-resolution FFT
        for i in 0..N_FFT {
            self.scratch[i] = Complex32::new(self.ring[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);
        // take the first FREQ_BINS bins, interleave re/im
        for k in 0..FREQ_BINS {
            out_spec[2 * k] = self.scratch[k].re;
            out_spec[2 * k + 1] = self.scratch[k].im;
        }
    }
}

/// ISTFT + overlap-add. Mirrors the analysis window; emits one hop per frame.
pub struct Synthesis {
    window: [f32; N_FFT],
    ola: [f32; N_FFT],
    ifft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}

impl Synthesis {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            window: vorbis_window(),
            ola: [0f32; N_FFT],
            ifft: planner.plan_fft_inverse(N_FFT),
            scratch: vec![Complex32::new(0.0, 0.0); N_FFT],
        }
    }

    pub fn reset(&mut self) {
        self.ola = [0f32; N_FFT];
    }

    pub fn add_frame(&mut self, spec: &[f32; SPEC_LEN], out_hop: &mut [f32; HOP]) {
        // rebuild full hermitian spectrum from FREQ_BINS interleaved bins
        for k in 0..FREQ_BINS {
            self.scratch[k] = Complex32::new(spec[2 * k], spec[2 * k + 1]);
        }
        // Hermitian mirror: for a length-N_FFT real signal, X[j] = conj(X[N_FFT-j]).
        // DC (bin 0) and Nyquist (bin N_FFT/2 = 480) are real and already set above;
        // fill bins 481..=959 from their conjugate partners 479..=1.
        for j in FREQ_BINS..N_FFT {
            self.scratch[j] = self.scratch[N_FFT - j].conj();
        }
        self.ifft.process(&mut self.scratch);
        // rustfft inverse is unnormalized: divide by N_FFT; window; overlap-add
        let norm = 1.0 / (N_FFT as f32);
        self.ola.copy_within(HOP.., 0);
        for s in &mut self.ola[N_FFT - HOP..] {
            *s = 0.0;
        }
        for i in 0..N_FFT {
            self.ola[i] += self.scratch[i].re * norm * self.window[i];
        }
        out_hop.copy_from_slice(&self.ola[..HOP]);
    }
}
