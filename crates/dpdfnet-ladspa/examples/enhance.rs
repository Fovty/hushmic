//! Offline file enhancer: run a 48 kHz mono WAV through the DPDFNet engine.
//!
//!   cargo run --release --example enhance -p dpdfnet-ladspa -- in.wav out.wav
//!
//! Reads a 48 kHz mono WAV, streams it through `dpdfnet_ladspa::engine::Engine`
//! hop-by-hop (480-sample hops, mirroring tests/parity.rs), and writes the
//! enhanced result as a 48 kHz mono WAV.
//!
//! Model path:   $HUSHMIC_MODEL_PATH  (else <repo>/assets/models/dpdfnet8_48khz_hr.onnx)
//! ORT runtime:  $ORT_DYLIB_PATH      (else baked-in <repo>/assets/lib/libonnxruntime.so)

use dpdfnet_ladspa::engine::Engine;
use dpdfnet_ladspa::stft::HOP;
use std::path::{Path, PathBuf};

fn read_wav_mono_f32(p: &str) -> (Vec<f32>, u32) {
    let mut r = hound::WavReader::open(p).expect("open input wav");
    let spec = r.spec();
    let s: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|x| x.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|x| x.unwrap() as f32 / max).collect()
        }
    };
    // Downmix to mono by taking the first channel (input is expected to be mono already).
    let mono: Vec<f32> = if spec.channels == 1 {
        s
    } else {
        s.iter().step_by(spec.channels as usize).copied().collect()
    };
    (mono, spec.sample_rate)
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt() as f32
}

fn model_path() -> PathBuf {
    if let Ok(p) = std::env::var("HUSHMIC_MODEL_PATH") {
        return PathBuf::from(p);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    root.join("assets/models/dpdfnet8_48khz_hr.onnx")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: enhance <in.wav> <out.wav>");
        std::process::exit(2);
    }
    let in_path = &args[1];
    let out_path = &args[2];

    let (noisy, sr) = read_wav_mono_f32(in_path);
    if sr != 48_000 {
        eprintln!(
            "[enhance] WARNING: input sample rate is {sr} Hz, expected 48000 Hz; \
             the model assumes 48 kHz and output will be played back at 48 kHz."
        );
    }

    let model = model_path();
    let mut eng = Engine::new(Path::new(&model))
        .unwrap_or_else(|e| panic!("engine init ({}): {e}", model.display()));

    let mut out = Vec::with_capacity(noisy.len());
    let mut hop_in = [0f32; HOP];
    let mut hop_out = [0f32; HOP];
    let hops = noisy.len() / HOP;
    for h in 0..hops {
        hop_in.copy_from_slice(&noisy[h * HOP..(h + 1) * HOP]);
        eng.process_hop(&hop_in, &mut hop_out).expect("process_hop");
        out.extend_from_slice(&hop_out);
    }

    // Write enhanced output as 48 kHz mono 16-bit PCM (universally playable).
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(out_path, spec).expect("create output wav");
    for &s in &out {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        w.write_sample(v).expect("write sample");
    }
    w.finalize().expect("finalize output wav");

    eprintln!(
        "[enhance] {} -> {} : {} hops, in_rms={:.5} out_rms={:.5}",
        in_path,
        out_path,
        hops,
        rms(&noisy),
        rms(&out)
    );
}
