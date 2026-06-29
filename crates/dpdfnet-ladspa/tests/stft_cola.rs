use dpdfnet_ladspa::stft::{Analysis, Synthesis, HOP, SPEC_LEN};

// Feeding identity frames (analysis -> synthesis with no spectral change) must
// reconstruct the input, delayed by one hop, because the vorbis window is COLA.
#[test]
fn analysis_then_synthesis_reconstructs_input() {
    let mut ana = Analysis::new();
    let mut syn = Synthesis::new();
    // deterministic pseudo-signal
    let total = HOP * 40;
    let input: Vec<f32> = (0..total)
        .map(|n| (n as f32 * 0.013).sin() * 0.5 + (n as f32 * 0.0007).sin() * 0.3)
        .collect();

    let mut out = vec![0f32; total];
    let mut spec = [0f32; SPEC_LEN];
    let mut hop_out = [0f32; HOP];
    let hops = total / HOP;
    for h in 0..hops {
        ana.push_hop(&input[h * HOP..(h + 1) * HOP], &mut spec);
        syn.add_frame(&spec, &mut hop_out);          // pass-through spectrum
        out[h * HOP..(h + 1) * HOP].copy_from_slice(&hop_out);
    }

    // Compare with a one-hop (480-sample) delay; skip the first 2 hops (warm-up).
    let delay = HOP;
    let mut max_err = 0f32;
    for n in (2 * HOP)..(total - delay) {
        max_err = max_err.max((out[n + delay] - input[n]).abs());
    }
    assert!(max_err < 1e-3, "reconstruction error too large: {max_err}");
}
