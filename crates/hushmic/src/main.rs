use hushmic::config::Config;
use hushmic::controller::{Controller, Paths};
use hushmic::pipewire;
use hushmic::tray::{HushmicTray, TrayCmd, TrayStatus};
use hushmic::{autostart, lock, watchdog};
use ksni::blocking::TrayMethods;
use std::sync::mpsc;

/// Unifies the two event sources (tray commands and watchdog ticks) into one
/// channel so the main loop is single-threaded and owns the `Controller`.
enum Event {
    Cmd(TrayCmd),
    Tick,
}

fn compute_status(cfg: &Config, controller: &mut Controller) -> TrayStatus {
    if !cfg.enabled {
        TrayStatus::Off
    } else if controller.is_running() && pipewire::hushmic_source_present() {
        TrayStatus::Active
    } else {
        TrayStatus::Error
    }
}

/// Acquire the single-instance lock, or exit if another hushmic already holds
/// it (a second tray + filter-chain would fight over `hushmic_source`).
fn acquire_single_instance() -> std::fs::File {
    match lock::try_lock(&lock::default_lock_path()) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!("hushmic is already running.");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("hushmic: could not take single-instance lock: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--tray") {
        eprintln!("usage: hushmic --tray");
        // also honor a one-shot enable for scripted/integration use:
        if args.iter().any(|a| a == "--enable-once") {
            let _lock = acquire_single_instance();
            let mut c = Controller::new(Paths::resolve());
            c.enable(&Config::load()).expect("enable");
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
        return;
    }

    let _lock = acquire_single_instance();

    let mut cfg = Config::load();
    let mut controller = Controller::new(Paths::resolve());

    let (tx, rx) = mpsc::channel::<Event>();

    // Tray -> commands
    let (ctx, crx) = mpsc::channel::<TrayCmd>();
    let tray = HushmicTray {
        cfg: cfg.clone(),
        mics: pipewire::list_real_sources(),
        cmd_tx: ctx,
        status: TrayStatus::Off,
    };
    let handle = tray.spawn().expect("spawn tray");

    // bridge TrayCmd -> Event
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for c in crx {
                if tx.send(Event::Cmd(c)).is_err() {
                    break;
                }
            }
        });
    }
    // watchdog -> Event::Tick
    {
        let (wtx, wrx) = mpsc::channel::<watchdog::Tick>();
        watchdog::spawn(wtx, 5);
        let tx = tx.clone();
        std::thread::spawn(move || {
            for _ in wrx {
                if tx.send(Event::Tick).is_err() {
                    break;
                }
            }
        });
    }

    // apply persisted state on launch
    if cfg.autostart != autostart::is_autostart_enabled() {
        let _ = autostart::set_autostart(cfg.autostart);
    }
    if cfg.enabled {
        let _ = controller.enable(&cfg);
    }

    let apply = |controller: &mut Controller, cfg: &Config| {
        if cfg.enabled {
            let _ = controller.enable(cfg);
        } else {
            let _ = controller.disable();
        }
    };

    // watchdog respawn backoff + throttled "node down" logging (state-change only)
    let mut backoff = hushmic::watchdog::Backoff::new();
    let mut logged_down = false;

    for ev in rx {
        match ev {
            Event::Cmd(cmd) => {
                match cmd {
                    TrayCmd::SetEnabled(v) => {
                        cfg.enabled = v;
                        apply(&mut controller, &cfg);
                    }
                    TrayCmd::SelectMic(m) => {
                        cfg.mic = m;
                        if cfg.enabled {
                            apply(&mut controller, &cfg);
                        }
                    }
                    TrayCmd::SelectModel(m) => {
                        cfg.model = m;
                        if cfg.enabled {
                            apply(&mut controller, &cfg);
                        }
                    }
                    TrayCmd::SetAttn(v) => {
                        cfg.attn_limit = v;
                        if cfg.enabled {
                            apply(&mut controller, &cfg);
                        }
                    }
                    TrayCmd::SetDefaultToggle(v) => {
                        cfg.set_default = v;
                        if cfg.enabled {
                            apply(&mut controller, &cfg);
                        }
                    }
                    TrayCmd::SetAutostart(v) => {
                        cfg.autostart = v;
                        let _ = autostart::set_autostart(v);
                    }
                    TrayCmd::Quit => {
                        let _ = controller.disable();
                        break;
                    }
                }
                let _ = cfg.save();
                // reflect updated state + refreshed mic list + status in the tray
                let status = compute_status(&cfg, &mut controller);
                let new_mics = pipewire::list_real_sources();
                let snapshot = cfg.clone();
                let _ = handle.update(move |t: &mut HushmicTray| {
                    t.cfg = snapshot;
                    t.mics = new_mics;
                    t.status = status;
                });
            }
            Event::Tick => {
                // watchdog: if we should be on but the node is gone, re-instantiate.
                //
                // Liveness must be judged by the *node*, not just the child PID:
                // when the PipeWire daemon restarts (or after suspend) the
                // `pipewire -c` child stays alive with a broken connection yet
                // `hushmic_source` disappears, so `is_running()` alone would never
                // fire. `enable()` reaps any lingering child before respawning.
                //
                // A persistently-broken environment must not respawn every tick
                // and spam the log, so attempts are gated by an exponential
                // backoff (ticks 0,1,3,7,15,31, cap 60) and the "down" line is
                // logged only on the down->state transition.
                let down = cfg.enabled
                    && (!controller.is_running() || !pipewire::hushmic_source_present());
                if down {
                    if !logged_down {
                        eprintln!("[hushmic] node not running; attempting re-instantiation");
                        logged_down = true;
                    }
                    if backoff.should_attempt() {
                        // "Success" must match the liveness model (node present), not
                        // just a live child PID: a broken install leaves `pipewire -c`
                        // alive but creates no node, and judging success by the child
                        // alone would reset the backoff every tick and respawn forever.
                        let ok = controller.enable(&cfg).is_ok()
                            && controller.is_running()
                            && pipewire::hushmic_source_present();
                        backoff.record(ok);
                        if ok {
                            logged_down = false;
                        }
                    }
                } else {
                    backoff.record(true); // healthy -> reset
                    logged_down = false;
                }
                // reflect liveness in the tray status (icon + title) every tick
                let status = compute_status(&cfg, &mut controller);
                let _ = handle.update(move |t: &mut HushmicTray| {
                    t.status = status;
                });
            }
        }
    }
}
