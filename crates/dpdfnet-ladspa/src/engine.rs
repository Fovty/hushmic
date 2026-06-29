use crate::attn::AttnLimiter;
use crate::model::Model;
use crate::stft::{Analysis, Synthesis, HOP, SPEC_LEN};
use std::path::Path;

pub struct Engine {
    analysis: Analysis,
    synthesis: Synthesis,
    model: Model,
    state: Vec<f32>,
    spec: [f32; SPEC_LEN],
    spec_e: [f32; SPEC_LEN],
    state_out: Vec<f32>,
    attn: AttnLimiter,
}

impl Engine {
    pub fn new(model_path: &Path) -> Result<Engine, String> {
        let model = Model::load(model_path)?;
        let state = model.init_state.clone();
        let state_out = vec![0f32; model.state_size];
        Ok(Engine {
            analysis: Analysis::new(),
            synthesis: Synthesis::new(),
            model,
            state,
            spec: [0f32; SPEC_LEN],
            spec_e: [0f32; SPEC_LEN],
            state_out,
            attn: AttnLimiter::new(),
        })
    }

    pub fn reset(&mut self) {
        self.analysis.reset();
        self.synthesis.reset();
        self.state.clear();
        self.state.extend_from_slice(&self.model.init_state);
        self.attn.reset();
    }

    pub fn set_attn_db(&mut self, db: f32) {
        self.attn.set_db(db);
    }

    pub fn process_hop(&mut self, in_hop: &[f32; HOP], out_hop: &mut [f32; HOP]) -> Result<(), String> {
        self.analysis.push_hop(in_hop, &mut self.spec);
        self.model.run(&self.spec, &self.state, &mut self.spec_e, &mut self.state_out)?;
        std::mem::swap(&mut self.state, &mut self.state_out);
        self.attn.apply(&self.spec, &mut self.spec_e); // blend noisy floor per dB cap
        self.synthesis.add_frame(&self.spec_e, out_hop);
        Ok(())
    }
}
