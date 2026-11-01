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

    // Action palette overlay (press `p`).
    pub palette_active: bool,
    pub palette_filter: String,
    pub palette_cursor: usize,

    last_seen_ssid: String,
    last_seen_gw: String,
    pub stop_flag: Arc<AtomicBool>,

    /// Outbox for results produced by heavy actions running on background
    /// threads. The worker drains this every tick and merges outcomes into
    /// `App`, so the worker loop is NEVER blocked by slow shell work and stays
    /// free to process navigation (CursorUp/Down, SwitchTab) instantly.
    pub action_outbox: Arc<std::sync::Mutex<Vec<ActionOutcome>>>,
    /// Number of heavy background actions currently in flight (drives the busy
    /// spinner without blocking the loop).
    pub bg_inflight: Arc<std::sync::atomic::AtomicUsize>,
}

/// A result produced by a heavy action on a background thread, merged back into
/// `App` by the worker. Every field is optional: a task sets only what it
/// computed. `status` always carries the human-readable outcome line.
#[derive(Default)]
pub struct ActionOutcome {
    pub status: String,
    pub wan_snap: Option<WanSnapshot>,
    pub dns_results: Option<Vec<DnsResult>>,
    pub lan_data: Option<LanData>,
    pub router_status: Option<RouterStatus>,
    /// Merge-back flags so TUNE-page toggles reflect reality after an offload.
    pub kernel_wmm: Option<bool>,
    pub kernel_nodelay: Option<bool>,
    pub kernel_buffers: Option<bool>,
    pub zero_lag: Option<bool>,
    pub sync_frozen: Option<bool>,
    pub root_authorized: Option<bool>,
    pub cam_pacer: Option<bool>,
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
            palette_active: false,
            palette_filter: String::new(),
            palette_cursor: 0,
            last_seen_ssid: String::new(),
            last_seen_gw: String::new(),
            stop_flag,
            action_outbox: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            bg_inflight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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

    /// Run a heavy action's work on a background thread. `job` builds throwaway
    /// engines from the owned inputs it captured and returns an `ActionOutcome`.
    /// The worker merges the outcome on a later tick via `drain_outbox`. This is
    /// what keeps navigation responsive while a slow action runs.
    fn spawn_bg<F>(&self, job: F)
    where
        F: FnOnce() -> ActionOutcome + Send + 'static,
    {
        let outbox = std::sync::Arc::clone(&self.action_outbox);
        let inflight = std::sync::Arc::clone(&self.bg_inflight);
        inflight.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let outcome = job();
            if let Ok(mut q) = outbox.lock() {
                q.push(outcome);
            }
            inflight.fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// True while any heavy background action is in flight (drives the spinner).
    pub fn has_bg_inflight(&self) -> bool {
        self.bg_inflight.load(Ordering::SeqCst) > 0
    }

    /// Merge any completed background-action outcomes into `App`. Called by the
    /// worker each tick. Returns true if anything was merged (so the worker can
    /// republish immediately).
    pub fn drain_outbox(&mut self) -> bool {
        let outcomes: Vec<ActionOutcome> = match self.action_outbox.lock() {
            Ok(mut q) if !q.is_empty() => std::mem::take(&mut *q),
            _ => return false,
        };
        for o in outcomes {
            if !o.status.is_empty() {
                self.status_msg = o.status;
            }
            if let Some(w) = o.wan_snap {
                self.wan_snap = w;
            }
            if let Some(d) = o.dns_results {
                self.dns_results = d;
            }
            if let Some(l) = o.lan_data {
                self.lan_data = l;
            }
            if let Some(r) = o.router_status {
                self.router_status = r;
            }
            if let Some(v) = o.kernel_wmm {
                self.kernel.wmm_vip_requested = v;
            }
            if let Some(v) = o.kernel_nodelay {
                self.kernel.ffxiv_nodelay_requested = v;
            }
            if let Some(v) = o.kernel_buffers {
                self.kernel.high_buffers_requested = v;
            }
            if let Some(v) = o.zero_lag {
                self.antilag.zero_lag_active = v;
                self.profiles.set_zero_lag_mode(v);
            }
            if let Some(v) = o.sync_frozen {
                self.antilag.sync_daemons_frozen = v;
            }
            if let Some(v) = o.root_authorized {
                self.antilag.root_authorized = v;
            }
            if let Some(v) = o.cam_pacer {
                self.scope.cam_pacer_active = v;
            }
        }
        true
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
            busy: self.busy || self.is_speedtest_running() || self.has_bg_inflight(),
            pw_edit_active: self.pw_edit_active,
            pw_edit_ssid: self.pw_edit_ssid.clone(),
            pw_edit_buf: self.pw_edit_buf.clone(),
            palette_active: self.palette_active,
            palette_filter: self.palette_filter.clone(),
            palette_cursor: self.palette_cursor,
            palette_items: {
                let cat = palette_catalog();
                palette_matches(&self.palette_filter)
                    .into_iter()
                    .map(|i| (cat[i].0.to_string(), cat[i].1.to_string()))
                    .collect()
            },
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
                let iface = self.collector.interface.clone();
                self.status_msg = "Running active LAN sweep…".into();
                self.spawn_bg(move || {
                    let mut scanner = LanScanner::new(&iface);
                    let lan = scanner.scan_lan(&ip, &gw, true);
                    ActionOutcome {
                        status: format!("[OK] LAN Sweep Complete: {}", lan.summary),
                        lan_data: Some(lan),
                        ..Default::default()
                    }
                });
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
            ToggleWmm => {
                let iface = self.collector.interface.clone();
                let target = !self.kernel.wmm_vip_requested;
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    let status = kernel.toggle_wmm_vip_priority(Some(target));
                    ActionOutcome {
                        status,
                        kernel_wmm: Some(target),
                        ..Default::default()
                    }
                });
            }
            ToggleNoDelay => {
                let iface = self.collector.interface.clone();
                let target = !self.kernel.ffxiv_nodelay_requested;
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    let status = kernel.toggle_ffxiv_tcp_nodelay(Some(target));
                    ActionOutcome {
                        status,
                        kernel_nodelay: Some(target),
                        ..Default::default()
                    }
                });
            }
            Hop2Repair => {
                let gw = self.telemetry.gateway.clone();
                let iface = self.collector.interface.clone();
                self.status_msg = "Running 6-Stage Hop 2 Anti-Bloat Repair…".into();
                self.spawn_bg(move || {
                    crate::core::hop2::refresh_gateway_arp(&gw);
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    kernel.toggle_ffxiv_tcp_nodelay(Some(true));
                    kernel.toggle_wmm_vip_priority(Some(true));
                    ActionOutcome {
                        status: "Ran 6-Stage Hop 2 Anti-Bloat & De-Jitter Repair.".into(),
                        kernel_wmm: Some(true),
                        kernel_nodelay: Some(true),
                        ..Default::default()
                    }
                });
            }
            Prewarm => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Pre-warming CDN/AI/gaming edges…".into();
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    ActionOutcome {
                        status: kernel.prewarm_workload_edges(),
                        ..Default::default()
                    }
                });
            }
            Boost => {
                let iface = self.collector.interface.clone();
                let tel = self.telemetry.clone();
                self.status_msg = "Running God-Mode Boost…".into();
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let mut antilag = AntiLagEngine::new(&iface);
                    let mut dns = DnsEngine::new("Wi-Fi");
                    let (_, doc) = doctor.run_auto_doctor(Some(&mut kernel), false);
                    let boost = antilag.run_god_mode_boost(&tel, &dns, &mut kernel);
                    let dns_results = dns.race_resolvers();
                    ActionOutcome {
                        status: format!("{boost} │ {doc}"),
                        wan_snap: Some(doctor.get_snapshot()),
                        dns_results: Some(dns_results),
                        kernel_wmm: Some(kernel.wmm_vip_requested),
                        kernel_nodelay: Some(kernel.ffxiv_nodelay_requested),
                        zero_lag: Some(true),
                        sync_frozen: Some(antilag.sync_daemons_frozen),
                        cam_pacer: Some(true),
                        ..Default::default()
                    }
                });
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
                let iface = self.collector.interface.clone();
                let target_freeze = !self.antilag.sync_daemons_frozen;
                self.status_msg = "Reaping zombie sockets & toggling sync freeze…".into();
                self.spawn_bg(move || {
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let mut antilag = AntiLagEngine::new(&iface);
                    let (_, z) = doctor.reap_zombie_sockets();
                    let s = antilag.toggle_sync_daemons(target_freeze);
                    ActionOutcome {
                        status: format!("{z} │ {s}"),
                        wan_snap: Some(doctor.get_snapshot()),
                        sync_frozen: Some(antilag.sync_daemons_frozen),
                        ..Default::default()
                    }
                });
            }
            Doctor => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Running WAN & YouTube Doctor…".into();
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let (_, doc) = doctor.run_auto_doctor(Some(&mut kernel), false);
                    let mut dns = DnsEngine::new("Wi-Fi");
                    let dns_results = dns.race_resolvers();
                    ActionOutcome {
                        status: doc,
                        wan_snap: Some(doctor.get_snapshot()),
                        dns_results: Some(dns_results),
                        ..Default::default()
                    }
                });
            }
            QuicFastpath => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Toggling QUIC → HTTP/2 fast-path…".into();
                self.spawn_bg(move || {
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let (_, m) = doctor.toggle_quic_fastpath(None);
                    ActionOutcome {
                        status: m,
                        wan_snap: Some(doctor.get_snapshot()),
                        ..Default::default()
                    }
                });
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
                let iface = self.collector.interface.clone();
                self.status_msg = "Refreshing DHCP lease & DNS cache…".into();
                self.spawn_bg(move || {
                    crate::core::dns_engine::DnsEngine::flush_cache();
                    crate::core::antilag::AntiLagEngine::refresh_dhcp_and_routes(&iface);
                    ActionOutcome {
                        status: "Refreshed DHCP lease & DNS cache safely (zero radio disruption)."
                            .into(),
                        ..Default::default()
                    }
                });
            }
            FixExclamation => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Fixing macOS \"!\" mark…".into();
                self.spawn_bg(move || {
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let (_, m) = doctor.fix_macos_exclamation_mark();
                    ActionOutcome {
                        status: m,
                        wan_snap: Some(doctor.get_snapshot()),
                        ..Default::default()
                    }
                });
            }
            Ipv6Guard => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Toggling IPv6 link-local guard…".into();
                self.spawn_bg(move || {
                    let mut doctor = WanDoctor::new(&iface, "Wi-Fi");
                    let (_, m) = doctor.toggle_ipv6_guard(None);
                    ActionOutcome {
                        status: m,
                        wan_snap: Some(doctor.get_snapshot()),
                        ..Default::default()
                    }
                });
            }
            MtuClamp => {
                let iface = self.collector.interface.clone();
                let new_mtu = if self.telemetry.mtu >= 1500 {
                    1492
                } else {
                    1500
                };
                self.status_msg = format!("Clamping MTU to {new_mtu}…");
                self.spawn_bg(move || {
                    let kernel = KernelTuner::new("Wi-Fi", &iface);
                    ActionOutcome {
                        status: kernel.set_optimal_mtu(new_mtu),
                        ..Default::default()
                    }
                });
            }
            MacPrivacy => self.status_msg = self.privacy.randomize_mac(),
            SweetSpot => {
                let q = crate::core::acoustic_radar::calculate_quality_score(&self.telemetry);
                self.status_msg = format!(
                    "Sweet-Spot Locator: {} {} ({:.1} clicks/s) │ SNR {}dB MCS {}",
                    q.meter, q.rating, q.click_hz, q.snr_db, q.mcs_index
                );
            }
            RootAuth => {
                let iface = self.collector.interface.clone();
                self.status_msg = "Requesting root authorization (GUI prompt)…".into();
                self.spawn_bg(move || {
                    let mut antilag = AntiLagEngine::new(&iface);
                    let status = antilag.authorize_root_privileges();
                    ActionOutcome {
                        status,
                        root_authorized: Some(antilag.root_authorized),
                        ..Default::default()
                    }
                });
            }
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
                let gw_ip = self.telemetry.gateway.clone();
                self.status_msg = "Probing captive portal & gateway…".into();
                self.spawn_bg(move || {
                    let mut captive = CaptivePortalEngine::new("Wi-Fi");
                    let cap = captive.probe_portal(true);
                    if cap.captive {
                        let dns = DnsEngine::new("Wi-Fi");
                        ActionOutcome {
                            status: captive.breakout_captive_portal(Some(&dns)),
                            ..Default::default()
                        }
                    } else {
                        let router_bot = RouterBot::new(&gw_ip);
                        let rs = router_bot.probe_ports();
                        let status =
                            format!("Gateway ({}): {} │ {}", rs.ip, rs.summary, cap.summary);
                        ActionOutcome {
                            status,
                            router_status: Some(rs),
                            ..Default::default()
                        }
                    }
                });
            }
            RoamRescan => {
                // A full refresh touches most engines; run it inline on the
                // worker (it's a single fast refresh tick, not a 17s shell job),
                // but it still returns quickly so navigation isn't blocked long.
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
            // Action palette.
            PaletteOpen => {
                self.palette_active = true;
                self.palette_filter.clear();
                self.palette_cursor = 0;
            }
            PaletteClose => self.palette_active = false,
            PaletteChar(c) => {
                self.palette_filter.push(c);
                self.palette_cursor = 0;
            }
            PaletteBackspace => {
                self.palette_filter.pop();
                self.palette_cursor = 0;
            }
            PaletteUp => {
                self.palette_cursor = self.palette_cursor.saturating_sub(1);
            }
            PaletteDown => {
                let n = palette_matches(&self.palette_filter).len();
                if n > 0 {
                    self.palette_cursor = (self.palette_cursor + 1).min(n - 1);
                }
            }
            PaletteConfirm => {
                let matches = palette_matches(&self.palette_filter);
                if let Some(&cat_idx) = matches.get(self.palette_cursor) {
                    self.palette_active = false;
                    if let Some(a) = palette_action(cat_idx) {
                        self.apply_action(a);
                    }
                }
            }
            PaletteToggleBuffers => {
                let iface = self.collector.interface.clone();
                let target = !self.kernel.high_buffers_requested;
                self.spawn_bg(move || {
                    let mut kernel = KernelTuner::new("Wi-Fi", &iface);
                    let status = kernel.toggle_8mb_streaming_buffers(Some(target));
                    ActionOutcome {
                        status,
                        kernel_buffers: Some(target),
                        ..Default::default()
                    }
                });
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
        // Dispatch each row to its (now background-offloaded) Action so the
        // TUNE page never blocks the worker loop either. Light, instant toggles
        // (AWDL mute, CAM pacer) run inline; everything that shells out is
        // routed through apply_action -> spawn_bg.
        let label = row.label.clone();
        match label.as_str() {
            "Zero-Lag Mode" => self.apply_action(Action::ZeroLag),
            "WMM VIP Airtime Priority" => self.apply_action(Action::ToggleWmm),
            "25Hz CAM Anti-Sleep Pacer" => self.apply_action(Action::CamPacer),
            "Gaming TCP No-Delay" => self.apply_action(Action::ToggleNoDelay),
            "32MB High-BDP Buffers" => self.apply_action(Action::PaletteToggleBuffers),
            "IPv6 Link-Local Guard" => self.apply_action(Action::Ipv6Guard),
            "QUIC → HTTP/2 Fast-Path" => self.apply_action(Action::QuicFastpath),
            "Sync Daemon Freezer" => self.apply_action(Action::ZombieSync),
            "Fiber/PPPoE MTU Clamp" => self.apply_action(Action::MtuClamp),
            "God-Mode Boost" => self.apply_action(Action::Boost),
            "WAN & YouTube Doctor" => self.apply_action(Action::Doctor),
            "Fix macOS \"!\" Mark" => self.apply_action(Action::FixExclamation),
            "Root Authorization" => self.apply_action(Action::RootAuth),
            "Export Diagnostic Ticket" => self.apply_action(Action::ExportTicket),
            _ => self.status_msg = "Module updated.".into(),
        }
    }

    fn toggle_tuning_control(&mut self) {
        let _ = self; // replaced by run_selected_tune_row; kept as a no-op stub
    }
}

/// Every command exposed in the action palette: (name, description).
/// The index into this list maps to an `Action` via `palette_action()`.
pub fn palette_catalog() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "WAN & YouTube Doctor",
            "Diagnose & heal DNS, MTU, IPv6, gateway (8 stages)",
        ),
        ("God-Mode Boost", "Apply every optimization at once"),
        (
            "Toggle Zero-Lag Mode",
            "Silence AWDL/AirDrop + pause sync daemons",
        ),
        (
            "Toggle WMM VIP Priority",
            "802.11e voice/video airtime priority",
        ),
        ("Toggle CAM Pacer", "25Hz anti-sleep radio heartbeat"),
        (
            "Toggle Gaming TCP No-Delay",
            "delayed_ack=0 + fast retransmit",
        ),
        (
            "Toggle 32MB Buffers",
            "High-BDP TCP window for 4K/large transfers",
        ),
        ("Toggle IPv6 Guard", "Force clean IPv4 routing"),
        ("Toggle QUIC Fast-Path", "Force TCP HTTP/2 over UDP QUIC"),
        (
            "Hop-2 Anti-Bloat Repair",
            "Refresh gateway ARP + fast-ACK + WMM",
        ),
        (
            "Reap Zombie Sockets",
            "Flush stuck CLOSE_WAIT/FIN sockets + sync",
        ),
        (
            "Fix macOS \"!\" Mark",
            "Restore DHCP DNS + refresh Wi-Fi agent",
        ),
        (
            "Clamp MTU (1492/1500)",
            "Toggle fiber/PPPoE fragmentation clamp",
        ),
        (
            "Pre-warm CDN Edges",
            "Warm YouTube/AI/gaming TLS handshakes",
        ),
        (
            "Verify Private MAC",
            "Confirm link-safe private Wi-Fi address",
        ),
        ("Cycle Preset", "Switch to the next mission profile"),
        (
            "Toggle Auto-Pilot",
            "Let the engine auto-apply the best preset",
        ),
        (
            "Captive / Gateway Probe",
            "Check captive portal + gateway ports",
        ),
        (
            "Roam & Rescan Airspace",
            "Smart BSSID roam + spectrum rescan",
        ),
        ("Export Diagnostic Ticket", "Write a Markdown ISP report"),
        (
            "Full Speed Test",
            "Multi-stream download/upload + bufferbloat",
        ),
        ("Quick 5s Speed Burst", "Fast throughput sample"),
        ("Apple RPM Benchmark", "networkQuality responsiveness test"),
        (
            "Root Authorization",
            "Install passwordless sudoers for kernel tuning",
        ),
    ]
}

/// Map a palette catalog index to the `Action` it fires.
pub fn palette_action(idx: usize) -> Option<Action> {
    use Action::*;
    Some(match idx {
        0 => Doctor,
        1 => Boost,
        2 => ZeroLag,
        3 => ToggleWmm,
        4 => CamPacer,
        5 => ToggleNoDelay,
        6 => PaletteToggleBuffers,
        7 => Ipv6Guard,
        8 => QuicFastpath,
        9 => Hop2Repair,
        10 => ZombieSync,
        11 => FixExclamation,
        12 => MtuClamp,
        13 => Prewarm,
        14 => MacPrivacy,
        15 => CyclePreset,
        16 => AutoPilot,
        17 => CaptiveGateway,
        18 => RoamRescan,
        19 => ExportTicket,
        20 => SpeedFull,
        21 => SpeedQuick,
        22 => SpeedApple,
        23 => RootAuth,
        _ => return None,
    })
}

/// Catalog indices that match the current fuzzy filter (case-insensitive
/// subsequence on the name + description).
pub fn palette_matches(filter: &str) -> Vec<usize> {
    let f = filter.to_lowercase();
    palette_catalog()
        .iter()
        .enumerate()
        .filter(|(_, (name, desc))| {
            f.is_empty() || subseq(&f, &name.to_lowercase()) || desc.to_lowercase().contains(&f)
        })
        .map(|(i, _)| i)
        .collect()
}

/// True if `needle` is a subsequence of `hay` (fuzzy match).
fn subseq(needle: &str, hay: &str) -> bool {
    let mut h = hay.chars();
    for nc in needle.chars() {
        if nc == ' ' {
            continue;
        }
        loop {
            match h.next() {
                Some(hc) if hc == nc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
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
    // Action palette overlay.
    PaletteOpen,
    PaletteClose,
    PaletteChar(char),
    PaletteBackspace,
    PaletteUp,
    PaletteDown,
    PaletteConfirm,
    PaletteToggleBuffers,
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

    #[test]
    fn heavy_actions_return_instantly_and_offload() {
        // The whole point of the freeze fix: a heavy action must dispatch to a
        // background thread and RETURN immediately, leaving the worker loop free
        // to process navigation. We call Doctor (normally ~17s of shell work)
        // and assert apply_action itself returns in well under a frame, and that
        // it registered an in-flight background task.
        let mut app = App::new("en0", Arc::new(AtomicBool::new(false)));
        let t = std::time::Instant::now();
        app.apply_action(Action::Doctor);
        let elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
        assert!(
            elapsed_ms < 50.0,
            "apply_action(Doctor) blocked for {elapsed_ms:.1}ms — it must offload, not run inline"
        );
        assert!(
            app.has_bg_inflight(),
            "Doctor should have spawned a background task"
        );

        // Navigation actions issued while the Doctor runs must apply instantly
        // and NOT wait for the background task.
        let t2 = std::time::Instant::now();
        app.apply_action(Action::SwitchTab(Tab::Spectrum));
        app.apply_action(Action::SwitchTab(Tab::Processes));
        let nav_ms = t2.elapsed().as_secs_f64() * 1000.0;
        assert!(nav_ms < 20.0, "navigation blocked for {nav_ms:.1}ms");
        assert_eq!(app.active_tab, Tab::Processes);
    }
}
