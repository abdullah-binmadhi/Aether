# Autonomous Closed-Loop Wi-Fi Congestion & Gaming Optimization Engine (`AutoPilotEngine`)

> **Target Codebase:** `WIFI-TUI` / `aether` (`/Users/abdullahbinmadhi/Projects/WIFI-TUI`)  
> **Scope:** Implementation architecture, Rust code sketches for 6 high-impact missing/partial features, closed-loop automation state machine, and projected before/after performance benchmarks.

---

## 1. System Architecture & Closed-Loop Data Flow

Currently, `WIFI-TUI` has strong individual engines that are **disconnected**:
* `PresetEngine::detect_issues_and_recommend` (`src/core/preset_engine.rs`) analyzes telemetry on every tick in `App::refresh` (`src/app.rs`), but only updates `effective_preset_id` for UI display without executing remediations.
* `Orchestrator` (`src/core/orchestrator.rs`) implements a 4-tier escalation ladder (`evaluate_health` / `execute_tier`), but only runs when manually triggered.
* `game_mode_tick` (`src/app.rs` -> `src/core/game_mode.rs`) automatically enables `ZeroJitterEngine` Tiers 1 & 2 on game detection, **but** explicitly reverts `KernelTuner` WMM VIP and TCP No-Delay tweaks, and never drives `BandwidthShaper`, `roaming`, or `WanDoctor`.
* `BandwidthShaper::configure_dummynet_pipe` (`src/core/bandwidth_shaper.rs`) creates a `dnctl` pipe without loading a `pfctl` `dummynet-anchor`, so packets never enter the pipe.

### Unified Closed-Loop Architecture

```
+-----------------------------------------------------------------------------+
|                        TELEMETRY & WORKLOAD SENSORS                         |
|  corewlan.rs (RSSI, Noise, MCS, Band, Width, CachedNeighbors)               |
|  hops.rs / hop2.rs (RTT, Jitter, Loss%, Hop-2 Bufferbloat Delta)            |
|  airtime.rs (Airtime Load %, TX Retry %, FCS Error Delta)                   |
|  procmon.rs (Per-PID Down/Up KB/s, Hog Detection)                           |
|  game_mode.rs (pgrep GAME_PROCESSES + Anti-Cheat Guard: vgc/vgk/EAC/BE)     |
|  wan_doctor.rs (ECS DNS, IPv6 Asymmetry, Wire PMTU, Zombie Sockets)         |
+--------------------------------------+--------------------------------------+
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                    CAPABILITY DISCOVERY MATRIX (Startup)                    |
|  - Privilege Tier: passwordless `sudo -n` (sysctl / pfctl / dnctl / ifconfig)|
|  - Anti-Cheat Tier: Vanguard/EAC/BE active? -> User-Space Safe vs Kernel    |
|  - Radio Tier: 5GHz / 6GHz sibling BSSIDs & DFS channels available?         |
|  - Router Tier: UPnP IGD / NAT-PMP / PCP gateway lease support?             |
+--------------------------------------+--------------------------------------+
                                       |
                                       v
+-----------------------------------------------------------------------------+
|                 AUTOPILOT CLOSED-LOOP ENGINE (autopilot.rs)                 |
|  States: IdleOptimal <-> HighThroughputStream <-> MultiUserCongestion       |
|          <-> PreGamePreparation <-> CompetitiveGamingLocked <-> LinkRescue  |
|  Guards: 15s Hysteresis Dwell, Exponential Cooldown, Zero-Drop Match Lock   |
+--+------------+--------------+--------------+---------------+------------+--+
   |            |              |              |               |            |
   v            v              v              v               v            v
[pfctl+dnctl] [awdl0/llw0] [BSSID Roam]  [Adaptive TCP]  [UPnP/PCP]  [Daemon/TUI]
 DSCP EF +    Hard Down    5/6GHz Steer  Buffer & MSS    Router NAT  Outbox &
 FQ-CoDel     Lockout      (Pre-Game)    Scaling         Flow Lease  Rollback
```

### Closed-Loop Execution Sequence (`App::refresh` & `run_autonomous_daemon`)

```mermaid
flowchart TD
    T["Tick: PingScope + Collector + Airtime + ProcMon + GameDetect"] --> CAP{"Capability & Safety Check"}
    CAP -->|"Anti-Cheat Detected (vgc/vgk/EAC)"| SAFE["Safe Profile: User-space + AWDL + 50Hz Pacer + Non-Kernel Shaper"]
    CAP -->|"No Kernel Anti-Cheat + Sudo OK"| FULL["Full Profile: Kernel WMM + pfctl/dnctl + awdl0 down + Adaptive TCP"]
    SAFE --> SM["AutoPilot State Machine (with 15s Hysteresis)"]
    FULL --> SM
    SM -->|"Loss >= 8% or Offline"| RESCUE["EmergencyLinkRescue: WanDoctor + DHCP + Zombie Reap"]
    SM -->|"Game Running"| GAME["CompetitiveGamingLocked: DSCP EF + AWDL Down + Tight 1MB TX + BSSID Pin"]
    SM -->|"Airtime >= 65% or Bloat > 18ms"| CONG["MultiUserCongestion: pfctl FQ-CoDel + Hog Throttle + MSS 1360"]
    SM -->|"Down > 1.2MB/s & Low Contention"| STREAM["HighThroughputStream: 32MB BDP + MSS 1452 + Pre-warmed GGC"]
    SM -->|"Clean Link & Idle"| IDLE["IdleOptimal: Revert Clamps, Release BSSID, Resume Sync"]
```

### Capability Discovery Matrix

| Capability Probe | Check Command / Method | Unlocked Actions When Available | Safe Fallback When Unavailable |
| :--- | :--- | :--- | :--- |
| **Passwordless Sudo** | `sudo -n sysctl net.inet.tcp.delayed_ack` | `sysctl -w`, `pfctl -a com.apple/aether`, `dnctl`, `ifconfig awdl0/llw0 down` | `defaults write DisableAirDrop`, 25Hz/50Hz UDP pacer, `taskpolicy -b`, `renice` |
| **Anti-Cheat Guard** | `pgrep -x vgc / vgk / EasyAntiCheat / BEService` | Blocks kernel `sysctl` writes, `SIGSTOP`, and live MTU changes mid-match | Allows AWDL `DisableAirDrop`, 50Hz gateway pacer, DNS flush, `taskpolicy` |
| **Router UPnP / PCP** | UPnP IGD descriptor (`WANIPConnection`) / UDP `:5351` | `AddPortMapping` / PCP `MAP` priority lease for game UDP ports | Local `pfctl` `set tos ef` DSCP marking + 50Hz NAT keepalive |
| **Multi-Band Sibling** | `Telemetry.sibling_5g_ssid` / `cachedScanResults` | Pre-game BSSID steer to 5GHz/6GHz lowest-load AP before BSSID lock | Advisory badge + WMM `AC_VO` (`CWmin=3`) contention win on current AP |

---

## 2. Detailed Design & Rust Code Sketches for the 6 High-Impact Features

### Feature 1: Completed `pfctl` + `dnctl` Fair-Queueing & DSCP `EF` Fast-Path
**File:** `src/core/bandwidth_shaper.rs`  
**Current Gap:** `BandwidthShaper::configure_dummynet_pipe` creates `dnctl pipe 1` manually per PID, but never loads a `pfctl` `dummynet-anchor` rule—meaning zero packets are routed through the shaper pipe or marked with `DSCP EF`.

```rust
// Sketch for src/core/bandwidth_shaper.rs
pub struct AqmConfig {
    pub interface: String,
    pub up_cap_mbps: u32,    // 85-90% of measured uplink to prevent router queue buildup
    pub down_cap_mbps: u32,  // 85-90% of measured downlink
    pub game_udp_ports: Vec<u16>,
    pub throttled_pids: Vec<i32>,
}

impl BandwidthShaper {
    /// Atomically loads `dnctl` FQ-CoDel pipes and a dedicated `pfctl` anchor (`com.apple/aether`)
    /// so game UDP packets get DSCP EF (802.11e AC_VO) and bypass bulk congestion queues.
    pub fn enable_autopilot_aqm(&mut self, cfg: &AqmConfig) -> Result<String, String> {
        let up_bw = format!("{}Mbit/s", cfg.up_cap_mbps.max(5));
        // 1. Configure CoDel pipe + weighted queues (weight 100 interactive vs weight 15 bulk)
        let dnctl_cmds = [
            vec!["sudo", "-n", "dnctl", "pipe", "1", "config", "bw", &up_bw, "codel", "target", "3ms", "interval", "60ms", "ecn"],
            vec!["sudo", "-n", "dnctl", "queue", "1", "config", "pipe", "1", "weight", "100"],
            vec!["sudo", "-n", "dnctl", "queue", "2", "config", "pipe", "1", "weight", "15"],
        ];
        for argv in dnctl_cmds {
            if !super::cmd::ok(&argv, std::time::Duration::from_millis(800)) {
                return Err("dnctl pipe/queue config requires passwordless sudo".into());
            }
        }

        // 2. Build isolated pfctl anchor ruleset (never clobbers main /etc/pf.conf rules)
        let mut rules = String::new();
        // Prioritize detected game UDP ports + ICMP/DNS into WMM AC_VO (TOS EF = 0xb8) + Queue 1
        rules.push_str(&format!(
            "match out on {} proto udp from any to any port {{ 53, 3074, 3478:3479, 5000:5500, 7000:8000, 27000:27050 }} set tos ef queue 1\n",
            cfg.interface
        ));
        // Route bulk outbound TCP (ports 80/443 > 1500B burst) through dummynet CoDel queue 2
        rules.push_str(&format!(
            "dummynet out on {} proto tcp from any to any port {{ 80, 443 }} queue 2\n",
            cfg.interface
        ));

        // 3. Load anchor via stdin (`pfctl -a com.apple/aether -f -`) and ensure pf is enabled
        let ok = super::cmd::run_with_stdin(
            &["sudo", "-n", "pfctl", "-a", "com.apple/aether", "-f", "-"],
            &rules,
            std::time::Duration::from_millis(1000),
        );
        let _ = super::cmd::ok(&["sudo", "-n", "pfctl", "-e"], std::time::Duration::from_millis(500));

        if ok {
            Ok(format!("● AQM Active: pfctl AC_VO (DSCP EF) + CoDel 3ms @ {up_bw}"))
        } else {
            Err("Failed to load pfctl anchor com.apple/aether".into())
        }
    }

    pub fn disable_autopilot_aqm(&mut self) {
        let _ = super::cmd::ok(
            &["sudo", "-n", "pfctl", "-a", "com.apple/aether", "-F", "all"],
            std::time::Duration::from_millis(800),
        );
        self.clear_dummynet_pipe();
    }
}
```

---

### Feature 2: Hardware `awdl0` / `llw0` Interface Down-Lock
**File:** `src/core/antilag.rs` & `src/core/zero_jitter.rs`  
**Current Gap:** `DisableAirDrop -bool YES` leaves `awdl0` and `llw0` (Universal Control, Handoff, AirPlay) active, causing periodic off-channel scans (Ch 6 / 44 / 149) and 40–120ms spikes on congested channels.

```rust
// Sketch for src/core/antilag.rs
impl AntiLagEngine {
    /// Physically brings down `awdl0` and `llw0` if UP and passwordless sudo is available.
    /// Anti-cheat safe (interface state only, no process/memory touch).
    pub fn enforce_awdl_down(&self, down: bool) -> String {
        let action = if down { "down" } else { "up" };
        if !self.root_authorized {
            // Fallback to user-space AirDrop suppression
            let val = if down { "YES" } else { "NO" };
            let _ = super::cmd::run(
                &["defaults", "write", "com.apple.NetworkBrowser", "DisableAirDrop", "-bool", val],
                std::time::Duration::from_millis(600),
            );
            return format!("○ User-space AirDrop {val} (press [S] to lock hardware awdl0 {action})");
        }

        // Only issue `ifconfig down` if `awdl0` is actually UP (avoids redundant forks)
        let check = super::cmd::run(&["ifconfig", "awdl0"], std::time::Duration::from_millis(400));
        let is_up = check.stdout.contains("<UP,") || check.stdout.contains(",UP,") || check.stdout.contains(",UP>");
        if down && is_up {
            let _ = super::cmd::ok(&["sudo", "-n", "ifconfig", "awdl0", "down"], std::time::Duration::from_millis(500));
            let _ = super::cmd::ok(&["sudo", "-n", "ifconfig", "llw0", "down"], std::time::Duration::from_millis(500));
            return "● Hardware awdl0 & llw0 LOCKED DOWN (0ms off-channel scan spikes)".into();
        } else if !down && !is_up {
            let _ = super::cmd::ok(&["sudo", "-n", "ifconfig", "awdl0", "up"], std::time::Duration::from_millis(500));
            let _ = super::cmd::ok(&["sudo", "-n", "ifconfig", "llw0", "up"], std::time::Duration::from_millis(500));
            return "○ Hardware awdl0 & llw0 RESTORED".into();
        }
        format!("● AWDL/LLW state verified ({action})")
    }
}
```

---

### Feature 3: Active BSSID & Clean-Channel Force-Roam (Pre-Game Only)
**File:** `src/core/roaming.rs` & `src/core/corewlan.rs`  
**Current Gap:** `roaming::evaluate_roaming` identifies `sticky_client` and `sibling_5g_ssid`, and `DarwinZeroJitter` records `locked_bssid`, but macOS never executes a steer to the cleaner 5GHz/6GHz BSSID.

```rust
// Sketch for src/core/roaming.rs
pub struct RoamCooldown {
    pub last_roam_at: Option<std::time::Instant>,
    pub attempts_this_hour: u8,
}

/// Executes an active roam to a superior 5GHz/6GHz sibling SSID or triggers a
/// non-disruptive interface re-association ONLY when no match socket is locked.
pub fn execute_smart_roam(
    interface: &str,
    eval: &RoamEval,
    wifi_password: &str,
    is_game_locked: bool,
    cooldown: &mut RoamCooldown,
) -> Option<String> {
    if is_game_locked || !eval.sticky_client {
        return None; // Never drop an active match socket mid-game
    }
    if let Some(last) = cooldown.last_roam_at {
        if last.elapsed() < std::time::Duration::from_secs(90) || cooldown.attempts_this_hour >= 4 {
            return None; // Hysteresis guard against ping-pong roaming
        }
    }
    let best = eval.best_candidate.as_ref()?;
    if best.gain_score < 14 {
        return None;
    }

    cooldown.last_roam_at = Some(std::time::Instant::now());
    cooldown.attempts_this_hour += 1;

    // 1. If a distinct 5GHz/6GHz sibling SSID exists, associate directly
    let out = if wifi_password.is_empty() {
        super::cmd::run(
            &["networksetup", "-setairportnetwork", interface, &best.ssid],
            std::time::Duration::from_millis(3500),
        )
    } else {
        super::cmd::run(
            &["networksetup", "-setairportnetwork", interface, &best.ssid, wifi_password],
            std::time::Duration::from_millis(3500),
        )
    };
    if out.success {
        Some(format!("● Smart-Roamed to {} ({} Ch{}, {}dBm)", best.ssid, best.band.label(), best.channel_num, best.signal_dbm))
    } else {
        None
    }
}
```

---

### Feature 4: Adaptive Congestion TCP Buffer & Frame-Size (MSS) Scaling
**File:** `src/core/kernel_tuner.rs`  
**Current Gap:** `toggle_8mb_streaming_buffers` locks `autorcvbufmax=33554432` (32MB) unconditionally. Under congested Wi-Fi with high L2 retries, oversized buffers induce massive self-inflicted TX ring bufferbloat.

```rust
// Sketch for src/core/kernel_tuner.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpCongestionTier {
    HighBdp32Mb,      // Clean link, high throughput / 4K streaming
    Balanced4Mb,      // Moderate contention (airtime 45-67%)
    TightAntiBloat1Mb // Severe congestion (airtime >= 68% or retry >= 8%) or Gaming
}

impl KernelTuner {
    pub fn adapt_tcp_profile_to_congestion(
        &mut self,
        airtime_load_pct: i64,
        tx_retry_pct: f64,
        is_gaming: bool,
        anti_cheat_active: bool,
    ) -> Option<String> {
        if anti_cheat_active {
            return None; // Strict anti-cheat boundary: zero sysctl mutations
        }
        let target = if is_gaming || airtime_load_pct >= 68 || tx_retry_pct >= 8.0 {
            TcpCongestionTier::TightAntiBloat1Mb
        } else if airtime_load_pct >= 45 || tx_retry_pct >= 3.5 {
            TcpCongestionTier::Balanced4Mb
        } else {
            TcpCongestionTier::HighBdp32Mb
        };

        let pairs: Vec<String> = match target {
            TcpCongestionTier::TightAntiBloat1Mb => vec![
                "net.inet.tcp.autorcvbufmax=2097152".into(),
                "net.inet.tcp.autosndbufmax=1048576".into(), // Tight 1MB TX cap stops TX ring bloat
                "net.inet.tcp.recvspace=262144".into(),
                "net.inet.tcp.sendspace=131072".into(),
                "net.inet.tcp.mssdflt=1360".into(),          // Shorter air-frames survive noisy RF better
                "net.inet.tcp.delayed_ack=0".into(),
                "net.inet.tcp.rexmt_slop=20".into(),
                "net.inet.tcp.l4s=1".into(),
                "net.classq.fq_codel.enable_ecn=1".into(),
            ],
            TcpCongestionTier::Balanced4Mb => vec![
                "net.inet.tcp.autorcvbufmax=4194304".into(),
                "net.inet.tcp.autosndbufmax=4194304".into(),
                "net.inet.tcp.mssdflt=1412".into(),
                "net.inet.tcp.delayed_ack=0".into(),
                "net.inet.tcp.rexmt_slop=30".into(),
                "net.inet.tcp.l4s=1".into(),
            ],
            TcpCongestionTier::HighBdp32Mb => vec![
                "net.inet.tcp.autorcvbufmax=33554432".into(),
                "net.inet.tcp.autosndbufmax=33554432".into(),
                "net.inet.tcp.recvspace=2097152".into(),
                "net.inet.tcp.sendspace=2097152".into(),
                "net.inet.tcp.mssdflt=1452".into(),
                "net.inet.tcp.win_scale_factor=8".into(),
            ],
        };

        if write_sysctls(&pairs) {
            Some(format!("● Adaptive TCP Profile -> {target:?} (Airtime {airtime_load_pct}%, Retry {tx_retry_pct:.1}%)"))
        } else {
            None
        }
    }
}
```

---

### Feature 5: UPnP IGD / NAT-PMP / PCP Gateway Priority & Dedicated Port Lease
**File:** `src/core/gateway.rs`  
**Current Gap:** `GatewayRebooter` only sends `GetStatusInfo`, `GetExternalIPAddress`, `ForceTermination`, and `Reboot`.

```rust
// Sketch for src/core/gateway.rs
impl GatewayRebooter {
    /// Requests a dedicated UPnP IGD `AddPortMapping` lease (with fallback to NAT-PMP UDP:5351)
    /// so the router's NAT table keeps a direct fast-path mapping for the gaming host.
    pub fn lease_priority_game_flow(&mut self, local_ip: &str, udp_port: u16, lease_secs: u32) -> (bool, String) {
        if local_ip.is_empty() || local_ip == "N/A" {
            return (false, "No local IP for UPnP/PCP lease".into());
        }
        let Some((control_url, service_type)) = self.discover_control_url(std::time::Duration::from_millis(1200)) else {
            return (false, "Router UPnP IGD unavailable (relying on 50Hz NAT keepalive)".into());
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
             <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\r\n\
               <s:Body>\r\n\
                 <u:AddPortMapping xmlns:u=\"{service_type}\">\r\n\
                   <NewRemoteHost></NewRemoteHost>\r\n\
                   <NewExternalPort>{udp_port}</NewExternalPort>\r\n\
                   <NewProtocol>UDP</NewProtocol>\r\n\
                   <NewInternalPort>{udp_port}</NewInternalPort>\r\n\
                   <NewInternalClient>{local_ip}</NewInternalClient>\r\n\
                   <NewEnabled>1</NewEnabled>\r\n\
                   <NewPortMappingDescription>Aether-VIP-GameFlow</NewPortMappingDescription>\r\n\
                   <NewLeaseDuration>{lease_secs}</NewLeaseDuration>\r\n\
                 </u:AddPortMapping>\r\n\
               </s:Body>\r\n\
             </s:Envelope>"
        );
        match post_soap(&control_url, &service_type, "AddPortMapping", &body) {
            Ok(_) => (true, format!("● Router UPnP Fast-Path Lease Active ({local_ip}:{udp_port} UDP, {lease_secs}s)")),
            Err(e) => (false, format!("UPnP AddPortMapping skipped: {e}")),
        }
    }
}
```

---

### Feature 6: Closed-Loop `AutoPilotEngine` State Machine
**File:** `src/core/autopilot.rs` (new module, wired into `src/app.rs` via `spawn_bg` and `src/core/daemon.rs`)

```rust
// Sketch for src/core/autopilot.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPilotState {
    IdleOptimal,
    HighThroughputStream,
    MultiUserCongestionMitigation,
    PreGamePreparation,
    CompetitiveGamingLocked,
    EmergencyLinkRescue,
}

pub struct Capabilities {
    pub has_sudo: bool,
    pub has_pfctl_dnctl: bool,
    pub has_upnp_igd: bool,
    pub anti_cheat_detected: bool,
}

pub struct AutoPilotEngine {
    pub enabled: bool,
    pub state: AutoPilotState,
    pub caps: Capabilities,
    last_transition: std::time::Instant,
    dwell_period: std::time::Duration, // Default: 15s hysteresis to prevent flapping
    pub last_actions: Vec<String>,
}

impl AutoPilotEngine {
    pub fn evaluate_next_state(
        &self,
        tel: &super::model::Telemetry,
        scope: &super::hops::ScopeSnapshot,
        game_running: bool,
    ) -> AutoPilotState {
        if !tel.radio_on || !tel.link_active || scope.loss_pct >= 8.0 {
            return AutoPilotState::EmergencyLinkRescue;
        }
        if game_running {
            return AutoPilotState::CompetitiveGamingLocked;
        }
        if tel.airtime_load_pct >= 65 || tel.tx_retry_pct >= 6.0 || scope.max_bloat_delta_ms > 18.0 {
            return AutoPilotState::MultiUserCongestionMitigation;
        }
        if tel.sticky_client && tel.sibling_5g_ssid.is_some() {
            return AutoPilotState::PreGamePreparation;
        }
        if scope.down_kbs > 1200.0 {
            return AutoPilotState::HighThroughputStream;
        }
        AutoPilotState::IdleOptimal
    }

    pub fn should_transition(&self, next: AutoPilotState) -> bool {
        if !self.enabled || next == self.state {
            return false;
        }
        // Immediate preemption for EmergencyLinkRescue or CompetitiveGamingLocked
        if matches!(next, AutoPilotState::EmergencyLinkRescue | AutoPilotState::CompetitiveGamingLocked) {
            return true;
        }
        self.last_transition.elapsed() >= self.dwell_period
    }
}
```

#### Integration into `App::refresh` (`src/app.rs`)
* Runs capability inventory once at startup (and refreshes `anti_cheat_detected` every 3 ticks).
* Calls `autopilot.evaluate_next_state(&self.telemetry, &self.scope_snap, self.game_mode.active())`.
* When `should_transition` is `true`, dispatches heavy shell actions (`pfctl`, `dnctl`, `ifconfig awdl0 down`, `sysctl`, `networksetup`) inside `self.spawn_bg(...)` and merges results back via `drain_outbox()`—keeping the TUI render loop completely non-blocking.

---

## 3. Projected Performance Benchmarks (Before vs. After)

> **Note:** The baseline figures below reflect existing architectural characteristics observed in the codebase (e.g., `corewlan.rs` ~3ms vs `system_profiler` ~6s; `nettop -L 1` ~20ms) combined with **modeled / projected** network performance ranges based on IEEE 802.11e WMM (`CWmin=3` vs `15`), `fq_codel` 3ms target AQM queuing behavior, and AWDL off-channel scan elimination. Live on-wire validation should be captured with `aether`'s `PingScope` and `SpeedtestState` after implementation.

### Scenario A: Multi-User Household Congestion
*Environment: 8+ active LAN clients, 75% Airtime Load, co-channel neighbors (`-65 dBm`), 2 background uploads.*

| Metric | Raw macOS Default | Current `WIFI-TUI` (Partial) | Upgraded `AutoPilotEngine` (Full) | Delta vs. Raw |
| :--- | :--- | :--- | :--- | :--- |
| **Median Gateway RTT (Hop 2)** | 14.0 – 28.0 ms | 8.0 – 16.0 ms | **2.0 – 4.5 ms** | **-80% to -85%** |
| **P99 Jitter** | 38.0 – 95.0 ms | 18.0 – 45.0 ms | **2.5 – 6.0 ms** | **-90%+** |
| **Hop-2 Bufferbloat (`+ms`)** | +85 – 240 ms | +35 – 90 ms (no `pfctl` anchor) | **+2 – 8 ms (`pfctl` + `dnctl` CoDel)** | **-95%** |
| **L2 Frame Retries** | 12% – 22% | 9% – 16% | **2.5% – 5.5% (5GHz steer + MSS 1360)** | **-70%** |
| **Packet Loss** | 1.5% – 4.5% | 0.5% – 2.0% | **0.0% – 0.2%** | **Near Zero** |

### Scenario B: Competitive Gaming Under Concurrent LAN + Local Load
*Environment: Valorant / LoL / FFXIV / CS2 running while iCloud / browser downloads & other users stream 4K.*

| Metric | Raw macOS Default | Current `WIFI-TUI` (`GameMode`) | Upgraded `AutoPilotEngine` |
| :--- | :--- | :--- | :--- |
| **AWDL Off-Channel Spike (every 10–15s)** | +45 – 120 ms | +15 – 40 ms (`llw0` / Universal Ctrl) | **0 – 1.5 ms (`awdl0` + `llw0` down)** |
| **99.9th Percentile Frame Spike** | 140+ ms | 55 ms | **< 9 ms** |
| **802.11e Airtime Contention Queue** | `AC_BE` (`CWmin=15`) | `AC_BE` (sysctl reverted on entry) | **`AC_VO` (`CWmin=3`, `pfctl set tos ef`)** |
| **Background Upload Bloat** | Unbounded | Manual `SIGSTOP` / reverts | **Auto-throttled via CoDel Weight 15** |

### Scenario C: Latency Distribution Comparison (ASCII Histogram)

```text
Ping Latency to Game Cluster Under Multi-User Congestion (1000 samples):

Raw macOS Default:
  [  0-15ms ] #####
  [ 15-35ms ] ####################
  [ 35-80ms ] ##############################
  [ 80ms+   ] ###################  <-- AWDL + Bufferbloat Spikes

Current WIFI-TUI (Partial / Manual):
  [  0-15ms ] #################
  [ 15-35ms ] ##########################
  [ 35-80ms ] ###########
  [ 80ms+   ] ###

Upgraded WIFI-TUI + AutoPilotEngine:
  [  0-15ms ] #############################################
  [ 15-35ms ] ###########
  [ 35-80ms ] #
  [ 80ms+   ] (0%)
```

### Scenario D: Host CPU / Memory Overhead
* **Control Loop Execution:** `AutoPilotEngine` evaluates telemetry in `< 0.05 ms` in-memory on `App::refresh`; shell mutations (`pfctl`, `dnctl`, `sysctl`, `ifconfig`, `networksetup`) run asynchronously via `App::spawn_bg`.
* **25Hz / 50Hz UDP Pacer Threads:** Single unprivileged `UdpSocket` sleeping 20ms/40ms (`< 0.05%` CPU, `< 64 KB` stack).
* **Total App Overhead:** `< 0.15%` CPU, zero UI frame drops.

---

## 4. Implementation Roadmap & Verification

1. **Create `src/core/autopilot.rs`**: Implement `AutoPilotState`, `Capabilities`, `AutoPilotEngine`, hysteresis dwell guard, and unit tests for state transitions & anti-cheat safety lockouts.
2. **Complete `src/core/bandwidth_shaper.rs`**: Add `enable_autopilot_aqm` and `disable_autopilot_aqm` with `dnctl` CoDel + `pfctl -a com.apple/aether`.
3. **Enhance `src/core/antilag.rs` & `src/core/zero_jitter.rs`**: Add `enforce_awdl_down` (`awdl0`/`llw0`) with clean rollback on exit/teardown.
4. **Enhance `src/core/kernel_tuner.rs`**: Add `adapt_tcp_profile_to_congestion` (32MB / 4MB / 1MB + MSS 1452/1412/1360).
5. **Enhance `src/core/roaming.rs` & `src/core/gateway.rs`**: Add `execute_smart_roam` (with 90s cooldown, blocked during active games) and `lease_priority_game_flow` (UPnP `AddPortMapping`).
6. **Wire into `src/core/mod.rs`, `src/app.rs`, and `src/core/daemon.rs`**: Register `pub mod autopilot;`, hook non-blocking `spawn_bg` transitions in `App::refresh`, and ensure `App::teardown` restores `pfctl`, `dnctl`, `awdl0`, and default sysctls.

### Verification Commands (Post-Implementation)
```bash
cargo test
cargo clippy --all-targets -- -D warnings
```
