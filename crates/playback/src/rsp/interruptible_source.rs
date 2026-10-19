//! Network media sources (HTTP streams, remote library files) driven from a
//! worker thread, so the playback thread never blocks in network I/O.
//!
//! The worker opens the source and then serves read/seek requests; the
//! playback thread waits for each reply in short slices and gives up when
//! the stop flag is set (a pause/next answers within ~100 ms even while a
//! host is still connecting) or when one operation stalls longer than
//! [`STALL_TIMEOUT`] (a connection that stays open but stops sending, which
//! the HTTP client's connect/response timeouts don't cover). A worker left
//! blocked on a socket only does I/O: it exits, dropping the source, as soon
//! as that call returns.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{Result, format_err};
use log::warn;
use metadata::radio_meta::RadioMeta;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSource;

/// Longest a single open/read/seek may take before the source counts as dead.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

type Opened = (Box<dyn MediaSource>, Option<RadioMeta>, Hint);

enum Op {
    Read(usize),
    Seek(SeekFrom),
}

enum Reply {
    Read(io::Result<Vec<u8>>),
    Seek(io::Result<u64>),
}

enum WaitError {
    Stopped,
    Stalled,
    WorkerGone,
}

impl From<WaitError> for io::Error {
    fn from(e: WaitError) -> Self {
        match e {
            // Not `Interrupted`: readers retry that kind.
            WaitError::Stopped => Self::other("playback stopped"),
            WaitError::Stalled => Self::new(io::ErrorKind::TimedOut, format!("no data for {}s", STALL_TIMEOUT.as_secs())),
            WaitError::WorkerGone => Self::new(io::ErrorKind::BrokenPipe, "source worker exited"),
        }
    }
}

fn wait<T>(rx: &Receiver<T>, stop: &AtomicBool) -> std::result::Result<T, WaitError> {
    let started = Instant::now();
    loop {
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(v) => return Ok(v),
            Err(RecvTimeoutError::Disconnected) => return Err(WaitError::WorkerGone),
            Err(RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return Err(WaitError::Stopped);
                }
                if started.elapsed() >= STALL_TIMEOUT {
                    return Err(WaitError::Stalled);
                }
            }
        }
    }
}

/// Runs `open` on a worker thread and returns a source that proxies to it.
/// `open` gets `hint` to fill in (e.g. from the response content type) and
/// hands it back.
pub fn open<F>(stop: Arc<AtomicBool>, hint: Hint, open: F) -> Result<Opened>
where
    F: FnOnce(&mut Hint) -> Result<(Box<dyn MediaSource>, Option<RadioMeta>)> + Send + 'static,
{
    let (opened_tx, opened_rx) = mpsc::channel();
    let (op_tx, op_rx) = mpsc::channel::<Op>();
    let (reply_tx, reply_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("source-io".to_string())
        .spawn(move || {
            let mut hint = hint;
            let mut source = match open(&mut hint) {
                Ok((source, radio_meta)) => {
                    let props = (source.is_seekable(), source.byte_len());
                    if opened_tx.send(Ok((props, radio_meta, hint))).is_err() {
                        return;
                    }
                    source
                }
                Err(e) => {
                    opened_tx.send(Err(e)).ok();
                    return;
                }
            };
            // Ends when the proxy is dropped (playback moved on or stopped).
            while let Ok(op) = op_rx.recv() {
                let reply = match op {
                    Op::Read(len) => {
                        let mut buf = vec![0; len];
                        Reply::Read(source.read(&mut buf).map(|n| {
                            buf.truncate(n);
                            buf
                        }))
                    }
                    Op::Seek(pos) => Reply::Seek(source.seek(pos)),
                };
                if reply_tx.send(reply).is_err() {
                    break;
                }
            }
        })?;

    let ((seekable, byte_len), radio_meta, hint) = match wait(&opened_rx, &stop) {
        Ok(opened) => opened?,
        Err(WaitError::Stopped) => return Err(format_err!("playback stopped while opening")),
        Err(e) => return Err(io::Error::from(e).into()),
    };
    let source = InterruptibleSource {
        ops: op_tx,
        replies: Mutex::new(reply_rx),
        stop,
        seekable,
        byte_len,
        broken: false,
    };
    Ok((Box::new(source), radio_meta, hint))
}

struct InterruptibleSource {
    ops: Sender<Op>,
    // Mutex only to make the source `Sync`; `get_mut` never locks.
    replies: Mutex<Receiver<Reply>>,
    stop: Arc<AtomicBool>,
    seekable: bool,
    byte_len: Option<u64>,
    /// An abandoned request's reply would arrive out of order; after a
    /// stop or stall the source refuses further calls.
    broken: bool,
}

impl InterruptibleSource {
    fn call(&mut self, op: Op) -> io::Result<Reply> {
        if self.broken {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "source abandoned after a stop or stall"));
        }
        self.ops.send(op).map_err(|_| io::Error::from(WaitError::WorkerGone))?;
        let replies = self.replies.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner);
        wait(replies, &self.stop).map_err(|e| {
            self.broken = true;
            if matches!(e, WaitError::Stalled) {
                warn!("Network source stalled for {}s, giving up", STALL_TIMEOUT.as_secs());
            }
            e.into()
        })
    }
}

impl Read for InterruptibleSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.call(Op::Read(buf.len()))? {
            Reply::Read(data) => {
                let data = data?;
                buf[..data.len()].copy_from_slice(&data);
                Ok(data.len())
            }
            Reply::Seek(_) => unreachable!("read request answered with a seek reply"),
        }
    }
}

impl Seek for InterruptibleSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self.call(Op::Seek(pos))? {
            Reply::Seek(res) => res,
            Reply::Read(_) => unreachable!("seek request answered with a read reply"),
        }
    }
}

impl MediaSource for InterruptibleSource {
    fn is_seekable(&self) -> bool {
        self.seekable
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::io::ReadOnlySource;

    struct Slow {
        delay: Duration,
    }

    impl Read for Slow {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(self.delay);
            buf[0] = 7;
            Ok(1)
        }
    }

    fn open_slow(stop: &Arc<AtomicBool>, delay: Duration) -> Box<dyn MediaSource> {
        let (source, _, _) = open(stop.clone(), Hint::new(), move |_| {
            Ok((Box::new(ReadOnlySource::new(Slow { delay })) as Box<dyn MediaSource>, None))
        })
        .expect("open");
        source
    }

    #[test]
    fn reads_through_the_worker() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut source = open_slow(&stop, Duration::ZERO);
        let mut buf = [0u8; 4];
        assert_eq!(source.read(&mut buf).unwrap(), 1);
        assert_eq!(buf[0], 7);
        assert!(!source.is_seekable());
    }

    #[test]
    fn stop_interrupts_a_blocked_read() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut source = open_slow(&stop, Duration::from_secs(30));
        let stopper = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            stopper.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let err = source.read(&mut [0u8; 4]).unwrap_err();
        assert_eq!(err.to_string(), "playback stopped");
        assert!(started.elapsed() < Duration::from_secs(1));
        // The abandoned reply must not be mistaken for a later call's.
        assert!(source.read(&mut [0u8; 4]).is_err());
    }

    #[test]
    fn stop_interrupts_a_blocked_open() {
        let stop = Arc::new(AtomicBool::new(true));
        let started = Instant::now();
        let res = open(stop, Hint::new(), |_| {
            std::thread::sleep(Duration::from_secs(30));
            Err(format_err!("unreachable"))
        });
        assert!(res.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
