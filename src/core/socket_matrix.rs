//! Kernel socket table sampler + rolling RF history. Ported from socket_matrix.py.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::cmd;
use super::model::Telemetry;

#[derive(Clone)]
pub struct Sockets {
    pub established: i64,
    pub syn_sent: i64,
    pub close_wait: i64,
    pub time_wait: i64,
    pub listen: i64,
    pub https_443: i64,
    pub quic_udp_443: i64,
    pub dns_53: i64,
    pub other_ports: i64,
    pub total_active: i64,
    pub verdict: String,
}

impl Default for Sockets {
    fn default() -> Self {
        Sockets {
            established: 0,
            syn_sent: 0,
            close_wait: 0,
            time_wait: 0,
            listen: 0,
            https_443: 0,
            quic_udp_443: 0,
            dns_53: 0,
            other_ports: 0,
            total_active: 0,
            verdict: "[OK] Socket Table Clean".into(),
        }
    }
}

#[derive(Clone)]
pub struct SocketSample {
    pub signal_history: Vec<f64>,
    pub snr_history: Vec<f64>,
    pub airtime_history: Vec<f64>,
    pub retry_history: Vec<f64>,
    pub sockets: Sockets,
}

pub struct SocketMatrixEngine {
    hist_len: usize,
    signal: VecDeque<f64>,
    snr: VecDeque<f64>,
    airtime: VecDeque<f64>,
    retry: VecDeque<f64>,
    last_ts: Option<Instant>,
    cached: Sockets,
}

impl SocketMatrixEngine {
    pub fn new(hist_len: usize) -> Self {
        SocketMatrixEngine {
            hist_len,
            signal: VecDeque::new(),
            snr: VecDeque::new(),
            airtime: VecDeque::new(),
            retry: VecDeque::new(),
            last_ts: None,
            cached: Sockets::default(),
        }
    }

    pub fn sample(&mut self, tel: &Telemetry) -> SocketSample {
        push(&mut self.signal, tel.signal_dbm as f64, self.hist_len);
        push(&mut self.snr, tel.snr_db as f64, self.hist_len);
        push(
            &mut self.airtime,
            tel.airtime_load_pct as f64,
            self.hist_len,
        );
        push(&mut self.retry, tel.tx_retry_pct, self.hist_len);

        let stale = self
            .last_ts
            .map(|t| t.elapsed() >= Duration::from_secs(3))
            .unwrap_or(true);
        if stale || self.cached.total_active == 0 {
            self.cached = collect_kernel_sockets();
            self.last_ts = Some(Instant::now());
        }

        SocketSample {
            signal_history: self.signal.iter().copied().collect(),
            snr_history: self.snr.iter().copied().collect(),
            airtime_history: self.airtime.iter().copied().collect(),
            retry_history: self.retry.iter().copied().collect(),
            sockets: self.cached.clone(),
        }
    }
}

fn collect_kernel_sockets() -> Sockets {
    let out = cmd::run(
        &["lsof", "-nP", "-iTCP", "-iUDP"],
        Duration::from_millis(1500),
    );
    let mut s = Sockets::default();
    for line in out.stdout.lines().skip(1) {
        let upper = line.to_uppercase();
        if upper.contains(" TCP ") {
            if upper.contains("(ESTABLISHED)") {
                s.established += 1;
                if upper.contains(":443 ") || upper.contains(":443(") {
                    s.https_443 += 1;
                } else if upper.contains(":53 ") || upper.contains(":853 ") {
                    s.dns_53 += 1;
                } else {
                    s.other_ports += 1;
                }
            } else if upper.contains("(SYN_SENT)") || upper.contains("(SYN_RCVD)") {
                s.syn_sent += 1;
            } else if upper.contains("(CLOSE_WAIT)")
                || upper.contains("(FIN_WAIT_1)")
                || upper.contains("(FIN_WAIT_2)")
                || upper.contains("(CLOSING)")
            {
                s.close_wait += 1;
            } else if upper.contains("(TIME_WAIT)") {
                s.time_wait += 1;
            } else if upper.contains("(LISTEN)") {
                s.listen += 1;
            }
        } else if upper.contains(" UDP ") {
            if upper.contains(":443") || !upper.contains("->*") {
                s.quic_udp_443 += 1;
            }
            if upper.contains(":53") {
                s.dns_53 += 1;
            }
        }
    }
    let total = s.established + s.syn_sent + s.close_wait + s.time_wait + s.quic_udp_443;
    s.verdict = if s.syn_sent >= 4 {
        format!(
            "[!] {} Stalled SYN_SENT Handshakes (Upstream / DNS Delay)",
            s.syn_sent
        )
    } else if s.close_wait >= 8 {
        format!(
            "[!] {} Draining CLOSE_WAIT/FIN Sockets (Press [Z] to Inspect)",
            s.close_wait
        )
    } else {
        format!(
            "[OK] {} Active TCP + {} QUIC Streams (0 Stalls)",
            s.established, s.quic_udp_443
        )
    };
    s.total_active = total.max(1);
    s
}

fn push(dq: &mut VecDeque<f64>, v: f64, cap: usize) {
    if dq.len() == cap {
        dq.pop_front();
    }
    dq.push_back(v);
}
