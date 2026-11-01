//! Airtime contention & TX retry-rate estimator. Ported from airtime.py.

use std::time::Instant;

use super::cmd;
use super::model::{Band, Telemetry};
use std::time::Duration;

/// Read (out_segs, retrans_segs, if_errs) from netstat. Ported from
/// `_read_tcp_retrans_and_if_errs`.
fn read_tcp_retrans_and_if_errs(interface: &str) -> (u64, u64, u64) {
    let mut out_segs = 0u64;
    let mut retrans = 0u64;
    let mut if_errs = 0u64;

    let tcp = cmd::run(&["netstat", "-s", "-p", "tcp"], Duration::from_millis(1200));
    for line in tcp.stdout.lines() {
        let s = line.trim();
        if s.contains("data packets (") && !s.contains("retransmitted") {
            if let Some(n) = s
                .split_whitespace()
                .next()
                .and_then(|t| t.parse::<u64>().ok())
            {
                out_segs = n;
            }
        } else if s.contains("data packets (") && s.contains("retransmitted") {
            if let Some(n) = s
                .split_whitespace()
                .next()
                .and_then(|t| t.parse::<u64>().ok())
            {
                retrans = n;
            }
        }
    }

    let iface = cmd::run(
        &["netstat", "-I", interface, "-b"],
        Duration::from_millis(1000),
    );
    for line in iface.stdout.lines() {
        if line.contains("<Link#") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let ierrs = parts[5].parse::<u64>().unwrap_or(0);
                let oerrs = parts[8].parse::<u64>().unwrap_or(0);
                if_errs = ierrs + oerrs;
                break;
            }
        }
    }
    (out_segs, retrans, if_errs)
}

pub struct AirtimeEngine {
    interface: String,
    prev_out: u64,
    prev_retrans: u64,
    prev_errs: u64,
    last_ts: Instant,
    pub tx_retry_pct: f64,
    pub fcs_err_delta: u64,
    pub airtime_load_pct: i64,
    pub contention_status: String,
}

pub struct AirtimeSample {
    pub tx_retry_pct: f64,
    pub airtime_load_pct: i64,
    pub contention_status: String,
}

impl AirtimeEngine {
    pub fn new(interface: &str) -> Self {
        let (out, retrans, errs) = read_tcp_retrans_and_if_errs(interface);
        AirtimeEngine {
            interface: interface.to_string(),
            prev_out: out,
            prev_retrans: retrans,
            prev_errs: errs,
            last_ts: Instant::now(),
            tx_retry_pct: 0.0,
            fcs_err_delta: 0,
            airtime_load_pct: 12,
            contention_status: "[OK] CLEAR AIRTIME (12% Load │ 0.0% Retries)".into(),
        }
    }

    pub fn sample(&mut self, tel: &Telemetry) -> AirtimeSample {
        // Retry-rate update gated to >= 1.2s.
        if self.last_ts.elapsed() >= Duration::from_millis(1200) {
            let (out_s, ret_s, errs) = read_tcp_retrans_and_if_errs(&self.interface);
            let d_out = out_s.saturating_sub(self.prev_out);
            let d_ret = ret_s.saturating_sub(self.prev_retrans);
            self.fcs_err_delta = errs.saturating_sub(self.prev_errs);
            self.prev_out = out_s;
            self.prev_retrans = ret_s;
            self.prev_errs = errs;
            self.last_ts = Instant::now();

            if d_out >= 5 {
                let raw_pct = (d_ret as f64 / (d_out + d_ret).max(1) as f64) * 100.0;
                self.tx_retry_pct = round1((0.6 * self.tx_retry_pct + 0.4 * raw_pct).min(65.0));
            } else {
                self.tx_retry_pct = round1(self.tx_retry_pct * 0.85);
            }
        }

        // Airtime contention load (every call).
        let active_ch = tel.channel_num;
        let band = tel.band;
        let noise_dbm = tel.noise_dbm;
        let mut co_channel_energy: i64 = 0;
        for net in &tel.neighbors {
            let n_ch = net.channel_num;
            let n_sig = net.signal_dbm;
            if n_ch == active_ch && active_ch > 0 {
                co_channel_energy += ((n_sig + 95) / 2).max(4);
            } else if band == Band::Band2_4GHz
                && active_ch > 0
                && n_ch > 0
                && (n_ch - active_ch).abs() <= 3
            {
                co_channel_energy += ((n_sig + 95) / 3).max(3);
            }
        }
        let noise_load = ((noise_dbm + 95) * 2).max(0);
        let retry_load = (self.tx_retry_pct * 3.2) as i64;
        let band_base = if band == Band::Band2_4GHz { 18 } else { 8 };
        let total_load = (band_base + co_channel_energy + noise_load + retry_load).clamp(5, 98);
        self.airtime_load_pct = total_load;

        self.contention_status = if self.airtime_load_pct >= 68 || self.tx_retry_pct >= 6.0 {
            format!(
                "[!] HIGH RF CONTENTION ({}% Load │ {:.1}% Retries)",
                self.airtime_load_pct, self.tx_retry_pct
            )
        } else if self.airtime_load_pct >= 42 || self.tx_retry_pct >= 2.5 {
            format!(
                "MODERATE AIRTIME ({}% Load │ {:.1}% Retries)",
                self.airtime_load_pct, self.tx_retry_pct
            )
        } else {
            format!(
                "[OK] CLEAR AIRTIME ({}% Load │ {:.1}% Retries)",
                self.airtime_load_pct, self.tx_retry_pct
            )
        };

        AirtimeSample {
            tx_retry_pct: self.tx_retry_pct,
            airtime_load_pct: self.airtime_load_pct,
            contention_status: self.contention_status.clone(),
        }
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
