use directories::BaseDirs;
use std::path::PathBuf;

/// Build the `Exec=` line for the autostart entry.
///
/// When running as an AppImage the live binary is at an ephemeral mount
/// (`/tmp/.mount_*`), so the autostart entry must point at the AppImage *file*
/// instead — its runtime exports `$APPIMAGE` with that absolute path. (If the
/// user later moves the AppImage, the entry goes stale; that's inherent to the
/// AppImage format.) For a normal install, `hushmic` is on `PATH`.
///
/// Pure helper so the path logic is unit-testable without touching the env.
fn exec_field_for(appimage: Option<&str>) -> String {
    match appimage {
        Some(p) if !p.is_empty() => format!("\"{p}\" --tray"),
        _ => "hushmic --tray".to_string(),
    }
}

fn exec_field() -> String {
    exec_field_for(std::env::var("APPIMAGE").ok().as_deref())
}

/// The full `hushmic.desktop` autostart entry, with the right `Exec=` for how
/// this build was launched (installed command vs AppImage path).
pub fn desktop_contents() -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name=Hushmic
Comment=Real-time microphone noise suppression
Exec={exec}
Icon=hushmic
Terminal=false
Categories=AudioVideo;Audio;
X-GNOME-Autostart-enabled=true
",
        exec = exec_field()
    )
}

pub fn desktop_path() -> PathBuf {
    BaseDirs::new().expect("home").config_dir().join("autostart").join("hushmic.desktop")
}

pub fn is_autostart_enabled() -> bool {
    desktop_path().exists()
}

pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    let p = desktop_path();
    if enabled {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, desktop_contents())
    } else if p.exists() {
        std::fs::remove_file(p)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_path_is_in_autostart() {
        let p = desktop_path();
        assert!(p.ends_with("autostart/hushmic.desktop"), "unexpected path: {p:?}");
    }

    #[test]
    fn installed_exec_uses_path_command() {
        assert_eq!(exec_field_for(None), "hushmic --tray");
        assert_eq!(exec_field_for(Some("")), "hushmic --tray");
    }

    #[test]
    fn appimage_exec_points_at_the_appimage_file() {
        assert_eq!(
            exec_field_for(Some("/home/u/Apps/Hushmic.AppImage")),
            "\"/home/u/Apps/Hushmic.AppImage\" --tray"
        );
    }

    #[test]
    fn contents_are_a_valid_desktop_entry() {
        let c = desktop_contents();
        assert!(c.contains("Type=Application"));
        assert!(c.contains("Name=Hushmic"));
        assert!(c.contains("--tray"));
    }
}
