use hushmic::config::Config;

#[test]
fn defaults_are_sane() {
    let c = Config::default();
    assert_eq!(c.model, "dpdfnet8_48khz_hr");
    assert_eq!(c.attn_limit, 100.0);
    assert!(c.enabled);
}

#[test]
fn toml_roundtrips() {
    let c = Config { enabled: false, mic: Some("alsa_input.x".into()), model: "dpdfnet2_48khz_hr".into(), attn_limit: 24.0, set_default: true, autostart: true };
    let s = toml::to_string_pretty(&c).unwrap();
    let back: Config = toml::from_str(&s).unwrap();
    assert_eq!(back.mic.as_deref(), Some("alsa_input.x"));
    assert_eq!(back.attn_limit, 24.0);
    assert!(back.set_default && back.autostart && !back.enabled);
}
