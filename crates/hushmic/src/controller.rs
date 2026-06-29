use crate::config::Config;
use crate::pipewire;
use directories::ProjectDirs;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};

/// Filesystem locations the controller needs to spawn the filter-chain host.
///
/// `plugin_so` is the v0.1 LADSPA `.so`, `model_dir` holds the `<model>.onnx`
/// files, and `dylib` is the ONNX Runtime shared object that the plugin
/// `dlopen`s via the `ORT_DYLIB_PATH` env var.
pub struct Paths {
    pub plugin_so: PathBuf,
    pub model_dir: PathBuf,
    pub dylib: PathBuf,
}

impl Paths {
    /// Env overrides win (dev); else the packaged install locations.
    pub fn resolve() -> Self {
        let plugin_so = std::env::var("HUSHMIC_PLUGIN_SO")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/lib/ladspa/libdpdfnet_ladspa.so"));
        let model_dir = std::env::var("HUSHMIC_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/share/hushmic/models"));
        let dylib = std::env::var("ORT_DYLIB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/lib/hushmic/libonnxruntime.so"));
        Paths {
            plugin_so,
            model_dir,
            dylib,
        }
    }
}

fn conf_path() -> PathBuf {
    ProjectDirs::from("io", "hushmic", "hushmic")
        .expect("home")
        .config_dir()
        .join("filter-chain.conf")
}

/// Render a SELF-CONTAINED PipeWire config for `pipewire -c <conf>`.
///
/// v0.1 Task 7 proved that a bare filter-chain *fragment*
/// (`context.modules = [ filter-chain ]`) fails to load standalone with
/// `can't find protocol 'PipeWire:Protocol:Native'` because it carries none of
/// the core modules. Since the controller spawns exactly `pipewire -c <conf>`,
/// this emits the base preamble (`context.properties`, `context.spa-libs`, and
/// the base `context.modules`: rt, protocol-native, client-node, adapter,
/// metadata — mirroring `/usr/share/pipewire/filter-chain.conf` and
/// `minimal.conf`) and then appends the hushmic filter-chain module.
///
/// The filter-chain is mono, 48 kHz, and (when a mic is chosen) pins
/// `target.object`. Pure function — no I/O.
pub fn render_conf(cfg: &Config, paths: &Paths) -> String {
    // target.object line only when a specific mic is chosen; otherwise the
    // filter-chain follows the system default capture device.
    let target = match &cfg.mic {
        Some(name) => format!("        target.object  = \"{name}\"\n"),
        None => String::new(),
    };
    format!(
        r#"# hushmic self-contained PipeWire filter-chain host (generated; do not edit).
# Base modules mirror /usr/share/pipewire/filter-chain.conf so a bare
# `pipewire -c <this>` has the core protocol + node infrastructure; the hushmic
# filter-chain module is then appended. See run-filter-chain.md (v0.1 Task 7).
context.properties = {{
    log.level = 2
}}
context.spa-libs = {{
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}}
context.modules = [
  {{ name = libpipewire-module-rt
    args = {{ }}
    flags = [ ifexists nofail ]
  }}
  {{ name = libpipewire-module-protocol-native }}
  {{ name = libpipewire-module-client-node }}
  {{ name = libpipewire-module-adapter }}
  {{ name = libpipewire-module-metadata }}
  {{ name = libpipewire-module-filter-chain
    flags = [ nofail ]
    args = {{
      node.description = "Hushmic Microphone"
      media.name       = "Hushmic Microphone"
      filter.graph = {{
        nodes = [
          {{ type   = ladspa
            name   = hushmic_dsp
            plugin = "{plugin}"
            label  = "dpdfnet_mono"
            control = {{ "Attenuation Limit (dB)" = {attn} }}
          }}
        ]
      }}
      capture.props = {{
        node.name      = "hushmic_input"
        node.passive   = true
        audio.rate     = 48000
        audio.channels = 1
        audio.position = [ MONO ]
{target}      }}
      playback.props = {{
        node.name        = "hushmic_source"
        node.description  = "Hushmic Microphone"
        media.class      = Audio/Source
        audio.rate       = 48000
        audio.channels   = 1
        audio.position   = [ MONO ]
      }}
    }}
  }}
]
"#,
        plugin = paths.plugin_so.display(),
        attn = cfg.attn_limit,
        target = target,
    )
}

/// Owns the `pipewire -c` child that hosts the virtual mic, plus the prior
/// system default source so it can be restored on teardown.
pub struct Controller {
    paths: Paths,
    child: Option<Child>,
    prior_default: Option<String>,
    /// True when this run repointed the system default to `hushmic_source`, so
    /// `disable` knows it must restore the prior default (if any) or otherwise
    /// clear the now-dangling key. Distinguishes "we set it" from "prior_default
    /// happened to be None", which a bare `Option` could not.
    set_default_active: bool,
}

impl Controller {
    pub fn new(paths: Paths) -> Self {
        Controller {
            paths,
            child: None,
            prior_default: None,
            set_default_active: false,
        }
    }

    /// True only if a spawned child is still alive (reaps it on exit).
    pub fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)), // Ok(None) = still alive
            None => false,
        }
    }

    /// Write the generated conf, spawn the dedicated filter-chain host with the
    /// plugin's runtime env, and (optionally) repoint the default input to us.
    ///
    /// MUST be called from the main thread: the spawn below installs
    /// `PR_SET_PDEATHSIG`, which is *thread-scoped* — it binds the child's
    /// lifetime to the spawning thread, not the process. Every caller (the main
    /// event loop's `Cmd`/`Tick` handlers and the `--enable-once` path) runs on
    /// the main thread, so the death-signal fires on process exit as intended.
    pub fn enable(&mut self, cfg: &Config) -> std::io::Result<()> {
        // ALWAYS tear down first — unconditionally, not just when `is_running()`.
        // `disable()` is idempotent (it `.take()`s the child and clears
        // `set_default_active`/`prior_default`), and it is the ONLY thing that
        // restores a previously-captured prior default. If the spawned child has
        // EXITED (crash / fatal conf / broken env), `is_running()` is false, so a
        // guarded `if self.is_running()` would SKIP `disable()`: the stale
        // `prior_default` (the user's real device) is never restored, the
        // `default.configured.audio.source` key still points at the dead
        // `hushmic_source`, and the unconditional re-capture below would then read
        // that dead node back as the "prior" default — permanently discarding the
        // user's real default. Restoring first means the re-capture sees the real
        // device again. (This is the watchdog-on-exited-child path.)
        self.disable()?;
        let conf = render_conf(cfg, &self.paths);
        let path = conf_path();
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&path, conf)?;

        let model = self.paths.model_dir.join(format!("{}.onnx", cfg.model));
        // Dedicated filter-chain host; env propagates to the plugin's dlopen.
        let mut command = Command::new("pipewire");
        command
            .arg("-c")
            .arg(&path)
            .env("HUSHMIC_MODEL_PATH", &model)
            .env("ORT_DYLIB_PATH", &self.paths.dylib);
        // Bind the host's lifetime to ours: if hushmic dies ungracefully
        // (crash, SIGKILL, session logout) Drop never runs, so without this the
        // child would linger and keep advertising a dead `hushmic_source` as the
        // default mic. PR_SET_PDEATHSIG makes the kernel SIGTERM the child when
        // the spawning (main) thread exits, guaranteeing teardown.
        //
        // INVARIANT: PR_SET_PDEATHSIG is THREAD-scoped — it ties the child to the
        // thread that calls prctl, not to the process. This is correct ONLY
        // because `enable()` is always called from the main thread (see the
        // `enable()` doc comment); were it called from a transient worker thread,
        // the child would be reaped when that worker exits, not on process exit.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        self.child = Some(child);

        if cfg.set_default {
            // May be None on a machine with no configured default input; we
            // still record that *we* set the default so `disable` can clear the
            // key rather than strand it on the dead node. Belt-and-suspenders:
            // never record our OWN virtual node as the prior default — if the key
            // somehow still reads `hushmic_source` here (e.g. a prior restore
            // failed), treat it as "no prior default" (None) so `disable` clears
            // the dangling key instead of "restoring" the system default to a
            // dead node.
            self.prior_default = match pipewire::get_default_source() {
                Some(name) if name == "hushmic_source" => None,
                other => other,
            };
            self.set_default_active = true;
            // Give the node a moment to register before repointing the default.
            std::thread::sleep(std::time::Duration::from_millis(700));
            let _ = pipewire::set_default_source("hushmic_source");
        }
        Ok(())
    }

    /// Restore the prior default (before killing our node so clients don't
    /// briefly land on a dead source) and tear down the child.
    pub fn disable(&mut self) -> std::io::Result<()> {
        // Only undo the default if *this* run set it. Restore the prior default
        // when there was one; otherwise delete the key so it doesn't dangle on
        // the soon-to-be-dead `hushmic_source`. Done BEFORE killing the child so
        // clients never briefly land on a dead source. `.take()` keeps this
        // idempotent across the explicit disable + Drop path.
        if self.set_default_active {
            match self.prior_default.take() {
                Some(prev) => {
                    let _ = pipewire::set_default_source(&prev);
                }
                None => {
                    let _ = pipewire::clear_default_source();
                }
            }
            self.set_default_active = false;
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        Ok(())
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.disable(); // clean teardown + default restore on quit
    }
}
