# ⚡ WIFI TUI // Amber CRT Network Cockpit (Rust)

A native **Rust** rebuild of the macOS Wi-Fi cockpit: live RF telemetry, a 4-hop
bufferbloat isolation matrix, a spectrum collision radar, and a non-destructive
per-process bandwidth firewall — in an amber CRT terminal aesthetic.

This is a ground-up rewrite of the original Python project. The motivation was
reliability: the Python version drove everything through shell-outs (`system_profiler`,
`ping`, `networksetup`) and an untyped `dict` passed between threads, which was the
source of recurring bugs. The Rust rebuild reads the OS directly and is strongly typed.

## Why Rust fixed the recurring bugs

| Problem in the Python version | Fix in the Rust rebuild |
|---|---|
| RF data came from `system_profiler SPAirPortDataType` — **~6 s** per call, blocking the UI | Reads **CoreWLAN** directly via `objc2-core-wlan` — **~3 ms** per call (measured) |
| Each ping shelled out to `/sbin/ping` every 1.5 s on every hop | Unprivileged ICMP over `SOCK_DGRAM` — no subprocess, no root |
| Gateway / DNS / MTU / byte counters parsed from `netstat`/`ifconfig`/`scutil` text | Read structurally via the `netdev` crate — no text parsing |
| Telemetry was a `Dict[str, Any]`; reading a key no writer set failed silently | A typed `Telemetry` struct; the compiler rejects those mistakes |
| Frozen processes were only resumed on Ctrl-C; SIGTERM/SIGHUP leaked suspended procs | `SIGINT`/`SIGTERM`/`SIGHUP` all exit through one teardown that calls `resume_all()` |
| `unsafe`-equivalent FFI scattered implicitly | `unsafe` is **denied crate-wide** and re-allowed only in the documented CoreWLAN module |

## Features

- **Tab `[1]` Cockpit** — SSID, band, channel, signal/noise/SNR, link rate, MCS,
  live RTT/jitter/throughput sparklines, and a 4-hop fault-isolation matrix
  (`Mac → Gateway → ISP → Google Edge`) with bufferbloat-delta diagnosis.
- **Tab `[2]` Spectrum** — 5GHz UNII/DFS block occupancy, co-channel collision
  detection, a clean-channel recommendation, and a de-redacted neighbour list.
- **Tab `[3]` Processes** — live per-process bandwidth table; `[Space]` freezes
  (`SIGSTOP`) or resumes (`SIGCONT`) a bandwidth hog. System/terminal processes
  are protected and can never be frozen.

## Usage

```bash
# Interactive amber CRT TUI
wifi-tui

# One-shot CLI
wifi-tui --status      # print RF telemetry, 4-hop matrix & health score
wifi-tui --json        # full telemetry as JSON
wifi-tui --interface en0
```

Keys: `[Tab]` switch view · `[1/2/3]` jump to a tab · `[K]` toggle CAM pacer ·
`[j/k]` select process · `[Space]` pause/resume · `[U]` resume all · `[R]` rescan · `[Q]` quit.

## Build

```bash
cargo build --release
./target/release/wifi-tui
```

Requires macOS (CoreWLAN) and a Rust toolchain. No root required.

### A note on SSID names

On macOS 14+, the Wi-Fi SSID is masked unless the process has Location permission.
When masked, the cockpit infers the name from your saved preferred networks (macOS
keeps the connected network first) and labels it `(inferred)`. Grant Location
permission to the terminal to show the confirmed name.

## Development

```bash
cargo test            # unit tests (hop math, health score, spectrum, procmon parsing)
cargo clippy --all-targets
cargo fmt
```

## License

MIT
