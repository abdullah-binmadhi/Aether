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
        }
    }
}
