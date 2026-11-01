//! ISP diagnostic markdown ticket export. Ported from ticket_export.py.

use std::time::Duration;

use super::cmd;
use super::hops::ScopeSnapshot;
use super::model::Telemetry;

fn ticket_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(".config/wifi-tui/isp_diagnostic_ticket.md")
}

/// Returns (path, status message).
pub fn export_report(
    tel: &Telemetry,
    scope: &ScopeSnapshot,
    history_summary: Option<&str>,
) -> (String, String) {
    let ts = {
        let out = cmd::run(
            &["date", "+%Y-%m-%d %H:%M:%S %Z"],
            Duration::from_millis(500),
        );
        let s = out.stdout.trim().to_string();
        if s.is_empty() {
            "----".into()
        } else {
            s
        }
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push("# WIFI COCKPIT v6.0 — ISP & NETWORK DIAGNOSTIC PROOF TICKET".into());
    lines.push(format!("**Generated:** `{ts}`"));
    lines.push(String::new());
    lines.push("## 1. Local RF Airlink & Interface Telemetry (`en0`)".into());
    lines.push(format!(
        "- **Active SSID / Band:** `{}` (`{}`, `{}`)",
        tel.ssid,
        tel.band.label(),
        tel.phy_mode
    ));
    lines.push(format!("- **Channel & Width:** `{}`", tel.channel_label));
    lines.push(format!(
        "- **Signal / Noise / SNR:** `{} dBm` / `{} dBm` (`SNR: {} dB`)",
        tel.signal_dbm, tel.noise_dbm, tel.snr_db
    ));
    lines.push(format!(
        "- **PHY Link Rate / MCS:** `{} Mbps` (`MCS Index: {}`)",
        tel.tx_rate_mbps, tel.mcs_index
    ));
    lines.push(format!(
        "- **IPv4 -> Gateway:** `{} -> {}` (`Active MTU: {}B`)",
        tel.ipv4, tel.gateway, tel.mtu
    ));
    lines.push(format!(
        "- **Airtime Contention:** `{}`",
        tel.contention_status
    ));
    lines.push(format!(
        "- **802.11 Disconnect Log:** `{}`",
        tel.disassoc_summary
    ));
    lines.push(format!(
        "- **RF Channel Advisor:** `{}`",
        tel.channel_advisor_line
    ));
    lines.push(String::new());
    lines.push("## 2. 4-Hop Latency, Jitter & Bufferbloat Isolation Matrix".into());
    lines.push("| Hop | Target Node | IP Address | RTT (ms) | EWMA Jitter | Bufferbloat Delta | Loss % | Status |".into());
    lines.push("| :---: | :--- | :--- | :---: | :---: | :---: | :---: | :--- |".into());
    for h in &scope.hops {
        lines.push(format!(
            "| `[{}]` | {} | `{}` | `{} ms` | `{} ms` | `+{} ms` | `{}%` | `{}` |",
            h.hop, h.label, h.target, h.rtt_ms, h.jitter_ms, h.bloat_delta_ms, h.loss_pct, h.status
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "- **Isolation Verdict:** `{}`",
        scope.fault_diagnosis
    ));

    if let Some(score) = history_summary {
        lines.push(String::new());
        lines.push("## 4. Session Stability & Dead-Zone Drift Scorecard".into());
        lines.push(format!("- **Summary:** `{score}`"));
    }

    let content = lines.join("\n") + "\n";
    let path = ticket_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(&path, content) {
        Ok(_) => (
            path.display().to_string(),
            format!("[OK] Exported ISP Diagnostic Ticket -> {}", path.display()),
        ),
        Err(e) => (
            path.display().to_string(),
            format!("[!] Failed to export ticket: {e}"),
        ),
    }
}
