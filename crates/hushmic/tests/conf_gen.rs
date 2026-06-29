use hushmic::config::Config;
use hushmic::controller::{render_conf, Paths};
use std::path::PathBuf;

#[test]
fn conf_contains_required_fields() {
    let cfg = Config {
        mic: Some("alsa_input.realmic".into()),
        attn_limit: 24.0,
        ..Config::default()
    };
    let paths = Paths {
        plugin_so: PathBuf::from("/usr/lib/ladspa/libdpdfnet_ladspa.so"),
        model_dir: PathBuf::from("/usr/share/hushmic/models"),
        dylib: PathBuf::from("/usr/lib/hushmic/libonnxruntime.so"),
    };
    let c = render_conf(&cfg, &paths);
    assert!(c.contains("label  = \"dpdfnet_mono\""), "label missing");
    assert!(
        c.contains("/usr/lib/ladspa/libdpdfnet_ladspa.so"),
        "plugin path missing"
    );
    assert!(
        c.contains("\"Attenuation Limit (dB)\" = 24"),
        "attn control missing"
    );
    assert!(
        c.contains("target.object  = \"alsa_input.realmic\""),
        "mic pin missing"
    );
    assert!(
        c.contains("media.class      = Audio/Source"),
        "not exposed as a source"
    );
    assert!(c.contains("audio.rate     = 48000"));
    assert!(c.contains("node.name        = \"hushmic_source\""));

    // CRITICAL (v0.1 finding): `pipewire -c <conf>` needs the core base modules,
    // otherwise it fails with "can't find protocol 'PipeWire:Protocol:Native'".
    // render_conf MUST emit a SELF-CONTAINED config, not a bare filter-chain
    // fragment. Assert the load-bearing base module is present.
    assert!(
        c.contains("libpipewire-module-protocol-native"),
        "self-contained base modules missing (would fail to load standalone)"
    );
}

#[test]
fn conf_omits_target_when_no_mic() {
    // When no specific mic is chosen, there must be no target.object pin so the
    // filter-chain follows the system default capture device.
    let cfg = Config {
        mic: None,
        ..Config::default()
    };
    let paths = Paths {
        plugin_so: PathBuf::from("/usr/lib/ladspa/libdpdfnet_ladspa.so"),
        model_dir: PathBuf::from("/usr/share/hushmic/models"),
        dylib: PathBuf::from("/usr/lib/hushmic/libonnxruntime.so"),
    };
    let c = render_conf(&cfg, &paths);
    assert!(
        !c.contains("target.object"),
        "target.object must be absent when no mic chosen"
    );
}
