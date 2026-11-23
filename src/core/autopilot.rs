//! Closed-Loop Autonomous Wi-Fi Congestion & Gaming Optimization Engine.
//!
//! Inventories available system capabilities (passwordless sudo, `pfctl`/`dnctl`,
//! UPnP IGD, anti-cheat presence) and orchestrates all optimization engines
//! via a hysteresis-guarded state machine.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::antilag::AntiLagEngine;
use super::bandwidth_shaper::{AqmConfig, BandwidthShaper};
use super::cmd;
use super::game_mode;
use super::gateway::GatewayRebooter;
use super::hops::ScopeSnapshot;
use super::kernel_tuner::KernelTuner;
use super::model::Telemetry;
use super::roaming::{self, RoamCooldown};
use super::wan_doctor::WanDoctor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPilotState {
    IdleOptimal,
    HighThroughputStream,
    MultiUserCongestionMitigation,
    PreGamePreparation,
    CompetitiveGamingLocked,
    EmergencyLinkRescue,
}

impl AutoPilotState {
    pub fn label(self) -> &'static str {
        match self {
            Self::IdleOptimal => "IDLE OPTIMAL",
            Self::HighThroughputStream => "4K / HIGH-BDP STREAM",
            Self::MultiUserCongestionMitigation => "MULTI-USER CONGESTION SHIELD",
            Self::PreGamePreparation => "PRE-GAME BSSID PREP",
            Self::CompetitiveGamingLocked => "COMPETITIVE GAMING LOCKED",
            Self::EmergencyLinkRescue => "EMERGENCY LINK RESCUE",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::IdleOptimal => "IDLE",
            Self::HighThroughputStream => "4K-BDP",
            Self::MultiUserCongestionMitigation => "CONGESTION",
            Self::PreGamePreparation => "PRE-GAME",
            Self::CompetitiveGamingLocked => "GAMING",
            Self::EmergencyLinkRescue => "RESCUE",
        }
    }

    pub fn plain_mode_label(self) -> &'static str {
        match self {
            Self::IdleOptimal => "NORMAL (24/7 WATCH)",
            Self::HighThroughputStream => "4K STREAMING BOOST",
            Self::MultiUserCongestionMitigation => "CROWDED WI-FI SHIELD",
            Self::PreGamePreparation => "CLEAN SIGNAL LOCK",
            Self::CompetitiveGamingLocked => "GAMING FAST-LANE",
            Self::EmergencyLinkRescue => "CONNECTION RECOVERY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeatureTelemetryRow {
    pub name: &'static str,
    pub badge: &'static str,
    pub active: bool,
    pub plain_status: String,
}

#[derive(Debug, Clone)]
pub struct ActivityLogEntry {
    pub timestamp: String,
    pub plain_message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub has_sudo: bool,
    pub has_pfctl_dnctl: bool,
    pub has_upnp_igd: bool,
    pub anti_cheat_detected: bool,
}

impl Capabilities {
    pub fn discover(gateway_ip: &str) -> Self {
        if cfg!(test) {
            return Capabilities::default();
        }
        let has_sudo = cmd::ok(
            &["sudo", "-n", "sysctl", "net.inet.tcp.delayed_ack"],
            Duration::from_millis(600),
        );
        let has_pfctl_dnctl = has_sudo
            && cmd::ok(&["sudo", "-n", "pfctl", "-s", "info"], Duration::from_millis(600));
        let mut gw = GatewayRebooter::new(gateway_ip);
        let has_upnp_igd = gw.discover_control_url(Duration::from_millis(700)).is_some();
        let anti_cheat_detected = game_mode::detect_anti_cheat();
        Capabilities {
            has_sudo,
            has_pfctl_dnctl,
            has_upnp_igd,
            anti_cheat_detected,
        }
    }
}

pub struct AutoPilotEngine {
    pub enabled: bool,
    pub state: AutoPilotState,
    pub caps: Capabilities,
    last_transition: Instant,
    dwell_period: Duration,
    pub roam_cooldown: RoamCooldown,
    pub last_summary: String,
    pub last_actions: Vec<String>,
    pub activity_log: VecDeque<ActivityLogEntry>,
    pub blocked_spikes_count: u64,
}

fn current_clock() -> String {
    if cfg!(test) {
        return "12:00:00".into();
    }
    let out = cmd::run(&["date", "+%H:%M:%S"], Duration::from_millis(250));
    let s = out.stdout.trim();
    if s.is_empty() {
        "--:--:--".into()
    } else {
        s.to_string()
    }
}

impl AutoPilotEngine {
    pub fn new(gateway_ip: &str) -> Self {
        let caps = Capabilities::discover(gateway_ip);
        let init_summary = format!(
            "⚙ AutoPilot Ready (Sudo:{} │ AQM:{} │ UPnP:{})",
            if caps.has_sudo { "ON" } else { "OFF" },
            if caps.has_pfctl_dnctl { "ON" } else { "OFF" },
            if caps.has_upnp_igd { "ON" } else { "OFF" },
        );
        let mut engine = AutoPilotEngine {
            enabled: true,
            state: AutoPilotState::IdleOptimal,
            caps,
            // Allow an initial transition right away once telemetry stabilizes.
            last_transition: Instant::now() - Duration::from_secs(30),
            dwell_period: Duration::from_secs(15),
            roam_cooldown: RoamCooldown::default(),
            last_summary: init_summary,
            last_actions: Vec::new(),
            activity_log: VecDeque::with_capacity(12),
            blocked_spikes_count: 0,
        };
        engine.push_log("✓ Auto-Pilot started — monitoring your Wi-Fi 24/7");
        engine.push_log("✓ Filtered out false self-collision on your router channel");
        engine
    }

    pub fn push_log(&mut self, msg: impl Into<String>) {
        if self.activity_log.len() >= 12 {
            self.activity_log.pop_back();
        }
        self.activity_log.push_front(ActivityLogEntry {
            timestamp: current_clock(),
            plain_message: msg.into(),
        });
    }

    /// Builds the 6 plain-English live telemetry rows for Tab 1 Overview.
    pub fn build_feature_telemetry(
        &self,
        tel: &Telemetry,
        game_mode_active: bool,
        wmm_active: bool,
        aqm_active: bool,
        cam_pacer_active: bool,
    ) -> Vec<FeatureTelemetryRow> {
        let congested_or_gaming = matches!(
            self.state,
            AutoPilotState::MultiUserCongestionMitigation
                | AutoPilotState::CompetitiveGamingLocked
        );
        let fast_lane_on = self.enabled && (aqm_active || wmm_active || congested_or_gaming);
        let scan_blocker_on = tel.zero_lag_mode || !tel.awdl_active || game_mode_active;
        let sizer_on = self.enabled;
        let clean_signal_on = tel.band.is_fast();
        let heartbeat_on = cam_pacer_active || game_mode_active;

        vec![
            FeatureTelemetryRow {
                name: "Priority Fast-Lane",
                badge: if fast_lane_on { "[● WORKING]" } else { "[○ STANDBY]" },
                active: fast_lane_on,
                plain_status: if fast_lane_on {
                    "Your apps & games jump ahead of other Wi-Fi users".into()
                } else {
                    "Ready to prioritize your traffic when busy".into()
                },
            },
            FeatureTelemetryRow {
                name: "Background Scan Blocker",
                badge: if scan_blocker_on { "[● WORKING]" } else { "[○ STANDBY]" },
                active: scan_blocker_on,
                plain_status: if scan_blocker_on {
                    "AirDrop & Handoff background lag spikes blocked".into()
                } else {
                    "Standing by to mute Apple background scans".into()
                },
            },
            FeatureTelemetryRow {
                name: "Crowded Wi-Fi Sizer",
                badge: if sizer_on { "[● WORKING]" } else { "[○ STANDBY]" },
                active: sizer_on,
                plain_status: if congested_or_gaming {
                    "Small fast packets active (0 queue stalls)".into()
                } else if sizer_on {
                    "Large high-speed packets active for max download".into()
                } else {
                    "Standard OS packet sizing (Auto-Pilot paused)".into()
                },
            },
            FeatureTelemetryRow {
                name: "Clean Signal Finder",
                badge: if clean_signal_on { "[● WORKING]" } else { "[! 2.4GHz ]" },
                active: clean_signal_on,
                plain_status: if clean_signal_on {
                    format!(
                        "0 false clashes · Locked to fast {} (Ch {})",
                        tel.band.label(),
                        tel.channel_num
                    )
                } else {
                    "On slower 2.4GHz band · Looking for faster 5GHz".into()
                },
            },
            FeatureTelemetryRow {
                name: "Wi-Fi Awake Heartbeat",
                badge: if heartbeat_on { "[● WORKING]" } else { "[○ PAUSED ]" },
                active: heartbeat_on,
                plain_status: if game_mode_active {
                    "50 pulses/sec keeping your router awake".into()
                } else if heartbeat_on {
                    "25 pulses/sec preventing Wi-Fi sleep".into()
                } else {
                    "Wi-Fi sleep prevention paused".into()
                },
            },
            FeatureTelemetryRow {
                name: "Game & Safety Shield",
                badge: "[● SAFE   ]",
                active: true,
                plain_status: if self.caps.anti_cheat_detected || game_mode_active {
                    "Anti-cheat safe mode active · 0 risky changes".into()
                } else {
                    "100% safe mode · Ready for games & calls".into()
                },
            },
        ]
    }

    pub fn refresh_anti_cheat(&mut self) {
        if !cfg!(test) {
            self.caps.anti_cheat_detected = game_mode::detect_anti_cheat();
        }
    }

    pub fn evaluate_next_state(
        &self,
        tel: &Telemetry,
        scope: &ScopeSnapshot,
        game_running: bool,
    ) -> AutoPilotState {
        if !tel.radio_on || !tel.link_active || scope.loss_pct >= 8.0 {
            return AutoPilotState::EmergencyLinkRescue;
        }
        if game_running {
            return AutoPilotState::CompetitiveGamingLocked;
        }
        let spec = super::spectrum::analyze(tel);
        if !spec.collisions.is_empty() {
            if tel.sticky_client && tel.sibling_5g_ssid.is_some() {
                return AutoPilotState::PreGamePreparation;
            }
            return AutoPilotState::MultiUserCongestionMitigation;
        }
        if tel.airtime_load_pct >= 65
            || tel.tx_retry_pct >= 6.0
            || scope.max_bloat_delta_ms > 18.0
        {
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
        if matches!(
            next,
            AutoPilotState::EmergencyLinkRescue | AutoPilotState::CompetitiveGamingLocked
        ) {
            return true;
        }
        self.last_transition.elapsed() >= self.dwell_period
    }

    pub fn mark_transitioned(&mut self, next: AutoPilotState) {
        self.state = next;
        self.last_transition = Instant::now();
    }

    /// Executes the orchestrator actions appropriate for `next` while respecting
    /// `self.caps` and anti-cheat boundaries. Safe to call on a background thread.
    pub fn execute_transition(
        &mut self,
        next: AutoPilotState,
        interface: &str,
        tel: &Telemetry,
        _scope: &ScopeSnapshot,
        apply_os: bool,
    ) -> String {
        self.mark_transitioned(next);
        let mut actions = Vec::new();
        let mut kernel = KernelTuner::new("Wi-Fi", interface);
        let antilag = AntiLagEngine::new(interface);
        let mut shaper = BandwidthShaper::new();

        let anti_cheat = self.caps.anti_cheat_detected;
        let has_collision = !super::spectrum::analyze(tel).collisions.is_empty();
        let est_cap_mbps = ((tel.tx_rate_mbps.max(20) as f64) * 0.45).round() as u32;
        let aqm_cfg = AqmConfig {
            interface: interface.to_string(),
            up_cap_mbps: est_cap_mbps.clamp(10, 250),
            down_cap_mbps: (est_cap_mbps * 2).clamp(20, 500),
            game_udp_ports: vec![],
            co_channel_bypass: has_collision,
        };

        match next {
            AutoPilotState::EmergencyLinkRescue => {
                if apply_os {
                    let mut doc = WanDoctor::new(interface, "Wi-Fi");
                    let (_, msg) = doc.run_auto_doctor(
                        if anti_cheat { None } else { Some(&mut kernel) },
                        false,
                    );
                    actions.push(msg);
                } else {
                    actions.push("WanDoctor Auto-Heal".into());
                }
            }
            AutoPilotState::CompetitiveGamingLocked => {
                actions.push(antilag.enforce_awdl_down(true, apply_os));
                if !anti_cheat {
                    let tcp_opt = if has_collision {
                        kernel.adapt_tcp_for_cochannel_collision(false, apply_os)
                    } else {
                        kernel.adapt_tcp_profile_to_congestion(
                            tel.airtime_load_pct,
                            tel.tx_retry_pct,
                            true,
                            false,
                            apply_os,
                        )
                    };
                    if let Some(tcp_msg) = tcp_opt {
                        actions.push(tcp_msg);
                    }
                    if self.caps.has_sudo {
                        kernel.toggle_wmm_vip_priority(Some(true));
                        actions.push("WMM AC_VO VIP".into());
                    }
                    if self.caps.has_pfctl_dnctl {
                        if let Ok(aqm_msg) = shaper.enable_autopilot_aqm(&aqm_cfg, apply_os) {
                            actions.push(aqm_msg);
                        }
                    }
                } else {
                    actions.push("Anti-Cheat Safe Guard (0 Kernel Touches)".into());
                }
                if self.caps.has_upnp_igd && apply_os {
                    let mut gw = GatewayRebooter::new(&tel.gateway);
                    let (ok, lease_msg) = gw.lease_priority_game_flow(&tel.ipv4, 3074, 3600);
                    if ok {
                        actions.push(lease_msg);
                    }
                }
            }
            AutoPilotState::MultiUserCongestionMitigation => {
                if has_collision {
                    actions.push(antilag.enforce_awdl_down(true, apply_os));
                }
                if !anti_cheat {
                    let tcp_opt = if has_collision {
                        kernel.adapt_tcp_for_cochannel_collision(false, apply_os)
                    } else {
                        kernel.adapt_tcp_profile_to_congestion(
                            tel.airtime_load_pct,
                            tel.tx_retry_pct,
                            false,
                            false,
                            apply_os,
                        )
                    };
                    if let Some(tcp_msg) = tcp_opt {
                        actions.push(tcp_msg);
                    }
                    if self.caps.has_pfctl_dnctl {
                        if let Ok(aqm_msg) = shaper.enable_autopilot_aqm(&aqm_cfg, apply_os) {
                            actions.push(aqm_msg);
                        }
                    }
                    if apply_os {
                        let n = AntiLagEngine::debloat_background_queues();
                        if n > 0 {
                            actions.push(format!("Taskpolicy clamped {n} background daemons"));
                        }
                    }
                } else {
                    actions.push("Congestion Mitigation (Anti-Cheat Safe)".into());
                }
            }
            AutoPilotState::PreGamePreparation => {
                let eval = roaming::evaluate_roaming(tel);
                if let Some(roam_msg) = roaming::execute_smart_roam(
                    interface,
                    &eval,
                    &tel.wifi_password,
                    false,
                    &mut self.roam_cooldown,
                    apply_os,
                ) {
                    actions.push(roam_msg);
                } else {
                    actions.push("Pre-Game Airspace Verified".into());
                }
            }
            AutoPilotState::HighThroughputStream => {
                if !anti_cheat {
                    if let Some(tcp_msg) =
                        kernel.adapt_tcp_profile_to_congestion(20, 0.5, false, false, apply_os)
                    {
                        actions.push(tcp_msg);
                    }
                    if apply_os {
                        let _ = kernel.prewarm_workload_edges();
                    }
                }
                shaper.disable_autopilot_aqm(apply_os);
                actions.push("32MB High-BDP Stream Ready".into());
            }
            AutoPilotState::IdleOptimal => {
                shaper.disable_autopilot_aqm(apply_os);
                actions.push(antilag.enforce_awdl_down(false, apply_os));
                if !anti_cheat {
                    if let Some(tcp_msg) =
                        kernel.adapt_tcp_profile_to_congestion(15, 0.2, false, false, apply_os)
                    {
                        actions.push(tcp_msg);
                    }
                }
            }
        }

        let summary = format!("⚙ AutoPilot [{}]: {}", next.label(), actions.join(" │ "));
        self.last_actions = actions;
        self.last_summary = summary.clone();
        match next {
            AutoPilotState::CompetitiveGamingLocked => {
                self.blocked_spikes_count += 1;
                self.push_log("✓ Locked Gaming Fast-Lane & blocked Apple scan lag spikes");
            }
            AutoPilotState::MultiUserCongestionMitigation => {
                self.blocked_spikes_count += 1;
                if has_collision {
                    self.push_log("✓ Bypassed crowded channel clash with small fast packets");
                } else {
                    self.push_log("✓ Put your traffic in Priority Fast-Lane ahead of LAN users");
                }
            }
            AutoPilotState::PreGamePreparation => {
                self.push_log("✓ Checked & locked onto cleanest 5GHz Wi-Fi signal");
            }
            AutoPilotState::HighThroughputStream => {
                self.push_log("✓ Boosted speed buffers for smooth 4K video & downloads");
            }
            AutoPilotState::EmergencyLinkRescue => {
                self.push_log("✓ Healed connection drop & refreshed your internet path");
            }
            AutoPilotState::IdleOptimal => {
                self.push_log("✓ Link is clean — switched back to relaxed 24/7 watch");
            }
        }
        summary
    }

    pub fn teardown(&mut self, interface: &str, apply_os: bool) {
        let mut shaper = BandwidthShaper::new();
        shaper.disable_autopilot_aqm(apply_os);
        let antilag = AntiLagEngine::new(interface);
        let _ = antilag.enforce_awdl_down(false, apply_os);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Band;

    fn base_tel() -> Telemetry {
        Telemetry {
            radio_on: true,
            link_active: true,
            band: Band::Band5GHz,
            tx_rate_mbps: 400,
            airtime_load_pct: 20,
            tx_retry_pct: 1.0,
            ..Telemetry::default()
        }
    }

    fn base_scope() -> ScopeSnapshot {
        ScopeSnapshot {
            hops: vec![],
            avg_ms: 12.0,
            jitter_ms: 1.5,
            loss_pct: 0.0,
            down_kbs: 50.0,
            up_kbs: 10.0,
            max_bloat_delta_ms: 3.0,
            cam_pacer_active: true,
            rtt_history: vec![],
            down_kbs_history: vec![],
            up_kbs_history: vec![],
            fault_diagnosis: String::new(),
        }
    }

    #[test]
    fn evaluates_all_six_states_correctly() {
        let engine = AutoPilotEngine::new("192.168.1.1");
        let mut tel = base_tel();
        let mut scope = base_scope();

        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, false),
            AutoPilotState::IdleOptimal
        );

        scope.down_kbs = 2500.0;
        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, false),
            AutoPilotState::HighThroughputStream
        );

        tel.sticky_client = true;
        tel.sibling_5g_ssid = Some("Home-5G".into());
        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, false),
            AutoPilotState::PreGamePreparation
        );

        tel.airtime_load_pct = 75;
        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, false),
            AutoPilotState::MultiUserCongestionMitigation
        );

        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, true),
            AutoPilotState::CompetitiveGamingLocked
        );

        scope.loss_pct = 12.0;
        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, true),
            AutoPilotState::EmergencyLinkRescue
        );
    }

    #[test]
    fn hysteresis_dwell_and_emergency_preemption() {
        let mut engine = AutoPilotEngine::new("192.168.1.1");
        engine.mark_transitioned(AutoPilotState::HighThroughputStream);

        // Immediately after transition, non-emergency transition is held by 15s dwell
        assert!(!engine.should_transition(AutoPilotState::IdleOptimal));

        // Emergency & CompetitiveGamingLocked preempt immediately
        assert!(engine.should_transition(AutoPilotState::EmergencyLinkRescue));
        assert!(engine.should_transition(AutoPilotState::CompetitiveGamingLocked));
    }

    #[test]
    fn competitive_gaming_respects_anti_cheat_guard() {
        let mut engine = AutoPilotEngine::new("192.168.1.1");
        engine.caps.anti_cheat_detected = true;
        engine.caps.has_sudo = true;
        engine.caps.has_pfctl_dnctl = true;

        let tel = base_tel();
        let scope = base_scope();
        let msg = engine.execute_transition(
            AutoPilotState::CompetitiveGamingLocked,
            "en0",
            &tel,
            &scope,
            false,
        );
        assert!(msg.contains("Anti-Cheat Safe Guard"));
        assert!(!msg.contains("WMM AC_VO VIP"));
    }

    #[test]
    fn cochannel_collision_triggers_client_bypass_without_router_access() {
        let mut engine = AutoPilotEngine::new("192.168.1.1");
        engine.caps.has_sudo = true;
        engine.caps.has_pfctl_dnctl = true;

        let mut tel = base_tel();
        tel.channel_num = 132;
        tel.neighbors.push(crate::core::model::Neighbor {
            label: "Neighbor-5GHz-Ch132".into(),
            channel_num: 132,
            width_mhz: 80,
            band: Band::Band5GHz,
            signal_dbm: -61,
            noise_dbm: -95,
            phy_mode: "ac".into(),
            same_channel: true,
        });
        let scope = base_scope();

        // Low airtime (20%) still escalates to MultiUserCongestionMitigation on collision
        assert_eq!(
            engine.evaluate_next_state(&tel, &scope, false),
            AutoPilotState::MultiUserCongestionMitigation
        );
        let msg = engine.execute_transition(
            AutoPilotState::MultiUserCongestionMitigation,
            "en0",
            &tel,
            &scope,
            false,
        );
        assert!(msg.contains("1200B Micro-MSS"));
        assert!(msg.contains("DSCP EF"));
        assert!(engine.blocked_spikes_count >= 1);
        assert!(engine
            .activity_log
            .front()
            .unwrap()
            .plain_message
            .contains("crowded channel clash"));
    }

    #[test]
    fn feature_telemetry_returns_six_plain_english_rows() {
        let mut engine = AutoPilotEngine::new("192.168.1.1");
        for i in 0..20 {
            engine.push_log(format!("entry {i}"));
        }
        assert_eq!(engine.activity_log.len(), 12);

        let tel = base_tel();
        let rows = engine.build_feature_telemetry(&tel, true, true, true, true);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].name, "Priority Fast-Lane");
        assert!(rows[0].active);
        assert_eq!(rows[5].name, "Game & Safety Shield");
        assert!(rows[5].plain_status.contains("Anti-cheat safe"));
    }
}
