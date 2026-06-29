use crate::config::Config;
use crate::pipewire::Source;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, Tray};
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum TrayCmd {
    SetEnabled(bool),
    SelectMic(Option<String>),
    SelectModel(String),
    SetAttn(f32),
    SetDefaultToggle(bool),
    SetAutostart(bool),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayStatus {
    Off,
    Active,
    Error,
}

impl TrayStatus {
    pub fn icon_name(&self) -> &'static str {
        match self {
            TrayStatus::Active => "audio-input-microphone",
            TrayStatus::Off => "microphone-sensitivity-muted",
            TrayStatus::Error => "dialog-error",
        }
    }
    pub fn title_suffix(&self) -> &'static str {
        match self {
            TrayStatus::Error => " (error)",
            _ => "",
        }
    }
}

pub struct HushmicTray {
    pub cfg: Config,
    pub mics: Vec<Source>,
    pub cmd_tx: Sender<TrayCmd>,
    pub status: TrayStatus,
}

const MODELS: &[(&str, &str)] = &[
    ("dpdfnet8_48khz_hr", "High quality (dpdfnet8)"),
    ("dpdfnet2_48khz_hr", "Light / low-CPU (dpdfnet2)"),
];
const ATTN_PRESETS: &[(f32, &str)] = &[
    (100.0, "Maximum"),
    (24.0, "Strong (24 dB)"),
    (12.0, "Medium (12 dB)"),
    (6.0, "Light (6 dB)"),
];

impl Tray for HushmicTray {
    fn id(&self) -> String {
        "hushmic".into()
    }
    fn title(&self) -> String {
        format!("Hushmic{}", self.status.title_suffix())
    }
    fn icon_name(&self) -> String {
        self.status.icon_name().into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        // mic radio: index 0 = "System default", then each real source
        let mut mic_opts = vec![RadioItem {
            label: "System default".into(),
            ..Default::default()
        }];
        mic_opts.extend(self.mics.iter().map(|m| RadioItem {
            label: m.description.clone(),
            ..Default::default()
        }));
        let mic_selected = match &self.cfg.mic {
            None => 0,
            Some(name) => self
                .mics
                .iter()
                .position(|m| &m.name == name)
                .map(|i| i + 1)
                .unwrap_or(0),
        };
        let mics_for_select = self.mics.clone();

        let model_selected = MODELS
            .iter()
            .position(|(id, _)| *id == self.cfg.model)
            .unwrap_or(0);
        let attn_selected = ATTN_PRESETS
            .iter()
            .position(|(v, _)| (*v - self.cfg.attn_limit).abs() < 0.5)
            .unwrap_or(0);

        vec![
            CheckmarkItem {
                label: "Enable noise suppression".into(),
                checked: self.cfg.enabled,
                activate: Box::new(|t: &mut Self| {
                    t.cfg.enabled = !t.cfg.enabled;
                    let _ = t.cmd_tx.send(TrayCmd::SetEnabled(t.cfg.enabled));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Microphone".into(),
                submenu: vec![RadioGroup {
                    selected: mic_selected,
                    select: Box::new(move |t: &mut Self, idx| {
                        let pick = if idx == 0 {
                            None
                        } else {
                            mics_for_select.get(idx - 1).map(|m| m.name.clone())
                        };
                        t.cfg.mic = pick.clone();
                        let _ = t.cmd_tx.send(TrayCmd::SelectMic(pick));
                    }),
                    options: mic_opts,
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "Model".into(),
                submenu: vec![RadioGroup {
                    selected: model_selected,
                    select: Box::new(|t: &mut Self, idx| {
                        let id = MODELS[idx].0.to_string();
                        t.cfg.model = id.clone();
                        let _ = t.cmd_tx.send(TrayCmd::SelectModel(id));
                    }),
                    options: MODELS
                        .iter()
                        .map(|(_, label)| RadioItem {
                            label: (*label).into(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "Suppression strength".into(),
                submenu: vec![RadioGroup {
                    selected: attn_selected,
                    select: Box::new(|t: &mut Self, idx| {
                        let v = ATTN_PRESETS[idx].0;
                        t.cfg.attn_limit = v;
                        let _ = t.cmd_tx.send(TrayCmd::SetAttn(v));
                    }),
                    options: ATTN_PRESETS
                        .iter()
                        .map(|(_, label)| RadioItem {
                            label: (*label).into(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Set as default microphone".into(),
                checked: self.cfg.set_default,
                activate: Box::new(|t: &mut Self| {
                    t.cfg.set_default = !t.cfg.set_default;
                    let _ = t.cmd_tx.send(TrayCmd::SetDefaultToggle(t.cfg.set_default));
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Start on login".into(),
                checked: self.cfg.autostart,
                activate: Box::new(|t: &mut Self| {
                    t.cfg.autostart = !t.cfg.autostart;
                    let _ = t.cmd_tx.send(TrayCmd::SetAutostart(t.cfg.autostart));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.cmd_tx.send(TrayCmd::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_icons_distinct() {
        use super::TrayStatus::*;
        assert_ne!(Off.icon_name(), Active.icon_name());
        assert_ne!(Active.icon_name(), Error.icon_name());
        assert_eq!(Error.title_suffix(), " (error)");
    }

    #[test]
    fn menu_builds_non_empty() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let tray = HushmicTray {
            cfg: Config::default(),
            mics: vec![Source {
                name: "alsa_input.test".into(),
                description: "Test Mic".into(),
            }],
            cmd_tx: tx,
            status: TrayStatus::Off,
        };
        let menu = tray.menu();
        assert!(!menu.is_empty(), "tray menu should not be empty");
    }
}
