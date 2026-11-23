# Redesign Plan: Zero-Jargon Auto-Pilot Telemetry Overview Page (Tab 1)

> **Goal:** Redesign **Tab 1 (`OVERVIEW`, `src/ui/views/dashboard.rs`)** into an intuitive, plain-English **Auto-Pilot Telemetry Cockpit** where you can see—at a single glance, with **zero networking jargon**—that every feature we built today is actively working and keeping your Wi-Fi fast, smooth, and reliable.

---

## 1. Design Philosophy: Zero Technical Jargon

The current Overview page shows terms like `MCS /11`, `SNR dB`, `AWDL STD`, `Hop 2`, and `RTT`, which makes it hard to tell if Auto-Pilot is actually helping. The redesigned Overview translates every low-level mechanism into **clear, human-friendly telemetry**:

| Under-the-Hood Feature Built Today | Plain-English Name on Overview | What It Tells You Live |
| :--- | :--- | :--- |
| `pfctl` `DSCP EF` + `dnctl` CoDel Queue | **Priority Fast-Lane** | `● WORKING — Your apps & games jump ahead of other users` |
| `awdl0` / `llw0` Hardware Down-Lock | **Background Scan Blocker** | `● WORKING — Apple AirDrop/Handoff lag spikes blocked` |
| `1200B / 1360B` Micro-MSS + `1MB` TX Cap | **Crowded Wi-Fi Sizer** | `● WORKING — Small fast packets active (0 queue stalls)` |
| Self-Beacon Filter + Non-DFS Smart Roam | **Clean Signal Finder** | `● WORKING — False clashes removed · 5GHz locked` |
| `25Hz / 50Hz` CAM Gateway Heartbeat | **Wi-Fi Awake Heartbeat** | `● WORKING — 50 pulses/sec keeping Wi-Fi awake` |
| Anti-Cheat Guard + Router Fast-Path Lease | **Game & Safety Shield** | `● SAFE — Anti-cheat safe · 0 risky kernel touches` |

---

## 2. Full-Screen Visual Sketch of the Redesigned Overview Page

```text
┌─ NAV ──────┬──────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  AETHER    │ [ OVERVIEW ]  [ ⚙ AUTO: CROWDED WI-FI SHIELD ]   ● Faisal.5G · 5GHz · 866 Mbps       HEALTH 96/100 [GOOD]│
│  COCKPIT   ├──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│            │ ┌─ CONNECTION QUALITY ─┐ ┌─ RESPONSE TIME ──────┐ ┌─ WIFI STRENGTH ──────┐ ┌─ LIVE SPEED ──────────────┐ │
│ 1 ◉ OVER   │ │   96/100 EXCELLENT   │ │   11.2 ms  Smooth    │ │   Strong (-58 dBm)   │ │   ▼ 142 Mbps   ▲ 28 Mbps  │ │
│ 2 ◈ AIR    │ │   ▁▂▃▄▅▆▇█▇███ =     │ │   ▂▂▃▂▁▂▁▂▁▁▂▁ =     │ │   ▆▆▇▇▇▆▇▇▇▆▇▇ =     │ │   ▂▃▅▇▆▄▃▄▆█▇▆ =          │ │
│ 3 ◈ PROCS  │ └──────────────────────┘ └──────────────────────┘ └──────────────────────┘ └───────────────────────────┘ │
│ 4 ◆ TUNE   ├────────────────────────────────────────────────────┬─────────────────────────────────────────────────────┤
│ 5 ▲ SPEED  │ ⚙ AUTO-PILOT LIVE PROTECTIONS  ([A] Toggle)        │ SMOOTHNESS & SPEED MONITOR        ([M] Switch View) │
│ 6 ⊞ DEVICE │                                                    │                                                     │
│ 7 ◧ CLEAN  │ [ ● AUTO-PILOT ON: CROWDED WI-FI SHIELD ]          │ Smoothness: ±1.2ms  ·  Dropped: 0.0%  ·  Blocked: 14│
│ 8 ⌖ LAN    │                                                    │ 50m ┤·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·    │
│ 9 ⚔ GAME   │ [● WORKING] Priority Fast-Lane                     │     │  ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·  │
│ 0 ⛨ PRIV   │             Your traffic jumps ahead of LAN users  │ 25m ┤·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·    │
│            │ [● WORKING] Background Scan Blocker                │     │  ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·  │
│            │             AirDrop/Handoff lag spikes blocked     │  0m ┤▄▄▄▄▄▅▃▄▄▃▄▄▃▃▄▄▄▃▄▄▄▃▄▄▃▄▃▄▄▃▄▃▄▃▄▄▃▄▃▄▃▄▄▄▃▄▄  │
│            │ [● WORKING] Crowded Wi-Fi Sizer                    │ DOWN  ▂▃▅▆▇██▆▄▃▂▃▄▆▇█  142 Mbps                  │
│            │             Small fast packets · 0 queue stalls    │ UP    ▁▂▃▃▄▃▂▁▂▃▄▃▂▁▂▃   28 Mbps                  │
│            │ [● WORKING] Clean Signal Finder                    ├─────────────────────────────────────────────────────┤
│            │             0 ghost clashes · Best 5GHz locked     │ AUTO-PILOT ACTIVITY FEED & READINESS                │
│            │ [● WORKING] Wi-Fi Awake Heartbeat                  │                                                     │
│            │             50 pulses/sec keeping router awake     │ [ ✓ Online Gaming: Smooth ]  [ ✓ 4K Video: Instant ]│
│            │ [● SAFE   ] Game & Safety Shield                   │ [ ✓ Voice / Zoom: Clear  ]                          │
│            │             Anti-cheat safe · 0 risky changes      │                                                     │
│            ├────────────────────────────────────────────────────┤ LIVE LOG (Proof Auto-Pilot Is Working):           │
│            │ WHERE IS THE SLOWDOWN? (CONNECTION PATH)           │  21:01:12  ✓ Blocked Apple scan lag spike           │
│            │                                                    │  21:00:58  ✓ Cleared false Ch 132 router clash      │
│            │ [1] Your Mac Wi-Fi       ████████████░░  0.2ms ✓Ok │  21:00:45  ✓ Put your traffic in Fast-Lane queue   │
│            │ [2] Home Wi-Fi Router    ████████████░░  2.1ms ✓Ok │  21:00:30  ✓ Shrunk packets for crowded Wi-Fi       │
│            │ [3] Internet Provider    ████████████░░  6.4ms ✓Ok │  21:00:15  ✓ Auto-Pilot started (24/7 protection)   │
│            │ [4] Game & Video Servers ████████████░░ 11.2ms ✓Ok │                                                     │
│            │                                                    │                                                     │
│            │ VERDICT: ✓ Your connection is smooth end-to-end.   │                                                     │
│            ├────────────────────────────────────────────────────┴─────────────────────────────────────────────────────┤
│  p palette │ › ⚙ AutoPilot [MULTI-USER CONGESTION SHIELD]: Priority Fast-Lane + Small Packets + Scan Blocker Active   │
│  q quit    │ [A] Auto-Pilot   [Enter] Action Menu   [R] Refresh   [L] Zero-Lag   [M] Switch Graph   [1-0] Pages       │
└────────────┴──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Panel-by-Panel Architecture & Behaviour

### Top KPI Strip (4 Friendly Scorecards)
1. **CONNECTION QUALITY (`0–100`)**: Replaces raw health jargon with `"EXCELLENT"`, `"GOOD"`, `"BUSY"`, or `"NEEDS HELP"`.
2. **RESPONSE TIME**: Average ping in milliseconds (`ms`) with a human label (`"Super Smooth"`, `"Normal"`, `"Delayed"`).
3. **WIFI STRENGTH**: Plain-English strength (`"Strong"`, `"Good"`, `"Fair"`, `"Weak"`) + band (`"5GHz Fast"` / `"2.4GHz Slow"`).
4. **LIVE SPEED**: Combined live `▼ Download` and `▲ Upload` speeds in readable units (`KB/s` or `Mbps`).

### Tile 1 (Top-Left): `⚙ AUTO-PILOT LIVE PROTECTIONS`
* **Top Status Banner**:
  * Green pill `[ ● AUTO-PILOT ON: <PLAIN MODE NAME> ]` (e.g., `NORMAL`, `STREAMING 4K`, `CROWDED WI-FI SHIELD`, `GAMING LOCKED`, `LINK RECOVERY`) or dim `[ ○ AUTO-PILOT PAUSED (MANUAL) ]`.
* **6 Live Feature Rows**:
  Each row checks live engine state and renders a color-coded pill (`[● WORKING]` green, `[● SAFE]` green, `[○ STANDBY]` dim, `[! NEEDED]` amber/red if sudo or permission is missing) plus a single-line plain-English status:
  1. **Priority Fast-Lane**: Active whenever `BandwidthShaper::aqm_active` or `wmm_vip_requested` is enabled.
  2. **Background Scan Blocker**: Active whenever `zero_lag_mode` or `GameMode` Tier 1 is silencing AirDrop / `awdl0`.
  3. **Crowded Wi-Fi Sizer**: Active when `TcpCongestionTier` is in `Balanced4Mb`, `TightAntiBloat1Mb`, or `CoChannelBypass1Mb`.
  4. **Clean Signal Finder**: Shows active band cleanliness (`"0 false clashes · 5GHz locked"` or `"Steering to stronger 5GHz"`).
  5. **Wi-Fi Awake Heartbeat**: Active when `cam_pacer_active` or Tier 2 50Hz gateway heartbeat is running.
  6. **Game & Safety Shield**: Shows whether a game/anti-cheat is detected and confirms zero unsafe kernel edits.

### Tile 2 (Top-Right): `SMOOTHNESS & SPEED MONITOR`
* Keeps the crisp 2D phosphor oscilloscope (`[M]` cycles Latency, Download, Upload, Combined), but updates the header to plain English:
  * `Smoothness: ±1.2ms  ·  Dropped Packets: 0.0%  ·  Blocked Spikes: <N>`

### Tile 3 (Bottom-Left): `WHERE IS THE SLOWDOWN? (CONNECTION PATH)`
* Replaces `"4-HOP PATH"` (`Mac en0`, `Gateway`, `ISP Edge`, `Google Edge`) with clear stage names:
  1. `Your Mac Wi-Fi`
  2. `Home Wi-Fi Router`
  3. `Internet Provider`
  4. `Game & Video Servers`
* Includes a single-line plain-English verdict at the bottom (e.g., `"✓ Everything from your Mac to the internet is running smoothly"` or `"! Slowdown is at your Home Wi-Fi Router — Auto-Pilot is clearing the queue"`).

### Tile 4 (Bottom-Right): `AUTO-PILOT ACTIVITY FEED & READINESS`
* **Workload Readiness Pills**: `[ ✓ Online Gaming: Smooth ]`, `[ ✓ 4K Video: Instant ]`, `[ ✓ Voice / Zoom: Clear ]`.
* **Live Timestamped Activity Log**: Rolling ring buffer of the last 8–12 Auto-Pilot events translated into plain English with `HH:MM:SS` timestamps, proving that Auto-Pilot is actively monitoring and fixing issues in real time.

---

## 4. Code Implementation Roadmap

1. **`src/core/autopilot.rs`**:
   - Add `FeatureTelemetryRow` (`name`, `badge`, `active`, `plain_status`) and `ActivityLogEntry` (`timestamp`, `plain_message`).
   - Add `activity_log: VecDeque<ActivityLogEntry>` and `blocked_spikes_count: u64` to `AutoPilotEngine`.
   - Implement `AutoPilotEngine::build_feature_telemetry(&self, tel, scope, game_mode_active, kernel_tier, aqm_active) -> Vec<FeatureTelemetryRow>`.
   - Record human-friendly entries into `activity_log` on startup and on every state transition.
2. **`src/render.rs` & `src/app.rs`**:
   - Add `autopilot_features: Vec<FeatureTelemetryRow>` and `autopilot_log: Vec<ActivityLogEntry>` to `RenderState` and `ActionOutcome`.
   - Populate them in `App::build_render()` and merge background transition logs in `App::drain_outbox()`.
3. **`src/ui/views/dashboard.rs`**:
   - Refactor `draw_kpi`, `render_rf_gauges` -> `render_autopilot_protections`, `render_oscilloscope` -> `render_smoothness_monitor`, `render_hop_bars` -> `render_connection_path`, and `render_advice` -> `render_activity_and_readiness`.
   - Keep `wrap_text` and responsive layout constraints so nothing overflows on narrow terminals.
4. **Tests**:
   - Add unit tests in `src/core/autopilot.rs` for `build_feature_telemetry` (all 6 rows) and `activity_log` ring-buffer capping.
   - Add a `ratatui::backend::TestBackend` render test in `src/ui/views/dashboard.rs` verifying all 4 redesigned panels render cleanly without overflow and contain zero technical jargon leaks.
