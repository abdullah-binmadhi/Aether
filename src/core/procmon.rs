//! Per-process bandwidth monitor + non-destructive SIGSTOP/SIGCONT firewall.
//!
//! `nettop` is still used (there is no public per-process byte-counter API), but
//! in one-shot cumulative mode (`-L 1`, ~20ms measured) with deltas computed here,
//! instead of the Python version's slow blocking delta mode.
//!
//! Crucially, `resume_all()` is wired into both the Ctrl-C/SIGTERM handler and the
//! TUI's teardown, so a frozen process can never be left suspended on exit — the
//! defect the Python original had (only SIGINT unfroze; SIGHUP/SIGTERM leaked).

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::Instant;

/// System/terminal processes that must never be frozen.
pub const PROTECTED: &[&str] = &[
    "kernel_task",
    "launchd",
    "WindowServer",
    "mDNSResponder",
    "configd",
    "airportd",
    "wifi-tui",
    "kitty",
    "Terminal",
    "iTerm2",
    "loginwindow",
];

#[derive(Debug, Clone)]
pub struct ProcSample {
    pub pid: i32,
    pub name: String,
    pub down_kbs: f64,
    pub up_kbs: f64,
    pub total_mb: f64,
    pub paused: bool,
    pub is_hog: bool,
    pub protected: bool,
}

pub struct ProcMonitor {
    prev_totals: HashMap<i32, (u64, u64)>,
    prev_ts: Option<Instant>,
    paused_pids: HashSet<i32>,
    pub last: Vec<ProcSample>,
}

impl ProcMonitor {
    pub fn new() -> Self {
        ProcMonitor {
            prev_totals: HashMap::new(),
            prev_ts: None,
            paused_pids: HashSet::new(),
            last: Vec::new(),
        }
    }

    /// Parse `nettop -P -x -L 1 -t wifi -J bytes_in,bytes_out` cumulative output.
    fn parse(csv: &str) -> Vec<(String, i32, u64, u64)> {
        let mut out = Vec::new();
        for line in csv.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(",bytes_in") || line.starts_with("time,") {
                continue;
            }
            let parts: Vec<&str> = line.trim_end_matches(',').split(',').collect();
            if parts.len() < 3 {
                continue;
            }
            let proc_field = parts[0];
            // Format is "name.pid"; split on the LAST dot.
            let Some(dot) = proc_field.rfind('.') else {
                continue;
            };
            let name = &proc_field[..dot];
            let Ok(pid) = proc_field[dot + 1..].parse::<i32>() else {
                continue;
            };
            let rx = parts[1].parse::<u64>().unwrap_or(0);
            let tx = parts[2].parse::<u64>().unwrap_or(0);
            if rx > 0 || tx > 0 {
                out.push((name.to_string(), pid, rx, tx));
            }
        }
        out
    }

    pub fn sample(&mut self) -> &[ProcSample] {
        let now = Instant::now();
        let dt = self
            .prev_ts
            .map(|t| (now - t).as_secs_f64().max(0.2))
            .unwrap_or(1.0);

        let raw = Command::new("nettop")
            .args([
                "-P",
                "-x",
                "-L",
                "1",
                "-t",
                "wifi",
                "-J",
                "bytes_in,bytes_out",
            ])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();

        let parsed = Self::parse(&raw);
        let mut procs = Vec::with_capacity(parsed.len());
        let mut new_totals = HashMap::with_capacity(parsed.len());

        for (name, pid, rx, tx) in parsed {
            new_totals.insert(pid, (rx, tx));
            let (down_kbs, up_kbs) = match self.prev_totals.get(&pid) {
                Some(&(prx, ptx)) if self.prev_ts.is_some() => (
                    (rx.saturating_sub(prx) as f64 / 1024.0 / dt).max(0.0),
                    (tx.saturating_sub(ptx) as f64 / 1024.0 / dt).max(0.0),
                ),
                _ => (0.0, 0.0),
            };
            let is_hog = up_kbs > 350.0 || down_kbs > 2500.0;
            procs.push(ProcSample {
                pid,
                protected: PROTECTED.contains(&name.as_str()),
                paused: self.paused_pids.contains(&pid),
                name,
                down_kbs,
                up_kbs,
                total_mb: (rx + tx) as f64 / (1024.0 * 1024.0),
                is_hog,
            });
        }

        self.prev_totals = new_totals;
        self.prev_ts = Some(now);

        procs.sort_by(|a, b| {
            let ka = a.up_kbs * 2.0 + a.down_kbs;
            let kb = b.up_kbs * 2.0 + b.down_kbs;
            kb.partial_cmp(&ka)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    b.total_mb
                        .partial_cmp(&a.total_mb)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
        procs.truncate(15);
        self.last = procs;
        &self.last
    }

    /// Toggle SIGSTOP/SIGCONT on a process. Protected processes are refused.
    pub fn toggle_pause(&mut self, pid: i32, name: &str) -> String {
        if PROTECTED.contains(&name) {
            return format!("Protected process '{name}' cannot be paused.");
        }
        if self.paused_pids.contains(&pid) {
            match send_signal(pid, libc_sigcont()) {
                Ok(()) => {
                    self.paused_pids.remove(&pid);
                    format!("Resumed {name} (PID {pid}).")
                }
                Err(e) => format!("Failed to resume PID {pid}: {e}"),
            }
        } else {
            match send_signal(pid, libc_sigstop()) {
                Ok(()) => {
                    self.paused_pids.insert(pid);
                    format!("Paused bandwidth hog {name} (PID {pid}) via SIGSTOP.")
                }
                Err(e) => format!("Failed to pause PID {pid}: {e}"),
            }
        }
    }

    /// Resume every frozen process. Called on quit AND from the signal handler.
    pub fn resume_all(&mut self) -> usize {
        let mut count = 0;
        for pid in std::mem::take(&mut self.paused_pids) {
            if send_signal(pid, libc_sigcont()).is_ok() {
                count += 1;
            }
        }
        count
    }
}

fn libc_sigstop() -> i32 {
    17
}
fn libc_sigcont() -> i32 {
    19
}

/// Send a signal via the `kill` command (no libc dependency needed).
fn send_signal(pid: i32, sig: i32) -> Result<(), String> {
    let status = Command::new("kill")
        .arg(format!("-{sig}"))
        .arg(pid.to_string())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill exited with {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_pid_and_counters() {
        let csv = ",bytes_in,bytes_out,\nnsurlsessiond.482,500000,120000,\nzen.912,250000,40000,\n";
        let rows = ProcMonitor::parse(csv);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], ("nsurlsessiond".to_string(), 482, 500000, 120000));
        assert_eq!(rows[1], ("zen".to_string(), 912, 250000, 40000));
    }

    #[test]
    fn protected_process_is_refused() {
        let mut m = ProcMonitor::new();
        let msg = m.toggle_pause(1, "launchd");
        assert!(msg.contains("cannot be paused"));
        assert!(m.paused_pids.is_empty());
    }

    #[test]
    fn name_with_dots_splits_on_last_dot() {
        let csv = ",bytes_in,bytes_out,\ncom.apple.WebKit.Networking.1234,10,20,\n";
        let rows = ProcMonitor::parse(csv);
        assert_eq!(rows[0].0, "com.apple.WebKit.Networking");
        assert_eq!(rows[0].1, 1234);
    }
}
