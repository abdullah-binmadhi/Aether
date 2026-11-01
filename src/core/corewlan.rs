//! CoreWLAN bindings — the ONLY module permitted to use `unsafe`.
//!
//! The entire crate denies `unsafe_code` (see Cargo.toml `[lints]`). This module
//! re-allows it because every CoreWLAN accessor is declared `unsafe fn` by the
//! `objc2-core-wlan` bindings. The unsafety is purely the Objective-C FFI calling
//! convention; the calls themselves are read-only property reads and are safe to
//! make on any thread that holds an autorelease pool.
//!
//! This replaces the Python original's `system_profiler SPAirPortDataType`
//! subprocess (~6 seconds, measured) with direct framework reads (~3 ms, measured).
#![allow(unsafe_code)]

use objc2_core_wlan::{CWInterface, CWWiFiClient};

use super::model::{Band, Neighbor};

/// Live RF values read directly from the driver. `None` fields mean the driver
/// returned no value (e.g. SSID is masked without Location permission on macOS 14+).
///
/// Some fields (interface_name, bssid_present, service_active) are captured for
/// completeness/diagnostics even though the current UI does not surface them yet.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RfRead {
    pub interface_name: Option<String>,
    pub ssid: Option<String>,
    pub bssid_present: bool,
    pub hardware_address: Option<String>,
    pub power_on: bool,
    pub service_active: bool,
    pub rssi_dbm: i64,
    pub noise_dbm: i64,
    pub tx_rate_mbps: i64,
    pub transmit_power_mw: i64,
    pub phy_mode_raw: isize,
    pub security_raw: isize,
    pub channel_num: i64,
    pub channel_band: Band,
    pub channel_width_mhz: i64,
    pub neighbors: Vec<Neighbor>,
}

/// Translate CoreWLAN's `CWPHYMode` raw value to an 802.11 letter label.
pub fn phy_mode_label(raw: isize) -> &'static str {
    match raw {
        1 => "802.11a",
        2 => "802.11b",
        3 => "802.11g",
        4 => "802.11n",
        5 => "802.11ac",
        6 => "802.11ax",
        _ => "N/A",
    }
}

/// Translate CoreWLAN's `CWSecurity` raw value to a human label.
pub fn security_label(raw: isize) -> &'static str {
    match raw {
        0 => "None (Open)",
        1 => "WEP",
        2 => "WPA Personal",
        3 => "WPA/WPA2 Personal",
        4 => "WPA2 Personal",
        5 => "Personal",
        6 => "Dynamic WEP",
        7 => "WPA Enterprise",
        8 => "WPA/WPA2 Enterprise",
        9 => "WPA2 Enterprise",
        10 => "Enterprise",
        11 => "WPA3 Personal",
        12 => "WPA3 Enterprise",
        13 => "WPA3 Transition",
        14 => "OWE",
        _ => "Unknown",
    }
}

fn width_from_cw_raw(raw: isize) -> i64 {
    match raw {
        1 => 20,
        2 => 40,
        3 => 80,
        4 => 160,
        _ => 0,
    }
}

/// Read the current Wi-Fi RF state via CoreWLAN. Returns `None` only if there is
/// no default Wi-Fi interface at all.
pub fn read_rf() -> Option<RfRead> {
    // SAFETY: all calls below are read-only CoreWLAN property accessors. They are
    // `unsafe fn` solely because they cross the Objective-C FFI boundary. We hold
    // retained references for the duration of each access and never send them
    // across threads.
    unsafe {
        let client = CWWiFiClient::sharedWiFiClient();
        let iface: objc2::rc::Retained<CWInterface> = client.interface()?;

        let mut out = RfRead {
            interface_name: iface.interfaceName().map(|s| s.to_string()),
            ssid: iface.ssid().map(|s| s.to_string()),
            bssid_present: iface.bssid().is_some(),
            hardware_address: iface.hardwareAddress().map(|s| s.to_string()),
            power_on: iface.powerOn(),
            service_active: iface.serviceActive(),
            rssi_dbm: iface.rssiValue() as i64,
            noise_dbm: iface.noiseMeasurement() as i64,
            tx_rate_mbps: iface.transmitRate() as i64,
            transmit_power_mw: iface.transmitPower() as i64,
            phy_mode_raw: iface.activePHYMode().0,
            security_raw: iface.security().0,
            ..RfRead::default()
        };

        if let Some(ch) = iface.wlanChannel() {
            out.channel_num = ch.channelNumber() as i64;
            out.channel_band = Band::from_cw_raw(ch.channelBand().0);
            out.channel_width_mhz = width_from_cw_raw(ch.channelWidth().0);
        }

        // Passive neighbour list from the driver's cached scan results. This does
        // not trigger a new active scan (no radio disruption).
        if let Some(set) = iface.cachedScanResults() {
            let active_ch = out.channel_num;
            for (idx, net) in set.to_vec().iter().enumerate() {
                let (channel_num, band, width_mhz) = match net.wlanChannel() {
                    Some(c) => (
                        c.channelNumber() as i64,
                        Band::from_cw_raw(c.channelBand().0),
                        width_from_cw_raw(c.channelWidth().0),
                    ),
                    None => (0, Band::Unknown, 20),
                };
                // macOS masks neighbour SSIDs too; synthesise a stable RF label.
                let label = match net.ssid().map(|s| s.to_string()) {
                    Some(s) if !s.is_empty() => s,
                    _ => format!("Neighbor-{}-Ch{} #{}", band.label(), channel_num, idx + 1),
                };
                out.neighbors.push(Neighbor {
                    label,
                    channel_num,
                    width_mhz,
                    band,
                    signal_dbm: net.rssiValue() as i64,
                    noise_dbm: net.noiseMeasurement() as i64,
                    phy_mode: "scan".into(),
                    same_channel: channel_num == active_ch && active_ch > 0,
                });
            }
        }

        Some(out)
    }
}
