# ⚡ WIFI TUI // Amber CRT Network Cockpit (Rust)

A native **Rust** rebuild of the macOS Wi-Fi cockpit: live RF telemetry, a 4-hop
bufferbloat isolation matrix, spectrum collision radar, a non-destructive
per-process bandwidth firewall, a WAN/YouTube auto-doctor, DNS racer, kernel
anti-lag tuner, security guard, and a 4-tier self-healing orchestrator — in an
amber CRT terminal aesthetic.

This is a complete, ground-up port of the original Python project (≈11k lines,
45 modules). Every engine, preset, keybinding, and the full 5-tab UI were
reimplemented in Rust. The motivation was reliability: the Python version drove
everything through shell-outs and an untyped `dict` passed between threads, which
was the source of recurring bugs.

## Why the Rust rebuild is more reliable

| Problem in the Python version | Fix in the Rust rebuild |
|---|---|
| RF data came from `system_profiler SPAirPortDataType` — **~6 s** per call, blocking the UI | Reads **CoreWLAN** directly via `objc2-core-wlan` — **~3 ms** per call (measured) |
| Each ping shelled out to `/sbin/ping` on every hop every 1.5 s | Unprivileged ICMP over `SOCK_DGRAM` — no subprocess, no root |
| Gateway / DNS / MTU / byte counters parsed from `netstat`/`ifconfig`/`scutil` text | Read structurally via the `netdev` crate |
| Telemetry was a `Dict[str, Any]`; reading a key no writer set failed silently | A typed `Telemetry` struct; the compiler rejects those mistakes |
| Frozen processes were only resumed on Ctrl-C; SIGTERM/SIGHUP leaked suspended procs | `SIGINT`/`SIGTERM`/`SIGHUP` all exit through one teardown that calls `resume_all()` |
| Two orchestrator calls referenced methods that didn't exist (latent `AttributeError`) | Tier 4 maps to the real UPnP `ForceTermination`; the compiler guarantees the call resolves |
| FFI-equivalent unsafety scattered implicitly | `unsafe` is **denied crate-wide**, re-allowed only in the documented CoreWLAN module |

## Features (5 tabs)

- **Tab `[1]` Cockpit** — SSID, band, channel, signal/noise/SNR, link rate, MCS,
  live RTT/jitter/throughput sparklines, 4-hop fault-isolation matrix
  (`Mac → Gateway → ISP → Google Edge`) with bufferbloat-delta diagnosis, and a
  live RF/workload interpretation panel with LoL/YouTube/AI readiness badges.
- **Tab `[2]` Spectrum** — 5GHz UNII/DFS block occupancy, co-channel collision
  detection, clean-channel recommendation, de-redacted neighbour list, and the
  acoustic sweet-spot locator (`[S]`).
- **Tab `[3]` Processes** — live per-process bandwidth firewall; `[Space]` freezes
  (`SIGSTOP`) / resumes (`SIGCONT`) a hog, `[B]` applies QoS shaping, `[N]` sweeps
  the LAN. System/terminal processes are protected.
- **Tab `[4]` Tuning Lab** — 12 one-key tuning modules (WMM VIP, CAM pacer, TCP
  no-delay, 32MB BDP buffers, ECS DNS doctor, IPv6 guard, QUIC fast-path,
  zero-lag, zombie-socket reaper, 5GHz lock, MTU clamp, root lock), the preset
  brain with 7 mission presets, and the WAN/YouTube GGC doctor + DNS racer panel.
- **Tab `[5]` SpeedTest** — multi-stream download/upload saturation with loaded-ping
  bufferbloat grading, Apple-native `networkQuality` RPM benchmark (`[N]`), and a
  workload readiness matrix.

## One-shot CLI

```bash
wifi-tui --status        # RF telemetry, 4-hop matrix & health score
wifi-tui --json          # full telemetry as JSON
wifi-tui --doctor        # WAN & YouTube auto-doctor
wifi-tui --boost         # god-mode boost pipeline
wifi-tui --orchestrate [--tier N]   # 4-tier self-healing orchestrator
wifi-tui --debloat       # 6-stage Hop 2 anti-bloat repair
wifi-tui --pmtu          # Path MTU discovery + MSS clamp
wifi-tui --rf-inference  # non-Wi-Fi RF interference diagnosis
wifi-tui --security-guard           # evil-twin / ARP / DNS audit
wifi-tui --reboot-gateway [--confirm]  # UPnP WAN status / warm reboot
wifi-tui --speedtest     # multi-stream speed + bufferbloat
wifi-tui --sweetspot     # real-time acoustic sweet-spot locator
wifi-tui --export        # write ISP diagnostic markdown ticket
wifi-tui --preset gaming # activate a mission profile
wifi-tui --watchdog      # foreground link-anomaly watchdog loop
wifi-tui --daemon        # foreground autonomous self-healing loop
wifi-tui --daemon-install / --daemon-uninstall / --daemon-status
wifi-tui --interface en0
```

Aliases matching the Python CLI are accepted: `--auto-heal`, `--check-mtu`,
`--detect-interference`, `--audit-security`, `--gateway-status`, `--radar-audio`,
`--repair-hop2`.

## Keybindings (interactive)

`[1..5]` tabs · `[Tab]` cycle · `[D]` doctor · `[G]` boost · `[L]` zero-lag ·
`[K]` CAM pacer · `[W]` WMM · `[X]` TCP no-delay · `[O]` hop-2 repair · `[6]` IPv6
guard · `[Y]` QUIC fast-path · `[T]` MTU clamp · `[F]` clear macOS `[!]` · `[P]`
cycle preset · `[A]` auto-adapt · `[E]` Wi-Fi password editor · `[C]` captive/gateway
probe · `[R]` roam/rescan · `[M]`/`[`/`]` visual deck · `[J]` export ISP ticket ·
`[S]` root auth (or sweet-spot on Tab 2) · `[Z]` zombie reaper · `[Q]` quit.

Tab 3: `[j/k]` select · `[Space]` pause · `[B]` shape · `[N]` LAN sweep · `[U]` resume all.
Tab 5: `[Space]` full suite · `[V]` 5s burst · `[N]` Apple RPM.

## Build

```bash
cargo build --release
./target/release/wifi-tui
```

Requires macOS (CoreWLAN) and a Rust toolchain. No root required for monitoring;
the kernel-tuning actions use `sudo -n` and fall back gracefully when passwordless
sudo isn't configured (the UI shows "press [S] to persist" in that case).

### A note on SSID names

On macOS 14+, the Wi-Fi SSID is masked unless the process has Location permission.
When masked, the cockpit infers the name from your saved preferred networks and
labels it `(inferred)`. Grant Location permission to the terminal for the confirmed name.

## Development

```bash
cargo test            # hop math, health score, spectrum, procmon parsing
cargo clippy --all-targets
cargo fmt
```

## License

MIT
