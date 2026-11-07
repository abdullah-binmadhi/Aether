//! `RenderState` — the cloneable snapshot the worker publishes for the UI thread.
//!
//! The UI thread NEVER touches an engine. It locks this snapshot, draws it, and
//! releases. Everything expensive (ICMP probes, curl, networksetup, speedtests)
//! happens on the worker thread, so keypresses and redraws are always instant.

use crate::app::Tab;
use crate::core::dns_engine::DnsResult;
use crate::core::gaming_radar::GamingData;
use crate::core::lan_scanner::LanData;
use crate::core::privacy_fortress::PrivacyData;
use crate::core::model::Telemetry;
use crate::core::preset_engine::{CockpitAnalysis, Control};
use crate::core::privacy_shield::ArpInfo;
use crate::core::procmon::ProcSample;
use crate::core::router_bot::RouterStatus;
use crate::core::socket_matrix::SocketSample;
use crate::core::spectrum::SpectrumReport;
use crate::core::speedtest::{Snapshot as SpeedSnapshot, SpeedtestView};
use crate::core::wan_doctor::WanSnapshot;
use std::collections::HashMap;

/// A full, owned snapshot of everything the UI renders. Rebuilt by the worker
/// after every refresh tick and after every action.
#[derive(Clone)]
pub struct RenderState {
    pub telemetry: Telemetry,
    pub scope_avg_ms: f64,
    pub scope_jitter_ms: f64,
    pub scope_loss_pct: f64,
    pub scope_down_kbs: f64,
    pub scope_up_kbs: f64,
    pub scope_max_bloat: f64,
    pub scope_fault: String,
    pub scope_hops: Vec<HopView>,
    pub scope_rtt_history: Vec<f64>,
    pub scope_down_history: Vec<f64>,
    pub scope_up_history: Vec<f64>,
    pub cam_pacer_active: bool,

    pub spectrum: SpectrumReport,
    pub wan_snap: WanSnapshot,
    pub kernel_state: HashMap<String, String>,
    pub dns_results: Vec<DnsResult>,
    pub arp_info: ArpInfo,
    pub router_status: RouterStatus,
    pub lan_data: LanData,
    pub socket_sample: SocketSample,
    pub processes: Vec<ProcSample>,

    pub speedtest: SpeedtestView,
    pub speedtest_snapshot: SpeedSnapshot,

    // Precomputed preset/analysis outputs (so the UI never calls engine methods).
    pub tuning_controls: Vec<Control>,
    pub tune_rows: Vec<TuneRow>,
    pub tune_cursor: usize,
    pub cockpit_analysis: CockpitAnalysis,
    pub preset_effective_id: u8,
    pub preset_selected_id: u8,
    pub preset_cursor_idx: usize,
    pub preset_auto_reason: String,
    pub preset_before_after: String,
    pub preset_detected_issues: Vec<String>,
    pub auto_pilot_enabled: bool,
    pub root_authorized: bool,
    pub sync_daemons_frozen: bool,

    // Visual deck pre-rendered body lines for the active mode.
    pub deck_lines: Vec<String>,
    pub visual_mode: u8,

    // UI chrome.
    pub active_tab: Tab,
    pub proc_cursor: usize,
    pub status_msg: String,
    pub busy: bool,

    // Password editor mirror (so the UI can draw the prompt).
    pub pw_edit_active: bool,
    pub pw_edit_ssid: String,
    pub pw_edit_buf: String,

    // Action palette overlay.
    pub palette_active: bool,
    pub palette_filter: String,
    pub palette_cursor: usize,
    pub palette_total_count: usize,
    pub palette_items: Vec<PaletteItem>,

    /// Automatic Game Mode: active flag + detected game name, for the UI badge.
    pub game_mode_active: bool,
    pub game_name: String,

    /// Tab 6: Hardware, Silicon & System Cockpit
    pub device: crate::core::device_info::DeviceInfo,

    /// Tab 7: Zero-Touch Maintenance & Device Optimizer
    pub maintenance: crate::core::maintenance::MaintenanceSnapshot,
    /// Tab 9: Gaming & Esports Ping Radar
    pub gaming_data: GamingData,
    /// Tab 10: Privacy Fortress & DNS Racer
    pub privacy_data: PrivacyData,
    pub active_theme_name: String,
}

#[derive(Clone)]
pub struct HopView {
    pub hop: u8,
    pub label: String,
    pub target: String,
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub bloat_delta_ms: f64,
    pub loss_pct: f64,
    pub status: String,
}

/// One selectable row on the TUNE & FIX page.
#[derive(Clone)]
pub struct TuneRow {
    pub kind: TuneKind,
    pub label: String,
    pub detail: String,
    /// For toggles: Some(on/off). For actions: None.
    pub state: Option<bool>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TuneKind {
    /// Section header (not selectable).
    Header,
    /// A persistent on/off setting.
    Toggle,
    /// A one-shot "RUN" action.
    Run,
}

/// Rich metadata for an action in the Action Palette overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub name: String,
    pub category: String,
    pub desc: String,
    pub details: String,
    pub impact: String,
    pub safety: String,
    pub hotkey: Option<String>,
}
