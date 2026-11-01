//! Application state: owns every engine, runs the enrichment pipeline, and
//! dispatches keybinding actions. This is the Rust analogue of the Python
//! `WifiCockpitApp` plus its two background workers, collapsed into a single
//! refresh tick driven by the main loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::core::airtime::AirtimeEngine;
use crate::core::antilag::AntiLagEngine;
use crate::core::bandwidth_shaper::BandwidthShaper;
use crate::core::captive::CaptivePortalEngine;
use crate::core::channel_advisor;
use crate::core::collector::Collector;
use crate::core::disassoc::DisassocTracker;
use crate::core::dns_engine::{DnsEngine, DnsResult};
use crate::core::history::HistoryLogger;
use crate::core::hop2;
use crate::core::hops::ScopeSnapshot;
use crate::core::hotspot::HotspotEngine;
use crate::core::kernel_tuner::KernelTuner;
use crate::core::lan_scanner::{LanData, LanScanner};
use crate::core::model::Telemetry;
use crate::core::orchestrator::Orchestrator;
use crate::core::preset_engine::PresetEngine;
use crate::core::privacy_shield::{ArpInfo, PrivacyShield};
use crate::core::procmon::ProcMonitor;
use crate::core::profiles::ProfileManager;
use crate::core::roaming;
use crate::core::router_bot::{RouterBot, RouterStatus};
use crate::core::security_guard::SecurityGuard;
use crate::core::socket_matrix::{SocketMatrixEngine, SocketSample};
use crate::core::spectrum::{self, SpectrumReport};
use crate::core::speedtest::SpeedtestState;
use crate::core::wan_doctor::{WanDoctor, WanSnapshot};
use crate::core::PingScope;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard = 1,
    Spectrum = 2,
    Processes = 3,
    Tuning = 4,
    SpeedTest = 5,
}

impl Tab {
    pub fn all() -> [Tab; 5] {
        [
            Tab::Dashboard,
            Tab::Spectrum,
            Tab::Processes,
            Tab::Tuning,
            Tab::SpeedTest,
        ]
    }
    pub fn short(self, w: u16) -> &'static str {
        // Mirror get_responsive_tabs width breakpoints (wide >=122, medium >=96).
        match (self, w) {
            (Tab::Dashboard, 122..) => "[1:COCKPIT]",
            (Tab::Spectrum, 122..) => "[2:SPECTRUM]",
            (Tab::Processes, 122..) => "[3:FIREWALL]",
            (Tab::Tuning, 122..) => "[4:TUNING]",
            (Tab::SpeedTest, 122..) => "[5:SPEEDTEST]",
            (Tab::Dashboard, 96..) => "[1:COCK]",
            (Tab::Spectrum, 96..) => "[2:SPEC]",
            (Tab::Processes, 96..) => "[3:FIRE]",
            (Tab::Tuning, 96..) => "[4:TUNE]",
            (Tab::SpeedTest, 96..) => "[5:SPEED]",
            (Tab::Dashboard, _) => "[1:CKP]",
            (Tab::Spectrum, _) => "[2:SPC]",
            (Tab::Processes, _) => "[3:FWL]",
            (Tab::Tuning, _) => "[4:TUN]",
            (Tab::SpeedTest, _) => "[5:SPD]",
        }
    }
    pub fn next(self) -> Tab {
        match self {
            Tab::Dashboard => Tab::Spectrum,
            Tab::Spectrum => Tab::Processes,
            Tab::Processes => Tab::Tuning,
            Tab::Tuning => Tab::SpeedTest,
            Tab::SpeedTest => Tab::Dashboard,
        }
    }
    pub fn prev(self) -> Tab {
        match self {
            Tab::Dashboard => Tab::SpeedTest,
            Tab::Spectrum => Tab::Dashboard,
            Tab::Processes => Tab::Spectrum,
            Tab::Tuning => Tab::Processes,
            Tab::SpeedTest => Tab::Tuning,
        }
    }
}

pub struct App {
    // Engines.
    pub collector: Collector,
    pub scope: PingScope,
    pub procmon: ProcMonitor,
    pub dns: DnsEngine,
    pub kernel: KernelTuner,
    pub privacy: PrivacyShield,
    pub antilag: AntiLagEngine,
    pub profiles: ProfileManager,
    pub wan_doctor: WanDoctor,
    pub airtime: AirtimeEngine,
    pub disassoc: DisassocTracker,
    pub hotspot: HotspotEngine,
    pub captive: CaptivePortalEngine,
    pub lan_scanner: LanScanner,
    pub shaper: BandwidthShaper,
    pub history: HistoryLogger,
    pub socket_matrix: SocketMatrixEngine,
    pub security: SecurityGuard,
    pub router_bot: RouterBot,
    pub orchestrator: Orchestrator,
    pub preset: PresetEngine,
    pub speedtest: SpeedtestState,

    // Snapshots / derived data for the UI.
    pub telemetry: Telemetry,
    pub scope_snap: ScopeSnapshot,
    pub spectrum: SpectrumReport,
    pub wan_snap: WanSnapshot,
    pub kernel_state: HashMap<String, String>,
    pub dns_results: Vec<DnsResult>,
    pub arp_info: ArpInfo,
    pub router_status: RouterStatus,
    pub lan_data: LanData,
    pub socket_sample: SocketSample,

    // UI state.
    pub active_tab: Tab,
    pub proc_cursor: usize,
    pub visual_mode: u8,
    pub status_msg: String,
    pub should_quit: bool,
    pub tick: u64,
    pub busy: bool,

    // Password editor sub-mode.
    pub pw_edit_active: bool,
    pub pw_edit_ssid: String,
    pub pw_edit_buf: String,
    pub pw_edit_idx: usize,

    last_seen_ssid: String,
    last_seen_gw: String,
    pub stop_flag: Arc<AtomicBool>,
}

impl App {
    pub fn new(interface: &str, stop_flag: Arc<AtomicBool>) -> Self {
        let mut collector = Collector::new(interface);
        let scope = PingScope::new(interface);
        let telemetry = collector.collect(0.0, 0.0);
        let scope_snap = scope.empty_snapshot();
        let spectrum = spectrum::analyze(&telemetry);
        let kernel = KernelTuner::new("Wi-Fi", interface);
        let kernel_state = kernel.get_state();
        let mut privacy = PrivacyShield::new(interface);
        let arp_info = privacy.check_arp_integrity(&telemetry.gateway);
        let router_bot = RouterBot::new(&scope.gateway_ip);
        let router_status = router_bot.probe_ports();

        App {
            collector,
            scope,
            procmon: ProcMonitor::new(),
            dns: DnsEngine::new("Wi-Fi"),
            kernel,
            privacy,
            antilag: AntiLagEngine::new(interface),
            profiles: ProfileManager::new(),
            wan_doctor: WanDoctor::new(interface, "Wi-Fi"),
            airtime: AirtimeEngine::new(interface),
            disassoc: DisassocTracker::new(interface),
            hotspot: HotspotEngine::new(),
            captive: CaptivePortalEngine::new("Wi-Fi"),
            lan_scanner: LanScanner::new(interface),
            shaper: BandwidthShaper::new(),
            history: HistoryLogger::new(),
            socket_matrix: SocketMatrixEngine::new(60),
            security: SecurityGuard::new(interface),
            router_bot,
            orchestrator: Orchestrator::new(interface, "Wi-Fi"),
            preset: PresetEngine::new(),
            speedtest: SpeedtestState::default(),
            telemetry,
            scope_snap,
            spectrum,
            wan_snap: WanSnapshot::default(),
            kernel_state,
            dns_results: Vec::new(),
            arp_info,
            router_status,
            lan_data: LanData {
                devices: Vec::new(),
                count: 0,
                ip_conflict: false,
                summary: "LAN Ready".into(),
            },
            socket_sample: SocketSample {
                signal_history: Vec::new(),
                snr_history: Vec::new(),
                airtime_history: Vec::new(),
                retry_history: Vec::new(),
                sockets: Default::default(),
            },
            active_tab: Tab::Dashboard,
            proc_cursor: 0,
            visual_mode: 1,
            status_msg: "Ready. [Tab] switch view  [D] Doctor  [G] Boost  [5] SpeedTest  [Q] quit"
                .into(),
            should_quit: false,
            tick: 0,
            busy: false,
            pw_edit_active: false,
            pw_edit_ssid: String::new(),
            pw_edit_buf: String::new(),
            pw_edit_idx: 0,
            last_seen_ssid: String::new(),
            last_seen_gw: String::new(),
            stop_flag,
        }
    }

    /// One refresh tick. Mirrors the Python fast worker, including tick%N gates.
    pub fn refresh(&mut self) {
        self.tick += 1;
        self.scope_snap = self.scope.probe();
        self.telemetry = self
            .collector
            .collect(self.scope_snap.avg_ms, self.scope_snap.loss_pct);
        self.enrich();

        // Heavier work on a cadence, matching the original tick gates.
        if self.tick % 4 == 1 {
            self.spectrum = spectrum::analyze(&self.telemetry);
            self.router_status = self.router_bot.probe_ports();
        }
        if self.tick % 6 == 2 {
            self.kernel_state = self.kernel.get_state();
            self.arp_info = self.privacy.check_arp_integrity(&self.telemetry.gateway);
        }
        if self.tick % 8 == 3 {
            let rep = self.security.run_security_audit(&self.telemetry);
            self.telemetry.security_score = rep.security_score;
            self.telemetry.security_level = rep.threat_level;
            self.telemetry.security_threats = rep.threats;
        }
        if self.active_tab == Tab::Processes {
            self.procmon.sample();
            let n = self.procmon.last.len();
            self.proc_cursor = if n == 0 {
                0
            } else {
                self.proc_cursor.min(n - 1)
            };
        }
    }

    /// Fill all enrichment telemetry fields from the lightweight engines.
    fn enrich(&mut self) {
        let tel = &mut self.telemetry;
        tel.cam_pacer_active = self.scope.cam_pacer_active;
        tel.zero_lag_mode = self.antilag.zero_lag_active;
        tel.wifi_password = self.profiles.get_wifi_password(&tel.ssid);

        let air = self.airtime.sample(tel);
        tel.tx_retry_pct = air.tx_retry_pct;
        tel.airtime_load_pct = air.airtime_load_pct;
        tel.contention_status = air.contention_status;

        let roam = roaming::evaluate_roaming(tel);
        tel.sticky_client = roam.sticky_client;
        tel.roam_verdict = roam.verdict;

        tel.disassoc_summary = self.disassoc.check_recent_drops(false);
        tel.is_hotspot = HotspotEngine::is_cellular_hotspot(tel);
        if let Some(msg) = self.hotspot.evaluate_auto_armor(
            tel,
            self.profiles.auto_pilot_enabled(),
            Some(&mut self.antilag),
        ) {
            self.status_msg = msg;
        }
        tel.hotspot_armor_active = self.hotspot.armor_active;

        let h2 = hop2::detect_anomaly(&self.scope_snap);
        tel.hop2_anomaly = h2.anomaly;
        tel.hop2_diagnosis = h2.diagnosis;
        tel.hop2_rtt_ms = h2.rtt_ms;
        tel.hop2_bloat_ms = h2.bloat_ms;
        tel.hop2_jitter_ms = h2.jitter_ms;

        tel.channel_advisor_line = channel_advisor::advise(tel).advice_line;
        self.socket_sample = self.socket_matrix.sample(tel);

        self.history.record_sample(
            tel,
            self.scope_snap.avg_ms,
            self.scope_snap.jitter_ms,
            self.scope_snap.loss_pct,
            self.scope_snap.max_bloat_delta_ms,
            5.0,
        );
        tel.history_summary = self.history.scorecard();

        if self.tick % 4 == 1 {
            self.lan_data = self.lan_scanner.scan_lan(&tel.ipv4, &tel.gateway, false);
        }

        // SSID/GW change -> reload.
        let curr_ssid = tel.ssid.clone();
        let curr_gw = tel.gateway.clone();
        if !matches!(curr_ssid.as_str(), "Scanning..." | "Disconnected" | "")
            && (curr_ssid != self.last_seen_ssid || curr_gw != self.last_seen_gw)
        {
            let prev = self.last_seen_ssid.clone();
            self.last_seen_ssid = curr_ssid.clone();
            self.last_seen_gw = curr_gw.clone();
            if !prev.is_empty() && prev != curr_ssid {
                self.status_msg = format!(
                    "Switched to '{curr_ssid}' (GW {curr_gw}) — Dynamic topology & profile loaded."
                );
            }
        }
        if !curr_gw.is_empty() && curr_gw != "N/A" {
            self.router_bot.router_ip = curr_gw;
        }

        // Preset auto-adapt (background, non-mutating of the link).
        let rec = self.preset.detect_issues_and_recommend(
            tel,
            self.scope_snap.avg_ms,
            self.scope_snap.jitter_ms,
            self.scope_snap.loss_pct,
            self.scope_snap.max_bloat_delta_ms,
            self.scope_snap.up_kbs,
            self.scope_snap.down_kbs,
            &self.arp_info.status,
            &self.wan_snap,
        );
        // When Auto-Adapt preset (#1) is selected, the effective preset follows
        // the recommendation; otherwise it stays on the manual selection.
        self.preset.effective_preset_id = if self.preset.selected_preset_id == 1 {
            rec
        } else {
            self.preset.selected_preset_id
        };
        tel.orchestrator_tier = self.orchestrator.last_tier_executed as i64;
    }

    pub fn signalled_to_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    pub fn teardown(&mut self) {
        let resumed = self.procmon.resume_all();
        if resumed > 0 {
            self.status_msg = format!("Resumed {resumed} frozen process(es) on exit.");
        }
    }
}
