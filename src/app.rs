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

use crate::core::speedtest::{self, SpeedtestView};
use crate::render::{HopView, RenderState};

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

    /// Icon + full label for the left nav rail (no truncation — the rail is a
    /// fixed generous width, so this replaces the old cramped top tab strip).
    pub fn nav(self) -> (&'static str, &'static str, u8) {
        match self {
            Tab::Dashboard => ("◉", "OVERVIEW", 1),
            Tab::Spectrum => ("◈", "AIRSPACE", 2),
            Tab::Processes => ("◈", "PROCESSES", 3),
            Tab::Tuning => ("⚙", "TUNE & FIX", 4),
            Tab::SpeedTest => ("⚡", "SPEED TEST", 5),
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
    /// Shared so a background speedtest thread can update it live while the
    /// worker keeps publishing snapshots (that's what animates the UI).
    pub speedtest: std::sync::Arc<std::sync::Mutex<SpeedtestState>>,

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
    pub tune_cursor: usize,
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
            speedtest: std::sync::Arc::new(std::sync::Mutex::new(SpeedtestState::default())),
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
            tune_cursor: 0,
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

    /// True while a background speedtest thread is running (worker uses this to
    /// publish on a fast cadence so the UI animates).
    pub fn is_speedtest_running(&self) -> bool {
        self.speedtest
            .lock()
            .map(|s| s.is_running.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }

    fn spawn_speedtest(&self, target: std::net::Ipv4Addr, quick: bool) {
        let shared = std::sync::Arc::clone(&self.speedtest);
        std::thread::spawn(move || {
            speedtest::run_full_suite(&shared, target, quick);
        });
    }

    fn spawn_apple_rpm(&self) {
        let shared = std::sync::Arc::clone(&self.speedtest);
        std::thread::spawn(move || {
            speedtest::run_apple_network_quality(&shared);
        });
    }

    /// Build the TUNE & FIX page rows (toggles + one-shot actions) with a
    /// plain-English label and description for each, so nothing needs a manual.
    pub fn build_tune_rows(&self) -> Vec<crate::render::TuneRow> {
        use crate::render::{TuneKind, TuneRow};
        let controls = self.preset.get_tuning_controls(
            &self.telemetry,
            &self.kernel_state,
            &self.wan_snap,
            self.antilag.root_authorized,
            self.antilag.sync_daemons_frozen,
        );
        let state_of = |id: &str| controls.iter().find(|c| c.id == id).map(|c| c.active);

        let mut rows = Vec::new();
        rows.push(TuneRow {
            kind: TuneKind::Header,
            label: "TOGGLES — persistent settings (Space to flip)".into(),
            detail: String::new(),
            state: None,
        });
        // (id, friendly label, one-line description)
        let toggles = [
            (
                "zero_lag",
                "Zero-Lag Mode",
                "Silence AWDL/AirDrop hopping & pause iCloud sync for 0ms jitter",
            ),
            (
                "wmm_vip",
                "WMM VIP Airtime Priority",
                "Put your traffic in the 802.11e voice/video priority queue",
            ),
            (
                "cam_pacer",
                "25Hz CAM Anti-Sleep Pacer",
                "Keep the Wi-Fi radio awake so it never DTIM-sleeps mid-game",
            ),
            (
                "ffxiv_nodelay",
                "Gaming TCP No-Delay",
                "delayed_ack=0 + 30ms fast retransmit for lowest input lag",
            ),
            (
                "buf_8mb",
                "32MB High-BDP Buffers",
                "Bigger TCP window so 4K/large downloads aren't capped",
            ),
            (
                "ipv6_guard",
                "IPv6 Link-Local Guard",
                "Force clean IPv4 routing when IPv6 is slow/asymmetric",
            ),
            (
                "quic_fastpath",
                "QUIC → HTTP/2 Fast-Path",
                "Bypass UDP 443 policing by forcing TCP HTTP/2",
            ),
            (
                "sync_freeze",
                "Sync Daemon Freezer",
                "Freeze background iCloud/photo sync that bloats upload",
            ),
            (
                "mtu_clamp",
                "Fiber/PPPoE MTU Clamp",
                "Clamp MTU to 1492 to stop packet fragmentation",
            ),
        ];
        for (id, label, detail) in toggles {
            rows.push(TuneRow {
                kind: TuneKind::Toggle,
                label: label.into(),
                detail: detail.into(),
                state: state_of(id),
            });
        }
        rows.push(TuneRow {
            kind: TuneKind::Header,
            label: "ONE-SHOT ACTIONS — run once (Enter/Space to run)".into(),
            detail: String::new(),
            state: None,
        });
        let actions = [
            (
                "God-Mode Boost",
                "Apply every optimization at once for the current network",
            ),
            (
                "WAN & YouTube Doctor",
                "Diagnose & heal DNS, MTU, IPv6 and gateway in 8 stages",
            ),
            (
                "Fix macOS \"!\" Mark",
                "Restore DHCP DNS & refresh the Wi-Fi agent",
            ),
            (
                "Root Authorization",
                "Install a passwordless sudoers rule to lock kernel tuning",
            ),
            (
                "Export Diagnostic Ticket",
                "Write a timestamped Markdown ISP report to ~/.config",
            ),
        ];
        for (label, detail) in actions {
            rows.push(TuneRow {
                kind: TuneKind::Run,
                label: label.into(),
                detail: detail.into(),
                state: None,
            });
        }
        rows
    }

    /// Indices of selectable (non-header) tune rows, in order.
    pub fn tune_selectable(rows: &[crate::render::TuneRow]) -> Vec<usize> {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| r.kind != crate::render::TuneKind::Header)
            .map(|(i, _)| i)
            .collect()
    }

    /// Build a cloneable render snapshot for the UI thread. Precomputes the
    /// preset controls, cockpit analysis, and visual-deck lines so the UI never
    /// calls an engine method.
    pub fn build_render(&self) -> RenderState {
        let controls = self.preset.get_tuning_controls(
            &self.telemetry,
            &self.kernel_state,
            &self.wan_snap,
            self.antilag.root_authorized,
            self.antilag.sync_daemons_frozen,
        );
        let analysis = self.preset.generate_cockpit_analysis(
            &self.telemetry,
            self.scope_snap.jitter_ms,
            self.scope_snap.max_bloat_delta_ms,
            self.scope_snap.avg_ms,
            self.scope.cam_pacer_active,
            &self.spectrum,
            &self.wan_snap,
            self.antilag.root_authorized,
        );
        let tune_rows = self.build_tune_rows();
        let speed_view;
        let speed_snapshot;
        {
            let st = self.speedtest.lock().unwrap();
            speed_view = SpeedtestView::from_state(&st);
            speed_snapshot = speedtest::snapshot(&st, &self.telemetry);
        }
        let deck_lines = crate::ui::widgets::deck_body(self, self.visual_mode, 80, 7);

        let hops = self
            .scope_snap
            .hops
            .iter()
            .map(|h| HopView {
                hop: h.hop,
                label: h.label.clone(),
                target: h.target.clone(),
                rtt_ms: h.rtt_ms,
                jitter_ms: h.jitter_ms,
                bloat_delta_ms: h.bloat_delta_ms,
                loss_pct: h.loss_pct,
                status: h.status.to_string(),
            })
            .collect();

        RenderState {
            telemetry: self.telemetry.clone(),
            scope_avg_ms: self.scope_snap.avg_ms,
            scope_jitter_ms: self.scope_snap.jitter_ms,
            scope_loss_pct: self.scope_snap.loss_pct,
            scope_down_kbs: self.scope_snap.down_kbs,
            scope_up_kbs: self.scope_snap.up_kbs,
            scope_max_bloat: self.scope_snap.max_bloat_delta_ms,
            scope_fault: self.scope_snap.fault_diagnosis.clone(),
            scope_hops: hops,
            scope_rtt_history: self.scope_snap.rtt_history.clone(),
            scope_down_history: self.scope_snap.down_kbs_history.clone(),
            scope_up_history: self.scope_snap.up_kbs_history.clone(),
            cam_pacer_active: self.scope.cam_pacer_active,
            spectrum: self.spectrum.clone(),
            wan_snap: self.wan_snap.clone(),
            kernel_state: self.kernel_state.clone(),
            dns_results: self.dns_results.clone(),
            arp_info: self.arp_info.clone(),
            router_status: self.router_status.clone(),
            lan_data: self.lan_data.clone(),
            socket_sample: self.socket_sample.clone(),
            processes: self.procmon.last.clone(),
            speedtest: speed_view,
            speedtest_snapshot: speed_snapshot,
            tuning_controls: controls,
            tune_rows,
            tune_cursor: self.tune_cursor,
            cockpit_analysis: analysis,
            preset_effective_id: self.preset.effective_preset_id,
            preset_selected_id: self.preset.selected_preset_id,
            preset_cursor_idx: self.preset.cursor_idx,
            preset_auto_reason: self.preset.auto_reason.clone(),
            preset_before_after: self.preset.before_after_summary.clone(),
            preset_detected_issues: self.preset.detected_issues.clone(),
            auto_pilot_enabled: self.profiles.auto_pilot_enabled(),
            root_authorized: self.antilag.root_authorized,
            sync_daemons_frozen: self.antilag.sync_daemons_frozen,
            deck_lines,
            visual_mode: self.visual_mode,
            active_tab: self.active_tab,
            proc_cursor: self.proc_cursor,
            status_msg: self.status_msg.clone(),
            busy: self.busy || self.is_speedtest_running(),
            pw_edit_active: self.pw_edit_active,
            pw_edit_ssid: self.pw_edit_ssid.clone(),
            pw_edit_buf: self.pw_edit_buf.clone(),
        }
    }

    /// Execute a user action. Called on the worker thread, so it may block.
    pub fn apply_action(&mut self, action: Action) {
        use Action::*;
        let tab = self.active_tab;
        match action {
            Quit => self.should_quit = true,
            SwitchTab(t) => self.active_tab = t,
            NextTab => self.active_tab = self.active_tab.next(),
            PrevTab => self.active_tab = self.active_tab.prev(),
            CursorDown => {
                if tab == Tab::Processes && !self.procmon.last.is_empty() {
                    self.proc_cursor = (self.proc_cursor + 1).min(self.procmon.last.len() - 1);
                } else if tab == Tab::Tuning {
                    let rows = self.build_tune_rows();
                    let sel = App::tune_selectable(&rows);
                    if !sel.is_empty() {
                        let pos = sel.iter().position(|&i| i == self.tune_cursor).unwrap_or(0);
                        self.tune_cursor = sel[(pos + 1) % sel.len()];
                    }
                }
            }
            CursorUp => {
                if tab == Tab::Processes {
                    self.proc_cursor = self.proc_cursor.saturating_sub(1);
                } else if tab == Tab::Tuning {
                    let rows = self.build_tune_rows();
                    let sel = App::tune_selectable(&rows);
                    if !sel.is_empty() {
                        let pos = sel.iter().position(|&i| i == self.tune_cursor).unwrap_or(0);
                        self.tune_cursor = sel[(pos + sel.len() - 1) % sel.len()];
                    }
                } else {
                    self.status_msg = self.scope.toggle_cam_pacer();
                }
            }
            CamPacer => self.status_msg = self.scope.toggle_cam_pacer(),
            VisualNext => {
                self.visual_mode = (self.visual_mode % 4) + 1;
                self.status_msg = format!(
                    "Switched Visual Telemetry Deck -> [{}]",
                    visual_name(self.visual_mode)
                );
            }
            VisualPrev => {
                self.visual_mode = if self.visual_mode == 1 {
                    4
                } else {
                    self.visual_mode - 1
                };
                self.status_msg = format!(
                    "Switched Visual Telemetry Deck -> [{}]",
                    visual_name(self.visual_mode)
                );
            }
            Confirm => self.action_confirm(),
            ResumeAll => {
                let n = self.procmon.resume_all();
                self.status_msg = format!("Resumed {n} frozen processes via SIGCONT.");
            }
            LanSweep => {
                let ip = self.telemetry.ipv4.clone();
                let gw = self.telemetry.gateway.clone();
                self.lan_data = self.lan_scanner.scan_lan(&ip, &gw, true);
                self.status_msg = format!("[OK] LAN Sweep Complete: {}", self.lan_data.summary);
            }
            SpeedFull => {
                let target = self
                    .scope
                    .isp_ip
                    .parse()
                    .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
                self.spawn_speedtest(target, false);
                self.status_msg = "Running full multi-stream speed & bufferbloat suite...".into();
            }
            SpeedQuick => {
                let target = self
                    .scope
                    .isp_ip
                    .parse()
                    .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
                self.spawn_speedtest(target, true);
                self.status_msg = "Running 5-second quick speed & bufferbloat burst...".into();
            }
            SpeedApple => {
                self.spawn_apple_rpm();
                self.status_msg = "Running Apple networkQuality RPM benchmark...".into();
            }
            CyclePreset => {
                self.preset.cycle_preset();
                self.status_msg = format!(
                    "Switched Smart Tuning Preset -> #{}",
                    self.preset.selected_preset_id
                );
            }
            ToggleWmm => self.status_msg = self.kernel.toggle_wmm_vip_priority(None),
            ToggleNoDelay => self.status_msg = self.kernel.toggle_ffxiv_tcp_nodelay(None),
            Hop2Repair => {
                let gw = self.telemetry.gateway.clone();
                crate::core::hop2::refresh_gateway_arp(&gw);
                self.kernel.toggle_ffxiv_tcp_nodelay(Some(true));
                self.kernel.toggle_wmm_vip_priority(Some(true));
                self.status_msg = "Ran 6-Stage Hop 2 Anti-Bloat & De-Jitter Repair.".into();
            }
            Prewarm => self.status_msg = self.kernel.prewarm_workload_edges(),
            Boost => {
                self.profiles.set_zero_lag_mode(true);
                self.scope.cam_pacer_active = true;
                let (_, doc) = self
                    .wan_doctor
                    .run_auto_doctor(Some(&mut self.kernel), false);
                let boost = {
                    let tel = self.telemetry.clone();
                    self.antilag
                        .run_god_mode_boost(&tel, &self.dns, &mut self.kernel)
                };
                self.dns_results = self.dns.race_resolvers();
                self.wan_snap = self.wan_doctor.get_snapshot();
                self.status_msg = format!("{boost} │ {doc}");
            }
            ZeroLag => {
                let mut tel = self.telemetry.clone();
                let m = self.antilag.toggle_awdl(Some(&mut tel));
                self.telemetry = tel;
                self.profiles
                    .set_zero_lag_mode(self.antilag.zero_lag_active);
                self.status_msg = m;
            }
            ZombieSync => {
                let (_, z) = self.wan_doctor.reap_zombie_sockets();
                let s = self
                    .antilag
                    .toggle_sync_daemons(!self.antilag.sync_daemons_frozen);
                self.status_msg = format!("{z} │ {s}");
            }
            Doctor => {
                let (_, doc) = self
                    .wan_doctor
                    .run_auto_doctor(Some(&mut self.kernel), false);
                self.dns_results = self.dns.race_resolvers();
                self.wan_snap = self.wan_doctor.get_snapshot();
                self.status_msg = doc;
            }
            QuicFastpath => {
                let (_, m) = self.wan_doctor.toggle_quic_fastpath(None);
                self.wan_snap = self.wan_doctor.get_snapshot();
                self.status_msg = m;
            }
            Shape => {
                if tab == Tab::Processes {
                    if let Some(p) = self.procmon.last.get(self.proc_cursor).cloned() {
                        self.status_msg = self.shaper.toggle_throttle_pid(p.pid, &p.name, 8);
                    }
                } else {
                    self.status_msg = "5GHz sibling lock requires networksetup association.".into();
                }
            }
            SafeRenew => {
                crate::core::dns_engine::DnsEngine::flush_cache();
                let iface = self.collector.interface.clone();
                crate::core::antilag::AntiLagEngine::refresh_dhcp_and_routes(&iface);
                self.status_msg =
                    "Refreshed DHCP lease & DNS cache safely (zero radio disruption).".into();
            }
            FixExclamation => {
                let (_, m) = self.wan_doctor.fix_macos_exclamation_mark();
                self.status_msg = m;
            }
            Ipv6Guard => {
                let (_, m) = self.wan_doctor.toggle_ipv6_guard(None);
                self.wan_snap = self.wan_doctor.get_snapshot();
                self.status_msg = m;
            }
            MtuClamp => {
                let new_mtu = if self.telemetry.mtu >= 1500 {
                    1492
                } else {
                    1500
                };
                self.status_msg = self.kernel.set_optimal_mtu(new_mtu);
            }
            MacPrivacy => self.status_msg = self.privacy.randomize_mac(),
            SweetSpot => {
                let q = crate::core::acoustic_radar::calculate_quality_score(&self.telemetry);
                self.status_msg = format!(
                    "Sweet-Spot Locator: {} {} ({:.1} clicks/s) │ SNR {}dB MCS {}",
                    q.meter, q.rating, q.click_hz, q.snr_db, q.mcs_index
                );
            }
            RootAuth => self.status_msg = self.antilag.authorize_root_privileges(),
            AutoPilot => {
                let state = self.profiles.toggle_auto_pilot();
                if state {
                    self.preset.selected_preset_id = 1;
                }
                self.status_msg = format!(
                    "Closed-Loop Smart Preset Auto-Adapt {}.",
                    if state { "ENABLED" } else { "MANUAL" }
                );
            }
            CaptiveGateway => {
                let cap = self.captive.probe_portal(true);
                if cap.captive {
                    self.status_msg = self.captive.breakout_captive_portal(Some(&self.dns));
                } else {
                    self.router_status = self.router_bot.probe_ports();
                    self.status_msg = format!(
                        "Gateway ({}): {} │ {}",
                        self.router_status.ip, self.router_status.summary, cap.summary
                    );
                }
            }
            RoamRescan => {
                self.refresh();
                self.spectrum = crate::core::spectrum::analyze(&self.telemetry);
                self.status_msg =
                    "Smart BSSID Roam Evaluation & 5GHz/2.4GHz Rescan complete.".into();
            }
            ExportTicket => {
                let hist = self.telemetry.history_summary.clone();
                let (_, msg) = crate::core::ticket::export_report(
                    &self.telemetry,
                    &self.scope_snap,
                    Some(&hist),
                );
                self.status_msg = msg;
            }
            // Password editor.
            PwOpen => {
                let mut curr = self.telemetry.ssid.clone();
                if matches!(curr.as_str(), "Scanning..." | "Disconnected" | "") {
                    curr = self
                        .telemetry
                        .saved_networks
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Wi-Fi".into());
                }
                self.pw_edit_ssid = curr.clone();
                self.pw_edit_idx = 0;
                self.pw_edit_buf = self.profiles.get_wifi_password(&curr);
                self.pw_edit_active = true;
            }
            PwCancel => {
                self.pw_edit_active = false;
                self.status_msg = "Wi-Fi Password editor closed.".into();
            }
            PwChar(c) => self.pw_edit_buf.push(c),
            PwBackspace => {
                self.pw_edit_buf.pop();
            }
            PwTab => {
                let mut saved = self.telemetry.saved_networks.clone();
                let curr = self.telemetry.ssid.clone();
                if !curr.is_empty() && !saved.contains(&curr) {
                    saved.insert(0, curr);
                }
                if !saved.is_empty() {
                    self.pw_edit_idx = (self.pw_edit_idx + 1) % saved.len();
                    self.pw_edit_ssid = saved[self.pw_edit_idx].clone();
                    self.pw_edit_buf = self.profiles.get_wifi_password(&self.pw_edit_ssid);
                }
            }
            PwSave => {
                let target = self.pw_edit_ssid.clone();
                let pw = self.pw_edit_buf.trim().to_string();
                self.pw_edit_active = false;
                self.status_msg = if pw.is_empty() {
                    self.profiles.set_wifi_password(&target, "");
                    format!("Cleared stored password for '{target}'.")
                } else {
                    self.profiles.set_wifi_password(&target, &pw)
                };
                self.telemetry.wifi_password =
                    self.profiles.get_wifi_password(&self.telemetry.ssid);
            }
            PwKeychain => {
                let target = self.pw_edit_ssid.clone();
                self.pw_edit_active = false;
                self.status_msg = match self.profiles.fetch_keychain_password(&target) {
                    Some(pw) => format!("[OK] Synced password for '{target}' from macOS Keychain: {pw}"),
                    None => format!("[!] Keychain entry not found or cancelled for '{target}'. Press [E] to type it."),
                };
            }
        }
    }

    fn action_confirm(&mut self) {
        match self.active_tab {
            Tab::Processes => {
                if let Some(p) = self.procmon.last.get(self.proc_cursor).cloned() {
                    self.status_msg = self.procmon.toggle_pause(p.pid, &p.name);
                }
            }
            Tab::Tuning => self.run_selected_tune_row(),
            Tab::SpeedTest => {
                let target = self
                    .scope
                    .isp_ip
                    .parse()
                    .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
                self.spawn_speedtest(target, false);
                self.status_msg = "Running full multi-stream speed & bufferbloat suite...".into();
            }
            _ => {}
        }
    }

    fn run_selected_tune_row(&mut self) {
        let rows = self.build_tune_rows();
        let Some(row) = rows.get(self.tune_cursor) else {
            return;
        };
        // Dispatch by the plain-English label (stable identifiers).
        self.status_msg = match row.label.as_str() {
            "Zero-Lag Mode" => {
                let mut tel = self.telemetry.clone();
                let m = self.antilag.toggle_awdl(Some(&mut tel));
                self.telemetry = tel;
                self.profiles
                    .set_zero_lag_mode(self.antilag.zero_lag_active);
                m
            }
            "WMM VIP Airtime Priority" => self.kernel.toggle_wmm_vip_priority(None),
            "25Hz CAM Anti-Sleep Pacer" => self.scope.toggle_cam_pacer(),
            "Gaming TCP No-Delay" => self.kernel.toggle_ffxiv_tcp_nodelay(None),
            "32MB High-BDP Buffers" => self.kernel.toggle_8mb_streaming_buffers(None),
            "IPv6 Link-Local Guard" => {
                let r = self.wan_doctor.toggle_ipv6_guard(None).1;
                self.wan_snap = self.wan_doctor.get_snapshot();
                r
            }
            "QUIC → HTTP/2 Fast-Path" => {
                let r = self.wan_doctor.toggle_quic_fastpath(None).1;
                self.wan_snap = self.wan_doctor.get_snapshot();
                r
            }
            "Sync Daemon Freezer" => {
                let (_, z) = self.wan_doctor.reap_zombie_sockets();
                let s = self
                    .antilag
                    .toggle_sync_daemons(!self.antilag.sync_daemons_frozen);
                format!("{z} │ {s}")
            }
            "Fiber/PPPoE MTU Clamp" => {
                let new_mtu = if self.telemetry.mtu >= 1500 {
                    1492
                } else {
                    1500
                };
                self.kernel.set_optimal_mtu(new_mtu)
            }
            "God-Mode Boost" => {
                self.profiles.set_zero_lag_mode(true);
                self.scope.cam_pacer_active = true;
                let (_, doc) = self
                    .wan_doctor
                    .run_auto_doctor(Some(&mut self.kernel), false);
                let boost = {
                    let tel = self.telemetry.clone();
                    self.antilag
                        .run_god_mode_boost(&tel, &self.dns, &mut self.kernel)
                };
                self.dns_results = self.dns.race_resolvers();
                self.wan_snap = self.wan_doctor.get_snapshot();
                format!("{boost} │ {doc}")
            }
            "WAN & YouTube Doctor" => {
                let (_, doc) = self
                    .wan_doctor
                    .run_auto_doctor(Some(&mut self.kernel), false);
                self.dns_results = self.dns.race_resolvers();
                self.wan_snap = self.wan_doctor.get_snapshot();
                doc
            }
            "Fix macOS \"!\" Mark" => self.wan_doctor.fix_macos_exclamation_mark().1,
            "Root Authorization" => self.antilag.authorize_root_privileges(),
            "Export Diagnostic Ticket" => {
                let hist = self.telemetry.history_summary.clone();
                crate::core::ticket::export_report(&self.telemetry, &self.scope_snap, Some(&hist)).1
            }
            _ => "Module updated.".into(),
        };
    }

    fn toggle_tuning_control(&mut self) {
        let _ = self; // replaced by run_selected_tune_row; kept as a no-op stub
    }
}

fn visual_name(m: u8) -> &'static str {
    match m {
        1 => "1:TOPOLOGY PIPELINE DIAGRAM",
        2 => "2:2D RF SIGNAL & SNR CHRONOGRAPH",
        3 => "3:802.11 QAM CONSTELLATION & MIMO",
        _ => "4:KERNEL TCP SOCKET & PORT MATRIX",
    }
}

/// Every user-triggered action. Lightweight UI-only ones (tab/cursor/visual) and
/// heavy blocking ones (doctor/boost/speedtest) all flow through here so they run
/// on the worker thread, never on the UI thread.
pub enum Action {
    Quit,
    SwitchTab(Tab),
    NextTab,
    PrevTab,
    CursorUp,
    CursorDown,
    CamPacer,
    VisualNext,
    VisualPrev,
    Confirm,
    ResumeAll,
    LanSweep,
    SpeedFull,
    SpeedQuick,
    SpeedApple,
    CyclePreset,
    ToggleWmm,
    ToggleNoDelay,
    Hop2Repair,
    Prewarm,
    Boost,
    ZeroLag,
    ZombieSync,
    Doctor,
    QuicFastpath,
    Shape,
    SafeRenew,
    FixExclamation,
    Ipv6Guard,
    MtuClamp,
    MacPrivacy,
    SweetSpot,
    RootAuth,
    AutoPilot,
    CaptiveGateway,
    RoamRescan,
    ExportTicket,
    PwOpen,
    PwCancel,
    PwChar(char),
    PwBackspace,
    PwTab,
    PwSave,
    PwKeychain,
}

#[cfg(test)]
mod perf_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn build_render_is_cheap_and_nonblocking() {
        // build_render must only clone already-collected data + run pure
        // computations (no ICMP/curl/networksetup). It should complete in well
        // under a frame budget even on a cold App.
        let mut app = App::new("en0", Arc::new(AtomicBool::new(false)));
        // One refresh to populate fields (this DOES do I/O — not what we measure).
        app.refresh();
        let t = std::time::Instant::now();
        for _ in 0..200 {
            let _ = app.build_render();
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / 200.0;
        // Each render-snapshot build must be far under a 16ms frame.
        assert!(per < 5.0, "build_render too slow: {per:.2} ms/call");
    }
}
