//! Autonomous Protection Shield & Anti-Orphan Watchdog.
//!
//! Provides 4 layers of automated hardware and process safety:
//! 1. Dead-Man's Switch: Monitors parent PID and controlling TTY liveness.
//!    If the process becomes orphaned (reparented to launchd / PID 1) or stdin
//!    is revoked, it immediately unfreezes all processes and self-terminates.
//! 2. CPU Runaway Circuit Breaker: Samples process `rusage` every 1.5s. If CPU
//!    consumption exceeds 85% for 3 consecutive intervals while idle (no active
//!    speedtest/workload), it trips and aborts to prevent laptop thermal throttling.
//! 3. Single-Instance Advisory Lock: `/tmp/wifi-tui.lock` with auto-reaping of
//!    dead or orphaned instances from earlier crashes.
//! 4. System Rescue Engine: Scans for any ghost instances and resumes all trapped
//!    processes across the operating system (`wifi-tui --rescue`).

#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::core::procmon;

const LOCK_FILE_PATH: &str = "/tmp/wifi-tui.lock";
const LOG_FILE_PATH: &str = "/tmp/wifi-tui-shield.log";

/// Append a timestamped entry to the shield log.
pub fn log_shield(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE_PATH)
    {
        let ts = chrono_or_fallback();
        let _ = writeln!(f, "[{ts}] [SHIELD] {msg}");
    }
}

fn chrono_or_fallback() -> String {
    let now = Instant::now();
    format!("{now:?}")
}

// ============================================================================
// 1. DEAD-MAN'S SWITCH
// ============================================================================

/// Background monitor that checks parent process ID and TTY connection health.
/// If parent becomes `launchd` (PID 1) or the terminal hangs up, cleanly terminates.
pub struct DeadManSwitch {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl DeadManSwitch {
    pub fn start() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = Arc::clone(&running);

        let handle = std::thread::Builder::new()
            .name("wifi-deadman".into())
            .spawn(move || {
                while r.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(600));

                    if !r.load(Ordering::Relaxed) {
                        break;
                    }

                    // Check 1: Parent Process ID.
                    // If an interactive TUI's parent PID becomes 1 (launchd), the terminal emulator
                    // (Terminal, iTerm2, Alacritty, VS Code) died or closed its tab.
                    let ppid = unsafe { libc::getppid() };
                    if ppid == 1 {
                        log_shield("Parent PID became 1 (launchd). Terminal closed. Cleanly terminating.");
                        procmon::resume_all_global();
                        // Exit cleanly
                        std::process::exit(0);
                    }

                    // Check 2: STDIN file descriptor status (POLLHUP / POLLERR / POLLNVAL).
                    let mut pfd = libc::pollfd {
                        fd: libc::STDIN_FILENO,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let ret = unsafe { libc::poll(&mut pfd, 1, 0) };
                    if ret > 0 && (pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)) != 0 {
                        log_shield("Controlling terminal poll hangup detected (POLLHUP/POLLERR). Terminating.");
                        procmon::resume_all_global();
                        std::process::exit(0);
                    }
                }
            })
            .ok();

        DeadManSwitch { running, handle }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for DeadManSwitch {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// 2. CPU RUNAWAY CIRCUIT BREAKER
// ============================================================================

/// Monitors own process CPU usage. If usage exceeds 85% of a core for 3 consecutive
/// evaluation periods without an active workload (e.g. speedtest), the circuit
/// breaker trips, unfreezes all processes, and aborts to protect laptop thermals.
pub struct CpuCircuitBreaker {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl CpuCircuitBreaker {
    pub fn start(busy_flag: Arc<AtomicBool>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = Arc::clone(&running);

        let handle = std::thread::Builder::new()
            .name("wifi-circuit-breaker".into())
            .spawn(move || {
                let mut prev_ts = Instant::now();
                let mut prev_cpu = get_process_cpu_time_secs();
                let mut consecutive_spikes = 0u32;

                while r.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(1500));

                    if !r.load(Ordering::Relaxed) {
                        break;
                    }

                    let now = Instant::now();
                    let current_cpu = get_process_cpu_time_secs();
                    let wall_dt = (now - prev_ts).as_secs_f64().max(0.1);
                    let cpu_dt = (current_cpu - prev_cpu).max(0.0);

                    let cpu_ratio = cpu_dt / wall_dt; // 1.0 = 100% of 1 core
                    prev_ts = now;
                    prev_cpu = current_cpu;

                    // If speedtest or heavy action is running, legitimate CPU is expected.
                    if busy_flag.load(Ordering::Relaxed) {
                        consecutive_spikes = 0;
                        continue;
                    }

                    if cpu_ratio > 0.85 {
                        consecutive_spikes += 1;
                        log_shield(&format!(
                            "High CPU detected: {:.1}% (strike {}/3)",
                            cpu_ratio * 100.0,
                            consecutive_spikes
                        ));

                        if consecutive_spikes >= 3 {
                            let msg = format!(
                                "CIRCUIT BREAKER TRIPPED: CPU usage sustained at {:.1}% while idle. Terminating to protect CPU from throttling.",
                                cpu_ratio * 100.0
                            );
                            log_shield(&msg);
                            eprintln!("\n[!] wifi-tui: {msg}\n");
                            procmon::resume_all_global();
                            std::process::exit(1);
                        }
                    } else {
                        consecutive_spikes = 0;
                    }
                }
            })
            .ok();

        CpuCircuitBreaker { running, handle }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for CpuCircuitBreaker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Query process CPU time (user + system) in seconds via getrusage.
fn get_process_cpu_time_secs() -> f64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if res == 0 {
        let u = usage.ru_utime.tv_sec as f64 + usage.ru_utime.tv_usec as f64 / 1_000_000.0;
        let s = usage.ru_stime.tv_sec as f64 + usage.ru_stime.tv_usec as f64 / 1_000_000.0;
        u + s
    } else {
        0.0
    }
}

// ============================================================================
// 3. SINGLE-INSTANCE LOCK & ORPHAN REAPER
// ============================================================================

pub struct InstanceLock {
    file: File,
    path: PathBuf,
}

#[derive(Debug)]
pub enum LockError {
    AlreadyRunning(i32),
    Io(std::io::Error),
}

impl InstanceLock {
    /// Attempt to acquire the single-instance advisory lock.
    /// If another instance exists:
    /// - If it is an orphaned ghost (parent=1 or non-existent), it automatically reaps it!
    /// - If it is an active user instance, returns `LockError::AlreadyRunning(pid)`.
    pub fn acquire() -> Result<Self, LockError> {
        let path = PathBuf::from(LOCK_FILE_PATH);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(LockError::Io)?;

        let fd = file.as_raw_fd();
        let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

        if res == 0 {
            // Lock acquired! Record our PID
            Self::write_pid(&mut file)?;
            return Ok(InstanceLock { file, path });
        }

        // Lock collision: read existing PID
        let existing_pid = Self::read_pid(&mut file);

        if let Some(pid) = existing_pid {
            // Check if existing PID is an orphaned runaway or dead process
            if Self::is_orphan_or_dead(pid) {
                log_shield(&format!("Detected orphaned wifi-tui process (PID {pid}). Reaping..."));
                Self::kill_process(pid);
                std::thread::sleep(Duration::from_millis(100));

                // Try locking again now that orphan is reaped
                let retry = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if retry == 0 {
                    Self::write_pid(&mut file)?;
                    log_shield(&format!("Successfully reaped orphan (PID {pid}) and acquired lock."));
                    return Ok(InstanceLock { file, path });
                }
            }
            return Err(LockError::AlreadyRunning(pid));
        }

        Err(LockError::AlreadyRunning(0))
    }

    fn write_pid(file: &mut File) -> Result<(), LockError> {
        let pid = unsafe { libc::getpid() };
        file.set_len(0).map_err(LockError::Io)?;
        file.seek(SeekFrom::Start(0)).map_err(LockError::Io)?;
        writeln!(file, "{}", pid).map_err(LockError::Io)?;
        file.flush().map_err(LockError::Io)?;
        Ok(())
    }

    fn read_pid(file: &mut File) -> Option<i32> {
        let _ = file.seek(SeekFrom::Start(0));
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_ok() {
            buf.lines().next()?.trim().parse::<i32>().ok()
        } else {
            None
        }
    }

    /// Check if a PID is either dead or an orphan (parent PID == 1 or has no TTY).
    fn is_orphan_or_dead(pid: i32) -> bool {
        // First test if process is alive: kill(pid, 0)
        let alive = unsafe { libc::kill(pid, 0) == 0 };
        if !alive {
            return true;
        }

        // Check if process has PPID == 1 or no controlling TTY via `ps`
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "ppid=,tty="])
            .output()
            .ok();

        if let Some(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.is_empty() {
                return true;
            }
            let ppid = parts[0].parse::<i32>().unwrap_or(0);
            let tty = parts.get(1).copied().unwrap_or("??");

            if ppid == 1 || tty == "??" || tty == "?" {
                return true;
            }
        }
        false
    }

    pub fn kill_process(pid: i32) {
        unsafe {
            let _ = libc::kill(pid, libc::SIGTERM);
            std::thread::sleep(Duration::from_millis(50));
            // Escalate to SIGKILL if still alive
            if libc::kill(pid, 0) == 0 {
                let _ = libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        unsafe {
            let _ = libc::flock(fd, libc::LOCK_UN);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

// ============================================================================
// 4. SYSTEM RESCUE ENGINE
// ============================================================================

#[derive(Debug, Default, Clone)]
pub struct RescueReport {
    pub orphans_killed: Vec<i32>,
    pub frozen_procs_resumed: usize,
    pub lock_cleaned: bool,
    pub diagnosis: String,
}

/// One-shot automated scan and rescue for the entire macOS host.
/// 1. Finds and kills any rogue or orphaned `wifi-tui` processes (PPID 1, TTY ??).
/// 2. Scans for any processes in `T` (stopped) state and wakes them with `SIGCONT`.
/// 3. Resumes all paused processes recorded in global registry.
/// 4. Cleans up stale lock files.
pub fn rescue_system() -> RescueReport {
    let mut report = RescueReport::default();
    let current_pid = unsafe { libc::getpid() };

    // 1. Scan process table for wifi-tui instances
    let out = std::process::Command::new("ps")
        .args(["-e", "-o", "pid=,ppid=,tty=,comm="])
        .output()
        .ok();

    if let Some(o) = out {
        let stdout = String::from_utf8_lossy(&o.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let Ok(pid) = parts[0].parse::<i32>() else {
                continue;
            };
            if pid == current_pid {
                continue;
            }
            let ppid = parts[1].parse::<i32>().unwrap_or(0);
            let tty = parts[2];
            let comm = parts[3];

            if comm.contains("wifi-tui") {
                // If it's reparented to launchd or has no TTY, it's an orphan!
                if ppid == 1 || tty == "??" || tty == "?" {
                    InstanceLock::kill_process(pid);
                    report.orphans_killed.push(pid);
                    log_shield(&format!("Rescue: killed orphaned wifi-tui PID {pid}"));
                }
            }
        }
    }

    // 2. Resume all globally recorded paused PIDs
    let resumed_global = procmon::resume_all_global();
    report.frozen_procs_resumed += resumed_global;

    // 3. Scan for any user processes in 'T' (stopped) state and resume them
    let stopped_out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,stat=,comm="])
        .output()
        .ok();

    if let Some(so) = stopped_out {
        let s = String::from_utf8_lossy(&so.stdout);
        for line in s.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let Ok(pid) = parts[0].parse::<i32>() else {
                continue;
            };
            let stat = parts[1];
            let comm = parts[2];

            // If state contains 'T' (stopped) and it's a typical user application
            if stat.contains('T') && !procmon::PROTECTED.contains(&comm) {
                unsafe {
                    if libc::kill(pid, libc::SIGCONT) == 0 {
                        report.frozen_procs_resumed += 1;
                        log_shield(&format!("Rescue: sent SIGCONT to stopped process {comm} (PID {pid})"));
                    }
                }
            }
        }
    }

    // 4. Clean lock file if stale
    let lock_path = Path::new(LOCK_FILE_PATH);
    if lock_path.exists() {
        if let Ok(mut f) = File::open(lock_path) {
            if let Some(pid) = InstanceLock::read_pid(&mut f) {
                if pid != current_pid && InstanceLock::is_orphan_or_dead(pid) {
                    let _ = std::fs::remove_file(lock_path);
                    report.lock_cleaned = true;
                }
            }
        }
    }

    report.diagnosis = if report.orphans_killed.is_empty() && report.frozen_procs_resumed == 0 {
        "[OK] System clean: No runaway orphans or frozen processes detected.".into()
    } else {
        format!(
            "[REPAIRED] Terminated {} runaway orphan(s) and resumed {} suspended process(es).",
            report.orphans_killed.len(),
            report.frozen_procs_resumed
        )
    };

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_cpu_time_query() {
        let cpu_time = get_process_cpu_time_secs();
        assert!(cpu_time >= 0.0, "CPU time must be non-negative");
    }

    #[test]
    fn test_deadman_start_and_stop() {
        let mut dm = DeadManSwitch::start();
        std::thread::sleep(Duration::from_millis(100));
        dm.stop();
    }

    #[test]
    fn test_circuit_breaker_start_and_stop() {
        let busy = Arc::new(AtomicBool::new(false));
        let mut cb = CpuCircuitBreaker::start(busy);
        std::thread::sleep(Duration::from_millis(100));
        cb.stop();
    }

    #[test]
    fn test_rescue_system_runs_cleanly() {
        let rep = rescue_system();
        assert!(!rep.diagnosis.is_empty(), "Rescue diagnosis should not be empty");
    }
}
