//! hushmic DPDFNet LADSPA plugin (v0.1).
pub mod stft;
pub mod model;
pub mod attn;
pub mod engine;

use engine::Engine;
use ladspa::{DefaultValue, Plugin, PluginDescriptor, Port, PortConnection, PortDescriptor};
use stft::HOP;
use std::path::PathBuf;

const LABEL: &str = "dpdfnet_mono";
const UNIQUE_ID: u64 = 0x68736D31; // "hsm1"
const DEFAULT_MODEL: &str = env!("HUSHMIC_DEFAULT_MODEL");

fn model_path() -> PathBuf {
    std::env::var("HUSHMIC_MODEL_PATH").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(DEFAULT_MODEL))
}

struct DpdfnetPlugin {
    engine: Option<Engine>,
    in_buf: Vec<f32>,
    out_buf: Vec<f32>,   // committed enhanced samples waiting to be emitted
    last_db: f32,
}

impl DpdfnetPlugin {
    fn new() -> Self {
        let engine = match Engine::new(&model_path()) {
            Ok(e) => Some(e),
            Err(e) => { eprintln!("[dpdfnet-ladspa] engine init failed: {e}"); None }
        };
        DpdfnetPlugin {
            engine,
            in_buf: Vec::with_capacity(HOP * 4),
            out_buf: Vec::with_capacity(HOP * 4),
            last_db: f32::NAN,
        }
    }
}

impl Plugin for DpdfnetPlugin {
    fn activate(&mut self) {
        // Reset recurrent state + buffers so no stale state bleeds across sessions.
        if let Some(e) = self.engine.as_mut() { e.reset(); }
        self.in_buf.clear();
        self.out_buf.clear();
        // pre-fill one hop of silence => one-hop output latency, absorbs the first frame.
        self.out_buf.resize(HOP, 0.0);
        self.last_db = f32::NAN;
    }

    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        let input = ports[0].unwrap_audio();
        let mut output = ports[1].unwrap_audio_mut();
        let db = *ports[2].unwrap_control();

        let engine = match self.engine.as_mut() {
            Some(e) => e,
            None => { for o in output.iter_mut() { *o = 0.0; } return; } // passthrough-silence on failure
        };
        if db != self.last_db {
            engine.set_attn_db(db);
            self.last_db = db;
        }

        // 1. enqueue input
        self.in_buf.extend_from_slice(&input[..sample_count]);
        // 2. drain whole hops through the engine
        let mut hop_in = [0f32; HOP];
        let mut hop_out = [0f32; HOP];
        while self.in_buf.len() >= HOP {
            hop_in.copy_from_slice(&self.in_buf[..HOP]);
            if engine.process_hop(&hop_in, &mut hop_out).is_ok() {
                self.out_buf.extend_from_slice(&hop_out);
            } else {
                self.out_buf.extend_from_slice(&[0f32; HOP]);
            }
            self.in_buf.drain(..HOP);
        }
        // 3. emit sample_count from the output queue (zero-fill if underfilled)
        let avail = self.out_buf.len().min(sample_count);
        output[..avail].copy_from_slice(&self.out_buf[..avail]);
        for o in output[avail..sample_count].iter_mut() { *o = 0.0; }
        self.out_buf.drain(..avail);
    }
}

fn new_instance(_d: &PluginDescriptor, _sample_rate: u64) -> Box<dyn Plugin + Send> {
    Box::new(DpdfnetPlugin::new())
}

#[no_mangle]
pub fn get_ladspa_descriptor(index: u64) -> Option<PluginDescriptor> {
    if index != 0 {
        return None;
    }
    Some(PluginDescriptor {
        unique_id: UNIQUE_ID,
        label: LABEL,
        properties: ladspa::PROP_NONE,
        name: "hushmic DPDFNet Noise Suppressor (Mono)",
        maker: "hushmic",
        copyright: "MIT OR Apache-2.0",
        ports: vec![
            Port { name: "Input", desc: PortDescriptor::AudioInput, hint: None, default: None, lower_bound: None, upper_bound: None },
            Port { name: "Output", desc: PortDescriptor::AudioOutput, hint: None, default: None, lower_bound: None, upper_bound: None },
            Port {
                name: "Attenuation Limit (dB)",
                desc: PortDescriptor::ControlInput,
                hint: None,
                default: Some(DefaultValue::Maximum),
                lower_bound: Some(0.0),
                upper_bound: Some(100.0),
            },
        ],
        new: new_instance,
    })
}
