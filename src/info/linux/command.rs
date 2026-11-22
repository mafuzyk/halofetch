//! External commands with a deadline. A package manager or the Nix daemon must
//! never hang the fetch; a command that misses its deadline is killed.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Interval between liveness checks. Small enough to keep the deadline accurate,
/// large enough not to spin while the command works.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Runs `command` with an empty stdin, a captured stdout and a discarded stderr.
/// Returns `None` when the command cannot start, its stdout cannot be captured or
/// it does not finish before `timeout`. The child is killed on timeout.
pub(super) fn output_with_timeout(mut command: Command, timeout: Duration) -> Option<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    let _reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };

    // A grandchild can keep the stdout pipe open after the command exits. Waiting
    // for the reader only until the deadline keeps the timeout a hard guarantee.
    let remaining = deadline.saturating_duration_since(Instant::now());
    let stdout = receiver.recv_timeout(remaining).ok()?;
    Some(Output {
        status,
        stdout,
        stderr: Vec::new(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_fast_command_returns_its_output() {
        let mut cmd = Command::new("printf");
        cmd.arg("hello");
        let output = output_with_timeout(cmd, Duration::from_secs(5)).expect("printf finishes");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello");
    }

    #[test]
    fn a_hung_command_is_killed() {
        let started = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let output = output_with_timeout(cmd, Duration::from_millis(50));
        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_grandchild_holding_stdout_cannot_hang_the_deadline() {
        let started = Instant::now();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 1 &"]);
        let output = output_with_timeout(cmd, Duration::from_millis(50));
        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
