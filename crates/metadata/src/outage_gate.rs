//! Lets parallel scan workers wait out a network outage together.
//!
//! Without it a dropped connection fails every remote file the workers pick
//! up next: each read errors at once, so a few seconds offline (phone
//! switching networks, Wi-Fi dozing) marked hundreds of files unreadable.
//! After a failed read a worker calls [`OutageGate::after_failure`]: the
//! first one checks whether the storage answers at all and, while it does
//! not, keeps checking with growing pauses; the other workers block in
//! [`OutageGate::wait`] / `after_failure` meanwhile instead of burning
//! through the file list. When the storage is back, all of them retry.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// First pause between reachability checks, doubled up to [`MAX_DELAY`].
const FIRST_DELAY: Duration = Duration::from_secs(1);
/// Short enough that a scan resumes soon after the network is back.
const MAX_DELAY: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Up,
    /// One worker is checking; the others wait.
    Down,
    /// Unreachable for longer than the gate's limit; reads fail at once.
    GaveUp,
}

/// What [`OutageGate::after_failure`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// The storage answers: the failure was the file's.
    Reachable,
    /// The storage was unreachable and came back: worth retrying, the
    /// failure says nothing about the file.
    CameBack,
    /// Still unreachable after the gate's limit.
    Unreachable,
}

pub struct OutageGate {
    state: Mutex<State>,
    changed: Condvar,
    /// Total pause time before giving up. Counted in pauses, not wall
    /// time: a process frozen in the background (Android) must still get
    /// its full set of checks after it thaws.
    max_outage: Duration,
}

impl OutageGate {
    #[must_use]
    pub const fn new(max_outage: Duration) -> Self {
        Self {
            state: Mutex::new(State::Up),
            changed: Condvar::new(),
            max_outage,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait_while_down<'a>(&self, mut state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        while *state == State::Down {
            state = self.changed.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
        state
    }

    /// Blocks while an outage is being waited out; `false` once the gate
    /// gave up (the storage stayed unreachable).
    pub fn wait(&self) -> bool {
        *self.wait_while_down(self.lock()) == State::Up
    }

    /// True while a worker is waiting for the storage to come back.
    pub fn is_down(&self) -> bool {
        *self.lock() == State::Down
    }

    /// Called after a failed read: checks the storage, waiting out an
    /// outage. `reachable` and `sleep` are parameters for tests.
    pub fn after_failure(&self, reachable: impl Fn() -> bool, sleep: impl Fn(Duration)) -> Check {
        {
            let mut state = self.lock();
            match *state {
                State::GaveUp => return Check::Unreachable,
                // Another worker is already checking; this failure most
                // likely came from the same outage.
                State::Down => {
                    return if *self.wait_while_down(state) == State::Up { Check::CameBack } else { Check::Unreachable };
                }
                State::Up => *state = State::Down,
            }
        }
        let mut delay = FIRST_DELAY;
        let mut waited = Duration::ZERO;
        let outcome = loop {
            if reachable() {
                break State::Up;
            }
            if waited >= self.max_outage {
                break State::GaveUp;
            }
            sleep(delay);
            waited += delay;
            delay = (delay * 2).min(MAX_DELAY);
        };
        *self.lock() = outcome;
        self.changed.notify_all();
        match outcome {
            State::Up if waited.is_zero() => Check::Reachable,
            State::Up => Check::CameBack,
            _ => Check::Unreachable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reachable_storage_means_retry() {
        let gate = OutageGate::new(Duration::from_secs(60));
        assert_eq!(gate.after_failure(|| true, |_| panic!("no pause when reachable")), Check::Reachable);
        assert!(gate.wait());
    }

    #[test]
    fn waits_out_an_outage_with_growing_pauses() {
        let gate = OutageGate::new(Duration::from_secs(300));
        let checks = AtomicUsize::new(0);
        let pauses = Mutex::new(Vec::new());
        let back = gate.after_failure(|| checks.fetch_add(1, Ordering::SeqCst) >= 4, |d| pauses.lock().unwrap().push(d.as_secs()));
        assert_eq!(back, Check::CameBack);
        assert_eq!(*pauses.lock().unwrap(), vec![1, 2, 4, 8]);
        assert!(gate.wait());
    }

    #[test]
    fn gives_up_after_the_limit() {
        let gate = OutageGate::new(Duration::from_secs(60));
        let pauses = Mutex::new(Vec::new());
        assert_eq!(gate.after_failure(|| false, |d| pauses.lock().unwrap().push(d.as_secs())), Check::Unreachable);
        // 1+2+4+8+10+10+10+10+10 = 65 s of pauses passes the 60 s limit.
        assert_eq!(*pauses.lock().unwrap(), vec![1, 2, 4, 8, 10, 10, 10, 10, 10]);
        assert!(!gate.wait());
        assert_eq!(gate.after_failure(|| true, |_| {}), Check::Unreachable);
    }

    #[test]
    fn other_workers_wait_for_the_checker() {
        let gate = Arc::new(OutageGate::new(Duration::from_secs(300)));
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let checker = {
            let gate = gate.clone();
            std::thread::spawn(move || {
                // Unreachable until the test releases it.
                gate.after_failure(|| release_rx.lock().unwrap().try_recv().is_ok(), |_| std::thread::sleep(Duration::from_millis(5)))
            })
        };
        while !gate.is_down() {
            std::thread::yield_now();
        }
        let waiter = std::thread::spawn(move || gate.wait());
        std::thread::sleep(Duration::from_millis(50));
        assert!(!waiter.is_finished(), "a worker must not start a read during an outage");
        release_tx.send(()).unwrap();
        assert_eq!(checker.join().unwrap(), Check::CameBack);
        assert!(waiter.join().unwrap());
    }
}
