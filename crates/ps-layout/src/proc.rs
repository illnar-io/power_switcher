//! Subprocess helper with Python-parity timeouts.
//!
//! The Python daemon uses `subprocess.run(cmd, capture_output=True, timeout=N)`
//! everywhere.  `std::process` has no timeout, so this module reproduces the
//! semantics: spawn with piped stdout, read the output on a separate thread
//! (so a full pipe cannot deadlock), poll for the deadline, kill on expiry and
//! return `None` (Python raises `TimeoutExpired`, which the callers swallow).

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Poll interval while waiting for the child (10 ms — finer than any of the
/// Python timeouts, coarse enough to be cheap).
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Outcome of a timed run: captured stdout plus, when the process exited in
/// time, whether it succeeded.
pub struct Captured {
    pub stdout: String,
    pub success: Option<bool>,
}

fn spawn_piped(program: &str, args: &[&str]) -> Option<Child> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn finish_with_timeout(mut child: Child, timeout: Duration) -> Option<Captured> {
    let mut stdout_pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Some(Captured {
                    stdout: reader.join().unwrap_or_default(),
                    success: Some(status.success()),
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Timeout: kill like Python's TimeoutExpired handling.
                    let _ = child.kill();
                    let _ = child.wait();
                    let drained = reader.join().unwrap_or_default();
                    return Some(Captured {
                        stdout: drained,
                        success: None,
                    });
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return None,
        }
    }
}

/// Run a command and capture stdout; `None` only when the binary cannot be
/// spawned.  Python callers read `.stdout` regardless of exit status, so
/// non-zero exits are *not* errors here.
pub fn run_stdout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    Some(finish_with_timeout(spawn_piped(program, args)?, timeout)?.stdout)
}

/// Availability probe: `Some(true/false)` mirrors Python's
/// `returncode == 0`; `None` when the process was killed by the timeout or
/// could not be spawned.
pub fn run_status(program: &str, args: &[&str], timeout: Duration) -> Option<bool> {
    let captured = finish_with_timeout(spawn_piped(program, args)?, timeout)?;
    captured.success
}

/// Run and capture stdout *and* status in one shot.
pub fn run_capture(program: &str, args: &[&str], timeout: Duration) -> Option<Captured> {
    finish_with_timeout(spawn_piped(program, args)?, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn captures_stdout() {
        let out = run_stdout("echo", &["hello"], Duration::from_secs(2));
        assert_eq!(out.as_deref(), Some("hello\n"));
    }

    #[test]
    fn missing_binary_returns_none() {
        let out = run_stdout("definitely-not-a-binary-xyz", &[], Duration::from_secs(2));
        assert!(out.is_none());
        let status = run_status("definitely-not-a-binary-xyz", &[], Duration::from_secs(2));
        assert!(status.is_none());
    }

    #[test]
    fn hanging_process_is_killed() {
        // `sleep 5` produces no output; we must not wait past the timeout.
        let start = std::time::Instant::now();
        let captured = run_capture("sleep", &["5"], Duration::from_millis(150));
        assert!(captured.is_some());
        let captured = captured.unwrap();
        assert!(
            captured.success.is_none(),
            "timeout must not report success"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn status_probe() {
        let ok = run_status("true", &[], Duration::from_secs(2));
        assert_eq!(ok, Some(true));
        let fail = run_status("false", &[], Duration::from_secs(2));
        assert_eq!(fail, Some(false));
    }
}
