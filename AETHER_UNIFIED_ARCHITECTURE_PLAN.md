# AETHER v2.1 — Unified Zero-Redundancy Architecture & TUI Blueprint

> **Purpose**: Eliminate every duplicate engine, synchronize state across all 10 tabs and the Spotlight Action Palette (`[P]`), remove dead/synthetic stubs, and ensure **100% of Aether's modules work together as a single organism** across **macOS, Linux, and Windows 11**.

---

## 1. Executive Audit: What Changes & Why

Currently, Aether has **10 tabs** and **36 core modules**, but several features evolved separately and now either **do the same job in two places without sharing state** or **compute synthetic/dead data**.

| # | Current Redundancy / Disconnect | What Is Happening Right Now | How We Unify It (Zero Redundancy) |
| :--- | :--- | :--- | :--- |
| **1** | **`antilag.rs` + `scope.rs` CAM Pacer vs. `zero_jitter.rs`** | Tab 4 (`TUNE & FIX`) toggles `antilag.zero_lag_active` and `scope.cam_pacer_active`. Tab 9 (`GAMING`) toggles `ZeroJitterEngine` (Tier 1/2/3). They don't share state, and Game Mode actually *blocks* Tab 4's Zero-Lag toggle. | **Make `ZeroJitterEngine` the single source of truth** for airspace muting, BSSID lock, CAM pacing, and 50Hz gateway keep-alive. Toggling `"Zero-Lag Mode"` or `"CAM Pacer"` on Tab 4 or `[P]` directly drives `ZeroJitterEngine` Tier 1 & Tier 2 and updates both Tab 4 and Tab 9 live. |
| **2** | **Duplicate LAN Map in Tab 3 (`Processes`) vs. Tab 8 (`LAN Radar`)** | Tab 3 (`views/processes.rs`) squeezes a cramped 6-line `"LAN DEVICE MAP"` box into the bottom-center column, duplicating Tab 8 (`views/lan_radar.rs`). | **Remove the mini LAN box from Tab 3**. Expand Tab 3's `SELECTED PROCESS` inspector to full width (showing live process bandwidth bars, PID state, and quick-action hints) while Tab 8 (`LAN Radar`) owns LAN discovery 100%. |
| **3** | **Split DNS Engines (`dns_engine.rs` vs. `privacy_fortress.rs`)** | Racing DNS in Tab 4 (`WAN Doctor` / `Boost`) uses `dns_engine.rs`, while Tab 0 (`Privacy Fortress`) uses its own `privacy_fortress.rs` UDP racer. Neither updates the other. | **Unify DNS state**: When `PrivacyFortress` races resolvers on Tab 0 (`[R]`) or applies a resolver (`[Enter]`), it automatically syncs `wan_snap.ecs_dns_optimal` and `dns_results` on Tab 4 (`TUNE & FIX`)—and vice versa. |
| **4** | **Synthetic Route Hops in Tab 9 (`Gaming Radar`) vs. Real Hops in `scope.rs`** | Tab 9 (`gaming_radar.rs`) fabricates Hop 2/3/4 RTTs by multiplying `best_ping * 0.35`, even though `scope.rs` + `hops.rs` already probe real Hop 1–4 ICMP RTT, jitter, and loss every tick! | **Feed real `scope_snap.hops` into Tab 9 (`Gaming Radar`)**: Hop 1 (Wi-Fi Router), Hop 2 (ISP Aggregation), Hop 3 (Core Transit), plus Hop 4 (Selected Regional Game Cluster live TCP RTT). Zero fake math. |
| **5** | **Three Separate MTU Clamps (`KernelTuner 1492`, `Tier 2 1452`, `pmtu.rs`)** | Tab 4 toggles `1492/1500`, Tier 2 reports `1452`, and `pmtu.rs` has a binary-search Path MTU discovery engine only callable via CLI (`--pmtu`). | **Unify MTU management**: Connect `pmtu::discover_pmtu()` to Tab 4's `"Smart PMTU Discovery & Clamp"` and `ZeroJitterEngine` Tier 2 so one atomic MTU snapshot is shared and cleanly restored on exit. |
| **6** | **Dead Per-Frame Computation (`deck_lines`) & Placebo `[B]` Key** | `app.rs::build_render()` runs `widgets::deck_body()` on every frame even though `rs.deck_lines` is never rendered anywhere. Pressing `[B]` outside Tab 3 prints a placebo string. | **Delete `deck_lines` from `RenderState`** (saving CPU cycles every frame) and bind `[B]` outside Tab 3 to trigger `Boost` or remove the dead branch. |
| **7** | **CLI-Only Security Guard (`security_guard.rs`) Not Shown in Tab 8 (`LAN Radar`)** | `security_guard.rs` detects Evil-Twin BSSID spoofing and Gateway ARP poisoning, but only runs via `--security-guard` CLI flag. | **Embed `SecurityGuard` live telemetry into Tab 8 (`LAN Radar`) & Tab 0 (`Privacy Fortress`)** so ARP poisoning and Evil-Twin BSSID alerts show live in the TUI. |

---

## 2. Unified System Architecture Diagram

All 10 tabs and the Spotlight Action Palette (`[P]`) read from a single immutable `RenderState` snapshot produced by **5 Unified Core Subsystems**—with zero duplicate state or conflicting OS calls:

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                           AETHER WORKER LOOP (src/app.rs)                               │
│                                                                                         │
│  ┌──────────────────────┐  ┌──────────────────────┐  ┌───────────────────────────────┐  │
│  │ 1. RF & Airspace     │  │ 2. 3-Tier Zero-Jitter│  │ 3. Multi-Hop & WAN Intelligence│  │
│  │ ──────────────────── │  │ ──────────────────── │  │ ───────────────────────────── │  │
│  │ • wifi_collector.rs  │  │ • zero_jitter.rs     │  │ • scope.rs + hops.rs (Real)   │  │
│  │ • spectrum.rs        │  │ • game_mode.rs       │  │ • wan_doctor.rs + pmtu.rs     │  │
│  │ • rf_inference.rs    │  │ • kernel_tuner.rs    │  │ • gaming_radar.rs (TCP Probe) │  │
│  │ • acoustic_radar.rs  │  │   (Single State!)    │  │ • speedtest.rs (Bloat Suite)  │  │
│  └──────────┬───────────┘  └──────────┬───────────┘  └───────────────┬───────────────┘  │
│             │                         │                              │                  │
│             ▼                         ▼                              ▼                  │
│  ┌────────────────────────────────────┴──────────────────────────────────────────────┐  │
│  │ 4. LAN, Firewall & Privacy Fortress       5. Silicon, RAM & Storage Maintenance   │  │
│  │ ───────────────────────────────────       ─────────────────────────────────────   │  │
│  │ • procmon.rs + traffic_shaper.rs          • device_info.rs (P/E Cores, GPU, Batt) │  │
│  │ • lan_scanner.rs + security_guard.rs      • maintenance.rs (RAM Purge, SSD, Apps) │  │
│  │ • privacy_fortress.rs + dns_engine.rs     • shield.rs (Dead-Man Rollback Journal) │  │
│  └────────────────────────────────────┬──────────────────────────────────────────────┘  │
│                                       │                                                 │
│                                       ▼                                                 │
│                         Immutable RenderState Snapshot (<1ms)                           │
└───────────────────────────────────────┬─────────────────────────────────────────────────┘
                                        │
        ┌───────┬───────┬───────┬───────┼───────┬───────┬───────┬───────┬───────┐
        ▼       ▼       ▼       ▼       ▼       ▼       ▼       ▼       ▼       ▼
      [1]     [2]     [3]     [4]     [5]     [6]     [7]     [8]     [9]     [0]
     OVER     AIR    PROCS   TUNE    SPEED   DEVICE  MAINT    LAN    GAMING  PRIVACY
```

---

## 3. How Every Page Works Together (10-Tab Responsibility Matrix)

Every tab has **one clear mission** and shares live state with its sibling tabs:

| Key | Tab Name | Exclusive Domain (What Only This Tab Does) | Shared State Connections |
| :-: | :--- | :--- | :--- |
| **`[1]`** | **`◉ OVER` (Overview)** | Executive 2×2 cockpit: RF Gauges, Multi-Mode Oscilloscope (`[M]`), Live Hop 1–4 Bars, and AI Cockpit Verdict. | Reads `scope_hops` (shared with Tab 9), `ZeroJitter` status badge, and `health_score`. |
| **`[2]`** | **`◈ AIR` (Spectrum)** | Physical RF layer: 5GHz/2.4GHz Channel Overlap Heatmap, Neighbor APs, Acoustic Sweet-Spot (`[S]`), and Non-Wi-Fi RF Interference (`rf_inference.rs`). | Feeds channel collision penalties into Tab 1's `HEALTH` score and Tab 4's recommendations. |
| **`[3]`** | **`▦ PROCS` (Processes)** | Per-Process Bandwidth Firewall (`SIGSTOP`/`SIGCONT` via `[Space]`, `[U]` Resume All), Top Talkers MB chart, and Kernel Socket Encryption Posture. | *(Duplicate LAN box removed!)* Shares socket counts (`SYN_SENT`/`CLOSE_WAIT`) with Tab 4 Zombie Reaper. |
| **`[4]`** | **`⚙ TUNE` (Tune & Fix)** | Interactive OS/Network Toggles & One-Shot Repairs (`WAN Doctor`, `God-Mode Boost`, `PMTU Discovery`, `WMM VIP`, `Zero-Lag`). | Toggling `"Zero-Lag Mode"` or `"CAM Pacer"` here directly toggles `ZeroJitterEngine` (synced with Tab 9). DNS health is synced with Tab 0. |
| **`[5]`** | **`⚡ SPEED` (Speedtest)** | Multi-Stream Download/Upload Throughput + Loaded Bufferbloat Latency Grade (`[Enter]` Full, `[V]` 5s Burst, `[N]` Apple RPM). | Publishes loaded bufferbloat delta into Tab 1 (`OVER`) and ISP Ticket (`[J]`). |
| **`[6]`** | **`❖ DEVICE` (Silicon)** | Cross-Platform Hardware Telemetry: Dynamic P/E-Core Utilization Bars, Thermal Pressure, Swap/Memory Compression, Battery Health, ASCII Mascot (`[C]`). | Reflects Tier 3 Hardcore RT P-Core & Thermal Governor lock state from Tab 9. |
| **`[7]`** | **`⛨ MAINT` (Maintenance)** | Safe System Hygiene: 3-Tier Safety Checklist, Inactive RAM Purge (`[R]`), SSD Cache & APFS Snapshot Cleaner (`[S]`), Protected App Uninstaller (`[U]`). | Strictly shields active browsers (`Zen`, `Arc`, `Safari`, etc.) and game processes (`game_mode.rs`). |
| **`[8]`** | **`📡 LAN` (LAN Radar)** | Local Subnet ARP/NDP Scanner, 150+ OUI Hardware Vendor Matrix, Intruder Trust (`[Space]`), TCP Port Scanner (`[P]`), + **Evil-Twin / ARP Guard (`security_guard.rs`)**. | Feeds Gateway RTT (`gateway_rtt_ms`) and ARP MAC verification directly into Tab 9 (`Gaming Radar`) and Tab 4. |
| **`[9]`** | **`🎮 GAMING` (Gaming Radar)** | Regional Esports Cluster Matrix (Valorant, LoL, CS2, OW2, Apex), **3-Tier Zero-Jitter Engine (`[Space]` T1+T2, `[H]` Tier 3 RT)**, 60s Jitter Sparkline, and **Real Hop 1–4 Telemetry**. | Consumes real Hop 1–3 ICMP metrics from `scope.rs` + Hop 4 live game server TCP RTT. |
| **`[0]`** | **`🛡 PRIVACY` (Fortress)** | Live Outbound Telemetry & Tracker Interceptor + **Unified UDP Port-53 DNS Resolver Benchmark & Switcher** (`[R]` Race, `[Enter]` Apply, `[D]` DHCP). | Racing or applying a DNS server on Tab 0 immediately updates Tab 4 (`WAN & DNS Health`) and Tab 1 (`DNS` badge). |

---

## 4. TUI Sketches of the Rebuilt / Unified Views

### A. Tab 3 (`▦ PROCS`) — Cleaned Up (Duplicate LAN Map Removed)

By removing the redundant `LAN DEVICE MAP` box from the bottom-center of Tab 3 (since Tab 8 `LAN Radar` is dedicated to LAN discovery), the `SELECTED PROCESS INSPECTOR` gets the full width of the left column, showing live process bandwidth bars, socket state, and direct controls without text truncation:

```
┌─ ▦ PROCS ────────────────────────────────────────────────────────────────────────────────────────┐
│ ┌─ ACTIVE PROCS ────┐ ┌─ TOP HOG ─────────┐ ┌─ TOTAL UP ────────┐ ┌─ FROZEN ──────────┐          │
│ │ 18 procs          │ │ cloudd            │ │ 412 KB/s          │ │ 0 procs           │          │
│ └───────────────────┘ └───────────────────┘ └───────────────────┘ └───────────────────┘          │
│ ┌─ PER-PROCESS BANDWIDTH FIREWALL ([Space] Pause │ [U] Resume All) ──┐ ┌─ LIVE CONNECTIONS ────┐ │
│ │   PID      PROCESS              ▼ DOWN KB/s   ▲ UP KB/s   SESS MB  │ │ ACTIVE SOCKETS: 64    │ │
│ │ ────────────────────────────────────────────────────────────────── │ │                       │ │
│ │ ▸ 1492     cloudd                 1420.4 KB    380.2 KB   128.4 MB │ │ ESTABLISHED █████  42 │ │
│ │   882      Zen Browser             310.0 KB     24.1 KB    84.2 MB │ │ HTTPS :443  ████   38 │ │
│ │   4102      RiotClientServices       18.5 KB     12.0 KB    14.9 MB │ │ UDP/QUIC    ██     14 │ │
│ └────────────────────────────────────────────────────────────────────┘ │ DNS 53/853  █       4 │ │
│ ┌─ TOP TALKERS — SESSION DATA (MB) ──────────────────────────────────┐ │ TIME_WAIT   █       6 │ │
│ │ cloudd           ████████████████████████████████████░░░░  128.4 MB│ │ SYN/CLOSE_W         0 │ │
│ │ Zen Browser      ██████████████████████░░░░░░░░░░░░░░░░░░   84.2 MB│ │                       │ │
│ │ RiotClientServic ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   14.9 MB│ │ ── ENCRYPTION 98% ──  │ │
│ └────────────────────────────────────────────────────────────────────┘ │ ███████████████████░  │ │
│ ┌─ SELECTED PROCESS INSPECTOR ([Space] Freeze/Unfreeze · [B] Shape) ─┐ │ HTTPS/TLS ███████  38 │ │
│ │ cloudd (PID 1492)  ·  State: [!] BANDWIDTH HOG  ·  Session: 128 MB │ │ QUIC/UDP  ███      14 │ │
│ │ ▼ DOWN ████████████████░░░░░░░░  1420.4 KB/s                       │ │ PLAINTEXT           1 │ │
│ │ ▲ UP   ████████░░░░░░░░░░░░░░░░   380.2 KB/s                       │ └───────────────────────┘ │
│ └────────────────────────────────────────────────────────────────────┘                           │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### B. Tab 9 (`🎮 GAMING`) — Unified 3-Tier Engine + Real Multi-Hop Telemetry

Tab 9 now displays the complete **3-Tier Zero-Jitter Engine** (`[Space]` for Tier 1+2 Shield, `[H]` for Opt-In Tier 3 Hardcore RT) and replaces synthetic hop math with **real live ICMP hops from `scope.rs` + live TCP game server RTT**:

```
┌─ 🎮 GAMING RADAR ────────────────────────────────────────────────────────────────────────────────┐
│ ┌─ BEST REGION ─────┐ ┌─ JITTER STABILITY ┐ ┌─ ZERO-JITTER ─────┐ ┌─ ANTI-CHEAT ──────┐          │
│ │ 18 ms             │ │ ±0.6 ms           │ │ T1+T2+T3 LOCKED   │ │ VERIFIED          │          │
│ │ EU-Central (FRA)  │ │ [ROCK SOLID]      │ │ [50Hz Fast-Path]  │ │ Vanguard Safe     │          │
│ └───────────────────┘ └───────────────────┘ └───────────────────┘ └───────────────────┘          │
│ ┌─ ⚔ REGIONAL GAME CLUSTER PING MATRIX ──────────┐ ┌─ ◉ 3-TIER ZERO-JITTER ENGINE ─────────────┐ │
│ │   CLUSTER / REGION             PING    JITTER  │ │ Platform : macOS Darwin [T1+T2+T3 HARDCORE]│ │
│ │ ▶ Valorant — EU Frankfurt       18 ms  ±0.4ms  │ │ Scan Mute: 0Hz LOCKED (AWDL + Locationd)  │ │
│ │   League — EUW Amsterdam        22 ms  ±0.7ms  │ │ Roam Lock: PINNED (a4:83:e7:11:22:33)     │ │
│ │   CS2 — EU Vienna               26 ms  ±0.9ms  │ │ Radio PS : CAM 25Hz ACTIVE (Zero Doze)    │ │
│ │   Overwatch 2 — EU Paris        24 ms  ±0.8ms  │ │ AQM Queue: FQ-CoDel 3ms · PMTU 1452       │ │
│ │   Apex Legends — EU London      29 ms  ±1.1ms  │ │ Fast-Path: 50Hz HOT (192.168.1.1 · ARP)   │ │
│ │   Valorant — ME Bahrain         34 ms  ±1.2ms  │ │ Tier 3 RT: LOCKED (Mach RT · P-Core QoS)  │ │
│ │                                                │ │ Control  : [Space] Shield · [H] Tier 3 RT │ │
│ │                                                │ └───────────────────────────────────────────┘ │
│ │                                                │ ┌─ ▦ 60-SECOND JITTER SPARKLINE ────────────┐ │
│ │                                                │ │ Jitter Track : ±0.6ms avg                 │ │
│ │                                                │ │ Live Spark   : ▁▁▂▁▁▁▂▁▁▁▁▁▂▁▁▁▁▁▁▁▁▁▁▁▁  │ │
│ │                                                │ │ Health       : ● Rock Solid (Zero Spikes) │ │
│ │                                                │ └───────────────────────────────────────────┘ │
│ │                                                │ ┌─ ◪ LIVE ROUTE HOP TELEMETRY (REAL HOPS) ──┐ │
│ │                                                │ │ [H1] Wi-Fi Gateway (192.168.1.1)   1.8ms  │ │
│ │                                                │ │ [H2] ISP Access Node (Real ICMP)   6.4ms  │ │
│ │                                                │ │ [H3] Core Transit (1.1.1.1)       11.2ms  │ │
│ │                                                │ │ [H4] Valorant — EU Frankfurt      18.0ms  │ │
│ └────────────────────────────────────────────────┘ └───────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### C. Tab 4 (`⚙ TUNE`) + Tab 0 (`🛡 PRIVACY`) + Tab 8 (`📡 LAN`) State Sync

```
┌─ ⚙ TUNE & FIX (Tab 4) ─────────────────┐        ┌─ 🎮 GAMING RADAR (Tab 9) ────────────────┐
│ ▸ [ ● ON  ] Zero-Lag Mode (T1+T2)      │◄──────►│ [T1+T2 LOCKED] Scan Mute + 50Hz Pacer    │
│   [ ● ON  ] 25Hz CAM Anti-Sleep Pacer  │        │ (Same ZeroJitterEngine instance!)        │
│   [ ● ON  ] Smart PMTU Clamp (1452)    │        └──────────────────────────────────────────┘
├────────────────────────────────────────┤
│ WAN & DNS HEALTH                       │        ┌─ 🛡 PRIVACY FORTRESS (Tab 0) ─────────────┐
│ DNS      : 1.1.1.1 (Cloudflare 9.4ms)  │◄──────►│ ▶ Cloudflare (1.1.1.1)  9.4ms  [ACTIVE]  │
│ Security : [OK] Gateway ARP Verified   │        │ (Unified DNS Race & Apply state!)        │
└────────────────────────────────────────┘        └──────────────────────────────────────────┘
                     ▲
                     │                            ┌─ 📡 LAN RADAR (Tab 8) ───────────────────┐
                     └───────────────────────────►│ Gateway ARP & Evil-Twin BSSID Guard      │
                                                  │ (Powered by security_guard.rs + lan)     │
                                                  └──────────────────────────────────────────┘
```

---

## 5. Phased Implementation Plan

### Phase 1: Unify Anti-Jitter, CAM Pacer & MTU State (`app.rs`, `zero_jitter.rs`, `antilag.rs`)
1. Wire `Action::ZeroLag` and `Action::CamPacer` in `src/app.rs` so toggling them on Tab 4 (`TUNE & FIX`) or via the Spotlight Action Palette (`[P]`) engages/disengages `self.game_mode.jitter_engine` (`Tier 1 + Tier 2`) and syncs `self.antilag.zero_lag_active` and `self.scope.cam_pacer_active`.
2. Allow `Action::ZeroLag`, `Action::CamPacer`, and `Action::GamingToggleTier3` while in Game Mode (since they use the 100% anti-cheat-safe `ZeroJitterEngine` rather than risky kernel/process mutations).
3. Connect `src/core/pmtu.rs` (`discover_pmtu`) into `Action::MtuClamp` so clamping MTU uses real Path MTU discovery instead of a hardcoded blind toggle.

### Phase 2: Replace Synthetic Hops in Tab 9 with Real `scope_hops` (`gaming_radar.rs`, `app.rs`)
1. Pass `&self.scope_snap.hops` into `gaming_radar.probe_quick_sample(...)` (or enrich `gaming_data.route_hops` in `app.rs`) so Hops 1–3 on Tab 9 display the **real live ICMP RTT and status verdicts** from `scope.rs`, and Hop 4 displays the **selected game server's live TCP RTT**.
2. Remove the unused `deck_lines` computation from `App::build_render()` and remove `deck_lines` from `RenderState` in `src/render.rs`.

### Phase 3: Unify DNS Engine & Remove Duplicate LAN Box (`processes.rs`, `privacy_fortress.rs`, `lan_radar.rs`)
1. Remove `render_lan()` from `src/ui/views/processes.rs` (Tab 3) so `render_detail()` spans the full bottom width of the left column, giving the Process Firewall & Top Talkers table clean breathing room while Tab 8 (`LAN Radar`) owns LAN discovery exclusively.
2. Sync `PrivacyFortress` DNS race results (`src/core/privacy_fortress.rs`) with `App::dns_results` and ` telemetry.dns_servers` in `src/app.rs` so racing or switching DNS on Tab 0 immediately updates Tab 4 (`WAN & DNS Health`) and Tab 1 (`Overview`).
3. Surface `SecurityGuard` (`src/core/security_guard.rs`) Evil-Twin BSSID & ARP spoofing status inside Tab 8 (`LAN Radar`) so zero core modules are left orphaned in CLI-only mode.

### Phase 4: Verification & Release
1. Run `cargo test` to verify all unit, UI layout, and performance tests pass with zero warnings.
2. Run `cargo install --path . --force` to update both `aether` and `wifi-tui` binaries.
3. Commit and push the unified architecture to GitHub (`https://github.com/abdullah-binmadhi/Aether.git`).
