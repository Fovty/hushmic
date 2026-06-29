use hushmic::pipewire::{parse_metadata_value, parse_pwdump_nodes};

/// A trimmed `pw-dump` array: a Device (not a Node), our virtual source, a real
/// RODE capture source, a Sink (not a source), a sink monitor source, and a
/// nick-only source. Mirrors the real shape captured on PipeWire 1.0.5.
const PWDUMP: &str = r#"[
  { "id": 10, "type": "PipeWire:Interface:Device",
    "info": { "props": { "device.name": "alsa_card.pci-0000_00_1f.3" } } },
  { "id": 39, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source",
      "node.name": "hushmic_source", "node.description": "hushmic Microphone" } } },
  { "id": 46, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source",
      "node.name": "alsa_input.usb-RODE_Microphones_RODE_NT-USB-00.analog-stereo",
      "node.description": "RODE NT-USB Analog Stereo" } } },
  { "id": 52, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Sink",
      "node.name": "alsa_output.pci-0000_00_1f.3.analog-stereo", "node.description": "Speakers" } } },
  { "id": 55, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source",
      "node.name": "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor",
      "node.description": "Monitor of Speakers" } } },
  { "id": 60, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source",
      "node.name": "alsa_input.usb-Webcam", "node.nick": "Webcam Mic" } } }
]"#;

#[test]
fn parses_pwdump_audio_sources_only() {
    let s = parse_pwdump_nodes(PWDUMP);
    let names: Vec<_> = s.iter().map(|x| x.name.as_str()).collect();
    // All four Audio/Source nodes are returned (Device + the Audio/Sink excluded).
    assert_eq!(s.len(), 4, "got {:?}", s);
    assert!(names.contains(&"hushmic_source"));
    assert!(names.contains(&"alsa_input.usb-RODE_Microphones_RODE_NT-USB-00.analog-stereo"));
    assert!(names.contains(&"alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"));
    // The Audio/Sink "Speakers" must NOT appear as a source.
    assert!(!names.contains(&"alsa_output.pci-0000_00_1f.3.analog-stereo"));
}

#[test]
fn friendly_description_with_nick_fallback() {
    let s = parse_pwdump_nodes(PWDUMP);
    let rode = s
        .iter()
        .find(|x| x.name.contains("RODE"))
        .expect("RODE source");
    assert_eq!(rode.description, "RODE NT-USB Analog Stereo");
    // node.nick is used when node.description is absent.
    let webcam = s
        .iter()
        .find(|x| x.name == "alsa_input.usb-Webcam")
        .expect("webcam source");
    assert_eq!(webcam.description, "Webcam Mic");
}

#[test]
fn real_source_filter_excludes_hushmic_and_monitor() {
    // Same predicate list_real_sources() applies after parsing.
    let real: Vec<_> = parse_pwdump_nodes(PWDUMP)
        .into_iter()
        .filter(|s| s.name != "hushmic_source" && !s.name.ends_with(".monitor"))
        .collect();
    let names: Vec<_> = real.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(real.len(), 2, "got {:?}", real);
    assert!(names.contains(&"alsa_input.usb-RODE_Microphones_RODE_NT-USB-00.analog-stereo"));
    assert!(names.contains(&"alsa_input.usb-Webcam"));
    assert!(!names.contains(&"hushmic_source"));
}

#[test]
fn hushmic_source_presence_detected() {
    // Present in the full dump...
    assert!(parse_pwdump_nodes(PWDUMP)
        .iter()
        .any(|s| s.name == "hushmic_source"));
    // ...absent when the node is gone (watchdog must then re-instantiate).
    let without = r#"[
      { "id": 46, "type": "PipeWire:Interface:Node",
        "info": { "props": { "media.class": "Audio/Source",
          "node.name": "alsa_input.usb-RODE", "node.description": "RODE" } } }
    ]"#;
    assert!(!parse_pwdump_nodes(without)
        .iter()
        .any(|s| s.name == "hushmic_source"));
}

#[test]
fn empty_or_garbage_pwdump_is_safe() {
    assert!(parse_pwdump_nodes("").is_empty());
    assert!(parse_pwdump_nodes("not json").is_empty());
    assert!(parse_pwdump_nodes("[]").is_empty());
}

#[test]
fn extracts_metadata_name() {
    let out = r#"Found "default" metadata
update: id:0 key:'default.configured.audio.source' value:'{"name":"alsa_input.usb-RODE"}' type:'Spa:String:JSON'"#;
    assert_eq!(parse_metadata_value(out).as_deref(), Some("alsa_input.usb-RODE"));
}
