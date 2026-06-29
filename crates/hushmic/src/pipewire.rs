use std::process::Command;

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub description: String,
}

/// Parse `pw-dump` JSON into the audio capture sources PipeWire exposes
/// (`media.class == "Audio/Source"` nodes).
///
/// hushmic is PipeWire-native: source enumeration and the watchdog liveness
/// check use `pw-dump` (and defaults use `pw-metadata`) — both ship in the
/// `pipewire`/`pipewire-bin` package set the project already depends on. They do
/// NOT use PulseAudio's `pactl`, which lives in the separate `pulseaudio-utils`
/// package that is absent on minimal installs and the Ubuntu live image; relying
/// on it made `hushmic_source_present()` silently return false there, so the
/// watchdog never saw the (correctly created) node and re-instantiated forever.
///
/// Returns EVERY Audio/Source node (including monitors and our own
/// `hushmic_source`); callers filter as needed. Pure function — no I/O.
pub fn parse_pwdump_nodes(stdout: &str) -> Vec<Source> {
    let v: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for o in arr {
        if o.get("type").and_then(|t| t.as_str()) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let Some(props) = o.get("info").and_then(|i| i.get("props")) else {
            continue;
        };
        if props.get("media.class").and_then(|c| c.as_str()) != Some("Audio/Source") {
            continue;
        }
        let name = match props.get("node.name").and_then(|n| n.as_str()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        // Friendly label for the tray: node.description, else node.nick, else the
        // node name (mirrors the human-readable name pactl's Description gave).
        let description = props
            .get("node.description")
            .and_then(|d| d.as_str())
            .or_else(|| props.get("node.nick").and_then(|d| d.as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.clone());
        out.push(Source { name, description });
    }
    out
}

/// Extract the node name from a pw-metadata value line: value:'{"name":"X"}'.
pub fn parse_metadata_value(stdout: &str) -> Option<String> {
    let v = stdout.split("value:'").nth(1)?;
    let json = v.split('\'').next()?; // {"name":"X"}
    let after = json.split("\"name\":\"").nth(1)?;
    Some(after.split('"').next()?.to_string())
}

/// Run `pw-dump` and return its stdout (empty string on any failure).
fn pw_dump() -> String {
    Command::new("pw-dump")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// List real capture sources, excluding our own `hushmic_source` and any
/// `.monitor` monitor sources.
pub fn list_real_sources() -> Vec<Source> {
    parse_pwdump_nodes(&pw_dump())
        .into_iter()
        .filter(|s| s.name != "hushmic_source" && !s.name.ends_with(".monitor"))
        .collect()
}

/// True if our virtual mic node `hushmic_source` is currently a live PipeWire
/// source. Used by the watchdog: the host child can linger after a daemon
/// restart while the node itself is gone, so node presence (not child PID) is
/// the real liveness signal.
pub fn hushmic_source_present() -> bool {
    parse_pwdump_nodes(&pw_dump())
        .iter()
        .any(|s| s.name == "hushmic_source")
}

/// Get the currently configured default source node name.
pub fn get_default_source() -> Option<String> {
    let out = Command::new("pw-metadata")
        .args(["-n", "default", "0", "default.configured.audio.source"])
        .output()
        .ok()?;
    parse_metadata_value(&String::from_utf8_lossy(&out.stdout))
}

/// Set the default source node name via pw-metadata.
pub fn set_default_source(node_name: &str) -> std::io::Result<()> {
    let val = format!("{{\"name\":\"{node_name}\"}}");
    let st = Command::new("pw-metadata")
        .args([
            "-n",
            "default",
            "0",
            "default.configured.audio.source",
            &val,
            "Spa:String:JSON",
        ])
        .status()?;
    if st.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("pw-metadata set failed"))
    }
}

/// Delete the `default.configured.audio.source` metadata key entirely.
///
/// Used on teardown when this run set the default but there was no prior
/// configured default to restore: leaving the key pointed at the now-dead
/// `hushmic_source` would strand the system default on a vanished node, so we
/// delete the key instead, returning PipeWire to "no configured default".
pub fn clear_default_source() -> std::io::Result<()> {
    let st = Command::new("pw-metadata")
        .args([
            "-n",
            "default",
            "-d",
            "0",
            "default.configured.audio.source",
        ])
        .status()?;
    if st.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("pw-metadata delete failed"))
    }
}
