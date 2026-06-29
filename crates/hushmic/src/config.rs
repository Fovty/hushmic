use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub mic: Option<String>,      // real source node.name; None = use system default
    pub model: String,            // model file stem under /usr/share/hushmic/models
    pub attn_limit: f32,          // dB cap for the plugin control port
    pub set_default: bool,        // make hushmic the default input on enable
    pub autostart: bool,          // launch on login
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: true, mic: None, model: "dpdfnet8_48khz_hr".into(), attn_limit: 100.0, set_default: true, autostart: false }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        ProjectDirs::from("io", "hushmic", "hushmic")
            .expect("home dir").config_dir().join("config.toml")
    }
    pub fn load() -> Self {
        match fs::read_to_string(Self::path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }
    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() { fs::create_dir_all(d)?; }
        let s = toml::to_string_pretty(self).expect("serialize config");
        let tmp = p.with_extension("toml.tmp");
        fs::write(&tmp, s)?;
        fs::rename(tmp, p)
    }
}
