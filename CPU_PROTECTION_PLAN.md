================================================================================
  WIFI-TUI  —  ZERO-CPU-THROTTLE & ORPHAN SHIELD ARCHITECTURE
================================================================================

MY OPINION (read first)
-----------------------
What happened to your laptop is the worst kind of bug for a systems utility: an
app that is supposed to optimize your network ended up pegging your CPU cores to
100% in the background, surviving terminal closure, and forcing a reboot.

The core reason was a "cascade of omissions":
  1) `signal_hook` caught SIGHUP (terminal closed) and kept the process alive.
  2) Upstream `crossterm` had an infinite `loop` on `read() == 0` (EOF).
  3) The process became reparented to `launchd` (PID 1) with no screen or user.
  4) There was no dead-man's switch, no parent-PID watchdog, no CPU circuit breaker,
     and no single-instance lock to prevent stacking multiple zombies.

We must build a multi-layered defense system so that **it is physically impossible
for wifi-tui to ever throttle CPU or stay alive as an orphaned ghost process again**.

--------------------------------------------------------------------------------
THE 4-PILLAR SHIELD ARCHITECTURE
--------------------------------------------------------------------------------

  ┌────────────────────────────────────────────────────────────────────────┐
  │ 1. RUNAWAY CPU CIRCUIT BREAKER (Self-Policing Watchdog Thread)        │
  │    • Low-overhead background thread checks process rusage every 2s     │
  │    • If CPU time delta > 90% over 2s while idle -> TRIP BREAKER        │
  │    • Automatically unfreezes all processes (SIGCONT) and exits (0)     │
  └──────────────────────────────────┬─────────────────────────────────────┘
                                     │
  ┌──────────────────────────────────▼─────────────────────────────────────┐
  │ 2. DEAD-MAN'S SWITCH (Parent Liveness & TTY Severance Guard)          │
  │    • Checks `getppid()`: if parent becomes `1` (launchd) in TUI mode   │
  │      -> immediately self-terminates cleanly in <10ms                   │
  │    • Checks `tcgetpgrp` & `libc::poll` for `POLLHUP | POLLERR`         │
  │    • If controlling terminal closes without signal -> graceful exit    │
  └──────────────────────────────────┬─────────────────────────────────────┘
                                     │
  ┌──────────────────────────────────▼─────────────────────────────────────┐
  │ 3. SINGLE-INSTANCE LOCK & ORPHAN REAPER (/tmp/wifi-tui.lock)           │
  │    • Advisory flock on startup                                         │
  │    • If another instance exists: inspect its PID                       │
  │      - If existing instance is an orphan (PPID 1) -> KILL & REAP IT    │
  │      - If legitimate user instance -> warn user & exit cleanly         │
  └──────────────────────────────────┬─────────────────────────────────────┘
                                     │
  ┌──────────────────────────────────▼─────────────────────────────────────┐
  │ 4. RESCUE & AUTO-CLEANER CLI TOOL (`wifi-tui --rescue`)               │
  │    • Scans process table for orphaned `wifi-tui` PIDs (PPID 1, TTY ??) │
  │    • Resumes any trapped SIGSTOP user processes across the entire OS   │
  │    • Cleans stale locks and prints clean recovery status               │
  └────────────────────────────────────────────────────────────────────────┘

--------------------------------------------------------------------------------
DETAILED FEATURE SPECIFICATIONS
--------------------------------------------------------------------------------

### PILLAR 1: DEAD-MAN'S SWITCH (Parent & Terminal Heartbeat)
* **Location**: New module `src/core/watchdog/deadman.rs`
* **Trigger Cadence**: Every 1.0 second on a dedicated low-priority thread.
* **Checks**:
  1. `getppid() == 1`: An interactive TUI must NEVER be a child of `launchd`. If
     the terminal emulator (Terminal.app, iTerm2, Alacritty, VS Code) is closed,
     macOS re-parents the child to PID 1. The moment `getppid() == 1` is observed,
     the dead-man's switch fires:
     - Log: `"[DEADMAN] Terminal parent died (reparented to launchd). Shutting down."`
     - Call `core::procmon::resume_all_global()` to guarantee zero frozen processes.
     - Execute `std::process::exit(0)`.
  2. `is_tty_hungup()`: Non-blocking poll on `STDIN_FILENO` checking for
     `POLLHUP | POLLERR | POLLNVAL`. If the terminal device has been revoked or
     disconnected, exit immediately.

### PILLAR 2: CPU RUNAWAY CIRCUIT BREAKER
* **Location**: Integrated into `src/core/watchdog/circuit_breaker.rs`
* **Mechanism**:
  - Sample CPU time using `libc::getrusage(libc::RUSAGE_SELF, ...)` every 2 seconds.
  - Calculate: `cpu_pct = (user_time_delta + sys_time_delta) / wall_time_delta`.
  - In normal TUI operation, `wifi-tui` runs at ~2.5% CPU (mostly sleeping on
    `kevent` waiting for user keystrokes or 1.5s refresh timer).
  - If `cpu_pct > 80%` continuously for 2 consecutive check cycles (4 seconds)
    without an active speedtest running:
    - **Circuit Breaker Trips**:
      - Emergency signal: unfreezes all frozen processes (`resume_all_global()`).
      - Restores terminal mode.
      - Terminates process with an emergency diagnostic log to `/tmp/wifi-tui-crash.log`.
    - Result: `wifi-tui` can NEVER peg a CPU core for more than 4 seconds under any
      circumstance, even if a third-party library has a zero-day spin bug.

### PILLAR 3: SINGLE-INSTANCE PROCESS LOCK & AUTO-REAP
* **Location**: `src/core/lock.rs`
* **File**: `/tmp/wifi-tui.lock` (contains current running PID + start timestamp)
* **Behavior**:
  - When starting up, acquire non-blocking advisory file lock (`flock`).
  - If lock acquisition fails:
    - Read PID from `/tmp/wifi-tui.lock`.
    - Inspect the existing PID:
      - If process is dead: take over lock.
      - If process is an orphaned ghost (`ppid == 1` or `tty == ??`):
        Send `SIGTERM`, wait 100ms, send `SIGKILL`, take over lock, unfreeze any
        leftover suspended processes.
      - If process is a legitimate interactive user session:
        Print friendly message:
        `"[!] wifi-tui is already running in another terminal (PID <pid>)."`
        Exit cleanly with code 0.

### PILLAR 4: ONE-SHOT RESCUE & CLEANER CLI COMMAND
* **Flag**: `wifi-tui --rescue` (or `--clean-orphans`)
* **Behavior**:
  1. Searches for any rogue `wifi-tui` processes across the entire machine.
  2. Kills any instances running without an active TTY (`??`) or attached to PID 1.
  3. Sends `SIGCONT` to all processes in `nettop` or system process table that have
     state `T` (stopped/frozen) that might have been paused by an older build.
  4. Removes stale lock files (`/tmp/wifi-tui.lock`).
  5. Outputs a clean status report:
     ```text
     // WIFI COCKPIT // RESCUE & SYSTEM CLEANER //
     ====================================================================
       Orphaned Processes Terminated : 2 (PID 1816, PID 29625)
       Frozen Bandwidth Hogs Resumed : 0
       Advisory Lock Status          : Cleaned (/tmp/wifi-tui.lock)
       System Status                 : [OK] Wi-Fi Cockpit baseline restored.
     ```

--------------------------------------------------------------------------------
IMPLEMENTATION STEPS
--------------------------------------------------------------------------------

  1. Create `src/core/shield.rs`:
     - Implement `DeadManSwitch`: background thread monitoring `getppid() == 1`
       and `POLLHUP` on stdin.
     - Implement `CpuCircuitBreaker`: monitoring `getrusage` delta every 2s.
     - Implement `SingleInstanceLock`: advisory flock with zombie detection.
     - Implement `rescue_system()`: scanner and cleaner function for `--rescue`.

  2. Integrate into `src/main.rs`:
     - Add `--rescue` / `--clean-orphans` CLI flag to `Cli` struct.
     - In `run_tui()`:
       - Acquire `SingleInstanceLock` before entering raw mode.
       - Start `DeadManSwitch` & `CpuCircuitBreaker` alongside worker.
       - Clean up lock on normal exit.

  3. Add Automated Regression Tests:
     - Test dead-man switch: verify process exits if parent dies.
     - Test single-instance lock: verify second instance detects first and doesn't stack.
     - Test circuit breaker: simulate simulated busy spin, verify auto-kill.
     - Test rescue command: verify clean report output.

--------------------------------------------------------------------------------
REVIEW & APPROVAL
--------------------------------------------------------------------------------
This plan gives you 4 independent fail-safes so neither a library bug nor an
abrupt terminal disconnect can ever throttle your machine again.
Review this file in Neovim:
  nvim CPU_PROTECTION_PLAN.md
================================================================================
