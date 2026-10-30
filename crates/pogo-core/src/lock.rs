//! Per-device lock files (docs/agent-contract.md §6.8).
//!
//! The lock is an OS file lock, so it is released when the process exits, even on a
//! crash. Who holds it is written to a separate `.holder` file because Windows locks
//! are mandatory and would stop other processes from reading the lock file itself.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::deadline::Deadline;
use crate::error::PogoError;

const POLL: Duration = Duration::from_millis(50);
const HOLDER_GRACE: Duration = Duration::from_millis(200);

/// Held for a command's whole duration; dropping it releases the device.
#[derive(Debug)]
pub struct DeviceLock {
    _file: File,
    holder_path: PathBuf,
}

impl Drop for DeviceLock {
    /// Remove the holder note while still holding the lock, so a later DEVICE_BUSY never
    /// names a process that has already finished.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.holder_path);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Holder {
    pid: u32,
    command: String,
}

/// Where lock files live: the runtime directory where the platform has one, else the
/// per-user temp directory.
pub fn lock_dir() -> PathBuf {
    directories::BaseDirs::new()
        .and_then(|d| d.runtime_dir().map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir)
        .join("pogo")
        .join("locks")
}

/// Lock the device identified by `key` (USB serial, or port path as a fallback).
/// With `wait`, retry until it frees up or the wait runs out; otherwise fail at once.
pub fn lock_device(
    key: &str,
    command: &str,
    wait: Option<Duration>,
) -> Result<DeviceLock, PogoError> {
    lock_in(&lock_dir(), key, command, wait)
}

pub fn lock_in(
    dir: &Path,
    key: &str,
    command: &str,
    wait: Option<Duration>,
) -> Result<DeviceLock, PogoError> {
    let io = |what: &str, e: std::io::Error| {
        PogoError::Internal(format!("cannot {what} lock file in {}: {e}", dir.display()))
    };
    std::fs::create_dir_all(dir).map_err(|e| io("create", e))?;
    let name = sanitize(key);
    let path = dir.join(format!("{name}.lock"));
    let holder_path = dir.join(format!("{name}.holder"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| io("open", e))?;
    let deadline = Deadline::after(wait.unwrap_or(Duration::ZERO));
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => match deadline.remaining() {
                Some(left) => std::thread::sleep(left.min(POLL)),
                None => return Err(busy(&holder_path)),
            },
            Err(TryLockError::Error(e)) => return Err(io("lock", e)),
        }
    }
    let holder = Holder {
        pid: std::process::id(),
        command: command.to_string(),
    };
    // Best effort: a missing holder file only makes a DEVICE_BUSY message less specific.
    if let Ok(text) = serde_json::to_string(&holder) {
        let _ = std::fs::write(&holder_path, text);
    }
    Ok(DeviceLock {
        _file: file,
        holder_path,
    })
}

/// The holder writes its note just after locking, so give it a moment to appear.
fn busy(holder_path: &Path) -> PogoError {
    let read = || {
        std::fs::read_to_string(holder_path)
            .ok()
            .and_then(|t| serde_json::from_str::<Holder>(&t).ok())
    };
    let give_up = Deadline::after(HOLDER_GRACE);
    let mut holder = read();
    while holder.is_none() && !give_up.is_expired() {
        std::thread::sleep(Duration::from_millis(10));
        holder = read();
    }
    match holder {
        Some(h) => PogoError::DeviceBusy {
            pid: h.pid,
            command: h.command,
        },
        None => PogoError::DeviceBusy {
            pid: 0,
            command: "unknown".into(),
        },
    }
}

/// A file name for any key: `/dev/cu.usbmodem1` → `_dev_cu.usbmodem1`.
fn sanitize(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::error::ErrorCode;

    fn dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pogo-lock-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn second_lock_is_busy_and_names_the_holder() {
        let d = dir("busy");
        let _held = lock_in(&d, "F4:12:FA", "serial.capture", None).unwrap();
        let err = lock_in(&d, "F4:12:FA", "flash", None).unwrap_err();
        assert_eq!(err.code(), ErrorCode::DeviceBusy);
        let PogoError::DeviceBusy { pid, command } = err else {
            unreachable!()
        };
        assert_eq!(
            (pid, command.as_str()),
            (std::process::id(), "serial.capture")
        );
    }

    #[test]
    fn holder_note_appears_while_held_and_goes_on_release() {
        let d = dir("note");
        let held = lock_in(&d, "k", "a", None).unwrap();
        assert!(d.join("k.holder").exists());
        drop(held);
        assert!(!d.join("k.holder").exists());
    }

    #[test]
    fn busy_waits_briefly_for_a_late_holder_note() {
        let d = dir("late");
        std::fs::create_dir_all(&d).unwrap();
        let note = d.join("k.holder");
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::write(note, r#"{"pid":42,"command":"flash"}"#).unwrap();
        });
        let err = busy(&d.join("k.holder"));
        writer.join().unwrap();
        let PogoError::DeviceBusy { pid, command } = err else {
            unreachable!()
        };
        assert_eq!((pid, command.as_str()), (42, "flash"));
    }

    #[test]
    fn dropping_the_lock_frees_the_device() {
        let d = dir("drop");
        drop(lock_in(&d, "k", "a", None).unwrap());
        assert!(lock_in(&d, "k", "b", None).is_ok());
    }

    #[test]
    fn different_devices_do_not_conflict() {
        let d = dir("two");
        let _a = lock_in(&d, "AA", "a", None).unwrap();
        assert!(lock_in(&d, "BB", "b", None).is_ok());
    }

    #[test]
    fn wait_lock_gets_the_device_once_released() {
        let d = dir("wait");
        let held = lock_in(&d, "k", "a", None).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(held);
        });
        let start = Instant::now();
        let got = lock_in(&d, "k", "b", Some(Duration::from_secs(2)));
        releaser.join().unwrap();
        assert!(got.is_ok());
        assert!(start.elapsed() >= Duration::from_millis(80));
    }

    #[test]
    fn wait_lock_gives_up_after_the_wait() {
        let d = dir("giveup");
        let _held = lock_in(&d, "k", "a", None).unwrap();
        let start = Instant::now();
        let err = lock_in(&d, "k", "b", Some(Duration::from_millis(120))).unwrap_err();
        assert_eq!(err.code(), ErrorCode::DeviceBusy);
        assert!(start.elapsed() >= Duration::from_millis(120));
    }

    #[test]
    fn keys_become_safe_file_names() {
        assert_eq!(sanitize("/dev/cu.usbmodem1101"), "_dev_cu.usbmodem1101");
        assert_eq!(sanitize("F4:12:FA:9C:3A:10"), "F4_12_FA_9C_3A_10");
        assert_eq!(sanitize(r"\\.\COM3"), "__._COM3");
    }
}
