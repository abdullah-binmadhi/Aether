//! Centralized external-command helper.
//!
//! The Python code called `subprocess.run(...)` ~87 times. This module is the
//! single choke point for spawning processes, with a hard timeout so a hung
//! `networksetup`/`ping`/`curl` can never freeze the UI thread (the engines run
//! off the UI thread, but the timeout is still a correctness guarantee).

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Result of a command run: captured stdout, stderr, and whether it exited 0.
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl Output {
    fn failed() -> Self {
        Output {
            stdout: String::new(),
            stderr: String::new(),
            success: false,
        }
    }
    /// Combined stdout + "\n" + stderr, trimmed — mirrors wan_doctor's `_run_cmd`.
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
            .trim()
            .to_string()
    }
}

/// Run `argv` with a timeout. Never panics; on spawn error or timeout returns a
/// non-success `Output`. The child is killed if it exceeds `timeout`.
pub fn run(argv: &[&str], timeout: Duration) -> Output {
    run_with_stdin(argv, None, timeout)
}

/// Run with optional stdin payload (used by the `scutil` reason-code probe).
pub fn run_with_stdin(argv: &[&str], stdin_data: Option<&str>, timeout: Duration) -> Output {
    if argv.is_empty() {
        return Output::failed();
    }
    let mut cmd = Command::new(argv[0]);
    cmd.args(&argv[1..])
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Strip proxy env so probes aren't tainted (mirrors wan_doctor._clean_env).
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        cmd.env_remove(key);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Output::failed(),
    };

    if let (Some(data), Some(mut sin)) = (stdin_data, child.stdin.take()) {
        use std::io::Write;
        let _ = sin.write_all(data.as_bytes());
        // Drop closes the pipe so the child sees EOF.
    }

    // Drain pipes on threads so a child that fills a pipe buffer can't deadlock.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_handle = thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let err_handle = thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });

    // Poll for completion up to `timeout`, killing the child if it overruns.
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break None,
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Output {
        success: status.map(|s| s.success()).unwrap_or(false),
        stdout,
        stderr,
    }
}

/// Convenience: true if the command exits 0 within the timeout.
pub fn ok(argv: &[&str], timeout: Duration) -> bool {
    run(argv, timeout).success
}
