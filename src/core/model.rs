//! Strongly-typed telemetry model.
//!
//! The Python original passed a `Dict[str, Any]` everywhere, which let typos
//! like reading a key no writer ever set fail silently. Here every field is a
//! real typed field, so the compiler rejects those mistakes.

use serde::Serialize;

/// Wi-Fi frequency band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[allow(clippy::enum_variant_names)] // "Band" prefix reads clearly at call sites
pub enum Band {
    Band2_4GHz,
    Band5GHz,
    Band6GHz,
    Offline,
    #[default]
    Unknown,
}

impl Band {
    pub fn label(self) -> &'static str {
        match self {
            Band::Band2_4GHz => "2.4GHz",
            Band::Band5GHz => "5GHz",
            Band::Band6GHz => "6GHz",
            Band::Offline => "OFFLINE",
            Band::Unknown => "N/A",
        }
    }

    /// Fast bands get the "LOCK" treatment in the UI; 2.4 is flagged "SLOW".
    pub fn is_fast(self) -> bool {
        matches!(self, Band::Band5GHz | Band::Band6GHz)
    }

    /// Map a CoreWLAN `CWChannelBand` raw value (1=2.4, 2=5, 3=6).
    pub fn from_cw_raw(raw: isize) -> Band {
        match raw {
            1 => Band::Band2_4GHz,
            2 => Band::Band5GHz,
            3 => Band::Band6GHz,
            _ => Band::Unknown,
        }
    }
}

/// Link health rating buckets, matching the original score thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HealthRating {
    Optimal,
    Good,
    Degraded,
    Critical,
    Offline,
}

impl HealthRating {
    pub fn label(self) -> &'static str {
        match self {
            HealthRating::Optimal => "OPTIMAL",
            HealthRating::Good => "GOOD",
            HealthRating::Degraded => "DEGRADED",
            HealthRating::Critical => "CRITICAL",
            HealthRating::Offline => "OFFLINE",
        }
    }
}

/// A single neighbouring access point from a passive scan.
#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub label: String,
    pub channel_num: i64,
    pub width_mhz: i64,
    pub band: Band,
    pub signal_dbm: i64,
    pub noise_dbm: i64,
    pub phy_mode: String,
    /// True if this AP shares our active channel (co-channel collision candidate).
    pub same_channel: bool,
}

/// Full RF + interface snapshot. Produced by the collectors, consumed by the UI.
#[derive(Debug, Clone, Serialize)]
pub struct Telemetry {
    // Identity (may be masked by macOS when Location permission is absent).
    pub ssid: String,
    pub ssid_available: bool,
    pub band: Band,
    pub channel_num: i64,
    pub channel_label: String,
    pub width_mhz: i64,
    pub phy_mode: String,
    pub security: String,

    // RF physics.
    pub signal_dbm: i64,
    pub noise_dbm: i64,
    pub snr_db: i64,
    pub tx_rate_mbps: i64,
    pub mcs_index: i64,
    pub transmit_power_mw: i64,

    // Interface / link.
    pub interface: String,
    pub ipv4: String,
    pub ipv6_active: bool,
    pub mac: String,
    pub mtu: i64,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub radio_on: bool,
    pub link_active: bool,
    pub awdl_active: bool,
    pub is_hotspot: bool,

    // Derived health.
    pub health_score: i64,
    pub health_rating: HealthRating,
    pub alerts: Vec<String>,

    // Neighbours.
    pub neighbors: Vec<Neighbor>,

    // --- Enrichment fields populated by the background workers ---
    // These mirror the Python telemetry dict keys the engines write each tick.
    pub sibling_5g_ssid: Option<String>,
    pub sibling_24g_ssid: Option<String>,
    pub saved_networks: Vec<String>,
    pub wifi_password: String,
    pub cam_pacer_active: bool,
    pub zero_lag_mode: bool,

    // Airtime (airtime.rs).
    pub tx_retry_pct: f64,
    pub airtime_load_pct: i64,
    pub contention_status: String,

    // Roaming (roaming.rs).
    pub sticky_client: bool,
    pub roam_verdict: String,

    // Disassoc (disassoc.rs).
    pub disassoc_summary: String,

    // Hotspot armor (hotspot.rs).
    pub hotspot_armor_active: bool,

    // Hop 2 repair (hop2.rs).
    pub hop2_anomaly: bool,
    pub hop2_diagnosis: String,
    pub hop2_rtt_ms: f64,
    pub hop2_bloat_ms: f64,
    pub hop2_jitter_ms: f64,

    // Channel advisor (channel_advisor.rs).
    pub channel_advisor_line: String,

    // History scorecard (history.rs).
    pub history_summary: String,

    // Security (security.rs).
    pub security_score: i64,
    pub security_level: String,
    pub security_threats: Vec<String>,

    // Captive portal (captive.rs).
    pub captive_portal: bool,
    pub captive_summary: String,

    // macOS warning badges (collector.rs tick%6).
    pub macos_weak_security: bool,
    pub macos_private_mac: bool,
    pub apple_probe_ok: bool,
    pub apple_probe_detail: String,

    // Orchestrator tier last executed.
    pub orchestrator_tier: i64,
}

impl Default for Telemetry {
    fn default() -> Self {
        Telemetry {
            ssid: "Scanning...".into(),
            ssid_available: false,
            band: Band::Unknown,
            channel_num: 0,
            channel_label: "N/A".into(),
            width_mhz: 20,
            phy_mode: "N/A".into(),
            security: "N/A".into(),
            signal_dbm: -90,
            noise_dbm: -95,
            snr_db: 0,
            tx_rate_mbps: 0,
            mcs_index: 0,
            transmit_power_mw: 0,
            interface: "en0".into(),
            ipv4: "N/A".into(),
            ipv6_active: false,
            mac: "N/A".into(),
            mtu: 1500,
            gateway: "N/A".into(),
            dns_servers: Vec::new(),
            radio_on: true,
            link_active: false,
            awdl_active: false,
            is_hotspot: false,
            health_score: 0,
            health_rating: HealthRating::Offline,
            alerts: Vec::new(),
            neighbors: Vec::new(),
            sibling_5g_ssid: None,
            sibling_24g_ssid: None,
            saved_networks: Vec::new(),
            wifi_password: String::new(),
            cam_pacer_active: true,
            zero_lag_mode: false,
            tx_retry_pct: 0.0,
            airtime_load_pct: 12,
            contention_status: "[OK] CLEAR AIRTIME (12% Load │ 0.0% Retries)".into(),
            sticky_client: false,
            roam_verdict: "[OK] AP Lock Optimal".into(),
            disassoc_summary: "[OK] 0 Unplanned Drops (Link Stable)".into(),
            hotspot_armor_active: false,
            hop2_anomaly: false,
            hop2_diagnosis: "Clean".into(),
            hop2_rtt_ms: 0.0,
            hop2_bloat_ms: 0.0,
            hop2_jitter_ms: 0.0,
            channel_advisor_line: String::new(),
            history_summary: "Session Drift: 0 Dead-Zone Drops │ Grade A+".into(),
            security_score: 100,
            security_level: "CLEAN".into(),
            security_threats: Vec::new(),
            captive_portal: false,
            captive_summary: "[OK] Direct WAN (No Captive Portal)".into(),
            macos_weak_security: false,
            macos_private_mac: true,
            apple_probe_ok: true,
            apple_probe_detail: String::new(),
            orchestrator_tier: 0,
        }
    }
}
