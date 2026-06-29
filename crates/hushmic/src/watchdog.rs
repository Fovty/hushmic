use std::sync::mpsc::Sender;
use std::time::Duration;

/// A single watchdog beat. The main loop re-checks the child/node on each one.
pub struct Tick;

/// Emits a [`Tick`] roughly every `secs` so the main loop can re-check the
/// child/node and re-instantiate it if it died (suspend / daemon restart).
///
/// The thread exits cleanly once the receiver is dropped (main loop ended).
pub fn spawn(tx: Sender<Tick>, secs: u64) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(secs));
        if tx.send(Tick).is_err() {
            break;
        }
    });
}

/// Exponential backoff for watchdog re-enable attempts (ticks: 0,1,3,7,15,31, cap 60).
pub struct Backoff {
    fails: u32,
    waited: u32,
}
impl Backoff {
    pub fn new() -> Self {
        Self {
            fails: 0,
            waited: 0,
        }
    }
    fn delay(&self) -> u32 {
        if self.fails == 0 {
            0
        } else {
            ((1u32 << self.fails.min(6)) - 1).min(60)
        }
    }
    /// Call once per tick when a re-enable is warranted; true => attempt now.
    pub fn should_attempt(&mut self) -> bool {
        if self.waited >= self.delay() {
            true
        } else {
            self.waited += 1;
            false
        }
    }
    /// Record the attempt outcome (resets the wait counter).
    pub fn record(&mut self, success: bool) {
        self.waited = 0;
        if success {
            self.fails = 0;
        } else {
            self.fails = self.fails.saturating_add(1);
        }
    }
}
impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule() {
        let mut b = Backoff::new();
        assert!(b.should_attempt()); // fails=0 -> immediate
        b.record(false); // fail #1 -> delay 1 tick
        assert!(!b.should_attempt()); // wait
        assert!(b.should_attempt()); // then attempt
        b.record(false); // fail #2 -> delay 3 ticks
        assert!(!b.should_attempt());
        assert!(!b.should_attempt());
        assert!(!b.should_attempt());
        assert!(b.should_attempt());
        b.record(true); // success resets
        assert!(b.should_attempt()); // back to immediate
    }
}
