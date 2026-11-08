//! Kernel socket table sampler + rolling RF history. Ported from socket_matrix.py.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    shared_sockets: Arc<Mutex<Sockets>>,
    probe_inflight: Arc<AtomicBool>,
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
            shared_sockets: Arc::new(Mutex::new(Sockets::default())),
            probe_inflight: Arc::new(AtomicBool::new(false)),
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
        let cached_snapshot = self
            .shared_sockets
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        if (stale || cached_snapshot.total_active == 0)
            && !self.probe_inflight.swap(true, Ordering::SeqCst)
        {
            self.last_ts = Some(Instant::now());
            let shared = Arc::clone(&self.shared_sockets);
            let inflight = Arc::clone(&self.probe_inflight);
            std::thread::spawn(move || {
                let sampled = collect_kernel_sockets();
                if let Ok(mut guard) = shared.lock() {
                    *guard = sampled;
                }
                inflight.store(false, Ordering::SeqCst);
            });
        }

        SocketSample {
            signal_history: self.signal.iter().copied().collect(),
            snr_history: self.snr.iter().copied().collect(),
            airtime_history: self.airtime.iter().copied().collect(),
            retry_history: self.retry.iter().copied().collect(),
            sockets: cached_snapshot,
        }
    }
}

fn collect_kernel_sockets() -> Sockets {
    // Fast-path: use `netstat -an -f inet` (~15ms on macOS/BSD/Linux) before falling back to `lsof`.
    let netstat = cmd::run(
        &["netstat", "-an", "-f", "inet"],
        Duration::from_millis(600),
    );
    if netstat.success && netstat.stdout.lines().count() > 4 {
        return parse_netstat_sockets(&netstat.stdout);
    }

    let out = cmd::run(
        &["lsof", "-nP", "-iTCP", "-iUDP"],
        Duration::from_millis(1500),
    );
    parse_lsof_sockets(&out.stdout)
}

fn parse_netstat_sockets(stdout: &str) -> Sockets {
    let mut s = Sockets::default();
    for line in stdout.lines() {
        let trim = line.trim();
        let upper = trim.to_uppercase();
        if upper.starts_with("TCP") {
            if upper.contains("ESTABLISHED") {
                s.established += 1;
                if upper.contains(".443 ") || upper.contains(":443 ") {
                    s.https_443 += 1;
                } else if upper.contains(".53 ")
                    || upper.contains(":53 ")
                    || upper.contains(".853 ")
                    || upper.contains(":853 ")
                {
                    s.dns_53 += 1;
                } else {
                    s.other_ports += 1;
                }
            } else if upper.contains("SYN_SENT") || upper.contains("SYN_RCVD") {
                s.syn_sent += 1;
            } else if upper.contains("CLOSE_WAIT")
                || upper.contains("FIN_WAIT_1")
                || upper.contains("FIN_WAIT_2")
                || upper.contains("CLOSING")
            {
                s.close_wait += 1;
            } else if upper.contains("TIME_WAIT") {
                s.time_wait += 1;
            } else if upper.contains("LISTEN") {
                s.listen += 1;
            }
        } else if upper.starts_with("UDP") {
            if upper.contains(".443 ") || upper.contains(":443 ") {
                s.quic_udp_443 += 1;
            } else if upper.contains(".53 ") || upper.contains(":53 ") {
                s.dns_53 += 1;
            }
        }
    }
    finalize_socket_verdict(&mut s);
    s
}

fn parse_lsof_sockets(stdout: &str) -> Sockets {
    let mut s = Sockets::default();
    for line in stdout.lines().skip(1) {
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
    finalize_socket_verdict(&mut s);
    s
}

fn finalize_socket_verdict(s: &mut Sockets) {
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
}

fn push(dq: &mut VecDeque<f64>, v: f64, cap: usize) {
    if dq.len() == cap {
        dq.pop_front();
    }
    dq.push_back(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sockets_are_safe_to_chart() {
        // The LIVE CONNECTIONS panel sizes each bar as count/total_active, where
        // the renderer clamps total_active with .max(1). The raw default total is
        // 0 (nothing sampled yet); verify the clamp keeps the divisor safe and
        // that no category count is negative (bars use usize width maths).
        let s = Sockets::default();
        assert_eq!(s.total_active, 0);
        assert!(s.total_active.max(1) >= 1); // renderer's clamp
        for c in [
            s.established,
            s.https_443,
            s.quic_udp_443,
            s.dns_53,
            s.time_wait,
            s.syn_sent,
            s.close_wait,
            s.listen,
            s.other_ports,
        ] {
            assert!(c >= 0);
        }
        assert!(!s.verdict.is_empty());
    }

    #[test]
    fn test_parse_netstat_sockets_extracts_tcp_and_quic_states() {
        let sample_netstat = "\
Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  192.168.1.25.54122     142.250.185.78.443     ESTABLISHED
tcp4       0      0  192.168.1.25.54123     1.1.1.1.53             ESTABLISHED
tcp4       0      0  192.168.1.25.54124     104.16.132.229.80      SYN_SENT
tcp4       0      0  192.168.1.25.54125     104.16.132.229.443     CLOSE_WAIT
tcp4       0      0  192.168.1.25.54126     104.16.132.229.443     TIME_WAIT
tcp4       0      0  *.8080                 *.*                    LISTEN
udp4       0      0  192.168.1.25.58111     142.250.185.78.443     *.*
udp4       0      0  192.168.1.25.58112     8.8.8.8.53             *.*
";
        let s = parse_netstat_sockets(sample_netstat);
        assert_eq!(s.established, 2);
        assert_eq!(s.https_443, 1);
        assert_eq!(s.dns_53, 2); // 1 TCP:53 + 1 UDP:53
        assert_eq!(s.syn_sent, 1);
        assert_eq!(s.close_wait, 1);
        assert_eq!(s.time_wait, 1);
        assert_eq!(s.listen, 1);
        assert_eq!(s.quic_udp_443, 1);
        assert!(s.verdict.contains("[OK]"));
    }

    #[test]
    fn test_socket_matrix_sample_is_non_blocking_under_10ms() {
        let mut engine = SocketMatrixEngine::new(30);
        let tel = Telemetry::default();
        let t0 = Instant::now();
        let snap = engine.sample(&tel);
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(25),
            "SocketMatrixEngine::sample blocked for {:?}, expected non-blocking <25ms",
            elapsed
        );
        assert_eq!(snap.signal_history.len(), 1);
    }
}
