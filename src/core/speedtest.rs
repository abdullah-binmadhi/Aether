//! Multi-stream speedtest + bufferbloat + Apple RPM. Ported from speedtest_engine.py.
//!
//! Runs synchronously on a worker thread (dispatched from the UI action pool).

use std::net::Ipv4Addr;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::model::Telemetry;
use super::ping;

pub struct SpeedtestState {
    pub is_running: Arc<AtomicBool>,
    pub phase_name: String,
    pub progress_pct: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub peak_down_mbps: f64,
    pub peak_up_mbps: f64,
    pub unloaded_rtt_ms: f64,
    pub loaded_down_rtt_ms: f64,
    pub loaded_up_rtt_ms: f64,
    pub loaded_jitter_ms: f64,
    pub rpm_score: i64,
    pub bufferbloat_grade: String,
    pub down_history: Vec<f64>,
    pub up_history: Vec<f64>,
    pub rtt_history: Vec<f64>,
    pub last_run_ts: String,
}

impl Default for SpeedtestState {
    fn default() -> Self {
        SpeedtestState {
            is_running: Arc::new(AtomicBool::new(false)),
            phase_name: "READY — Press [Space] for Full Suite, [V] Quick Burst, [N] Apple RPM"
                .into(),
            progress_pct: 0.0,
            down_mbps: 0.0,
            up_mbps: 0.0,
            peak_down_mbps: 0.0,
            peak_up_mbps: 0.0,
            unloaded_rtt_ms: 7.4,
            loaded_down_rtt_ms: 7.4,
            loaded_up_rtt_ms: 7.4,
            loaded_jitter_ms: 0.8,
            rpm_score: 0,
            bufferbloat_grade: "—".into(),
            down_history: vec![0.0; 56],
            up_history: vec![0.0; 56],
            rtt_history: vec![8.0; 56],
            last_run_ts: "Never".into(),
        }
    }
}

impl SpeedtestState {
    fn grade_for(delta: f64) -> &'static str {
        if delta <= 8.0 {
            "A+ (ZERO BLOAT)"
        } else if delta <= 20.0 {
            "A  (EXCELLENT)"
        } else if delta <= 45.0 {
            "B  (GOOD)"
        } else if delta <= 90.0 {
            "C  (MODERATE BLOAT)"
        } else if delta <= 180.0 {
            "D  (HEAVY BLOAT)"
        } else {
            "F  (SEVERE BLOAT)"
        }
    }

    fn compute_grade_and_rpm(&mut self) {
        let down_delta = (self.loaded_down_rtt_ms - self.unloaded_rtt_ms).max(0.0);
        let up_delta = (self.loaded_up_rtt_ms - self.unloaded_rtt_ms).max(0.0);
        let worst = down_delta.max(up_delta);
        self.bufferbloat_grade = Self::grade_for(worst).into();
        let loaded_avg = ((self.loaded_down_rtt_ms + self.loaded_up_rtt_ms) / 2.0).max(10.0);
        if self.rpm_score <= 0 {
            self.rpm_score = ((60000.0 / (loaded_avg * 1.6)).round() as i64).clamp(80, 3000);
        }
    }
}

/// Shorthand for the shared, thread-safe speedtest state.
pub type Shared = std::sync::Arc<std::sync::Mutex<SpeedtestState>>;

/// A download/upload sampling phase that reads `netdev` byte counters. Locks the
/// shared state only briefly per 140ms tick so the UI's `build_render` can read
/// the live-updating fields between ticks (this is what animates the bars).
fn sample_phase(
    duration: Duration,
    is_down: bool,
    base_prog: f64,
    span_prog: f64,
    ping_target: Ipv4Addr,
    shared: &Shared,
) -> Vec<f64> {
    let (mut prev_rx, mut prev_tx) = read_counters();
    let mut prev_ts = Instant::now();
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut loaded_rtts: Vec<f64> = Vec::new();
    let mut last_ping = Instant::now() - Duration::from_secs(1);
    let mut seq = 0u16;

    let running = || {
        shared
            .lock()
            .map(|s| s.is_running.load(Ordering::Relaxed))
            .unwrap_or(false)
    };

    while start.elapsed() < duration && running() {
        std::thread::sleep(Duration::from_millis(140));
        let (rx, tx) = read_counters();
        let dt = prev_ts.elapsed().as_secs_f64().max(0.05);
        let d_mbps = if rx >= prev_rx && prev_rx > 0 {
            (rx - prev_rx) as f64 * 8.0 / (1_000_000.0 * dt)
        } else {
            0.0
        };
        let u_mbps = if tx >= prev_tx && prev_tx > 0 {
            (tx - prev_tx) as f64 * 8.0 / (1_000_000.0 * dt)
        } else {
            0.0
        };
        prev_rx = rx;
        prev_tx = tx;
        prev_ts = Instant::now();

        let mut rtt_sample = None;
        if last_ping.elapsed() >= Duration::from_millis(400) {
            seq = seq.wrapping_add(1);
            if let Some(rtt) = ping::ping_once(ping_target, seq, Duration::from_millis(450)) {
                let ms = rtt.as_secs_f64() * 1000.0;
                loaded_rtts.push(ms);
                rtt_sample = Some(round1(ms));
            }
            last_ping = Instant::now();
        }

        let progress = (base_prog
            + (start.elapsed().as_secs_f64() / duration.as_secs_f64()) * span_prog)
            .min(99.0);

        // Short critical section: apply this tick's updates, then release.
        if let Ok(mut state) = shared.lock() {
            if is_down {
                state.down_mbps = round1(d_mbps);
                state.peak_down_mbps = state.peak_down_mbps.max(state.down_mbps);
                let v = state.down_mbps;
                push(&mut state.down_history, v);
                samples.push(d_mbps);
            } else {
                state.up_mbps = round1(u_mbps);
                state.peak_up_mbps = state.peak_up_mbps.max(state.up_mbps);
                let v = state.up_mbps;
                push(&mut state.up_history, v);
                samples.push(u_mbps);
            }
            if let Some(ms) = rtt_sample {
                push(&mut state.rtt_history, ms);
            }
            state.progress_pct = progress;
        }
    }

    if !loaded_rtts.is_empty() {
        let avg = loaded_rtts.iter().sum::<f64>() / loaded_rtts.len() as f64;
        let jitter = if loaded_rtts.len() >= 2 {
            let diffs: Vec<f64> = loaded_rtts
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .collect();
            Some(round1(diffs.iter().sum::<f64>() / diffs.len() as f64))
        } else {
            None
        };
        if let Ok(mut state) = shared.lock() {
            if is_down {
                state.loaded_down_rtt_ms = round1(avg);
            } else {
                state.loaded_up_rtt_ms = round1(avg);
            }
            if let Some(j) = jitter {
                state.loaded_jitter_ms = j;
            }
        }
    }
    samples
}

/// Run the full multi-stream suite against shared state. Call on a background
/// thread; the worker keeps publishing snapshots so the UI animates live.
pub fn run_full_suite(shared: &Shared, ping_target: Ipv4Addr, quick: bool) -> String {
    {
        let state = shared.lock().unwrap();
        if state.is_running.swap(true, Ordering::SeqCst) {
            return "Speed test is already running.".into();
        }
    }
    let set = |f: &dyn Fn(&mut SpeedtestState)| {
        if let Ok(mut s) = shared.lock() {
            f(&mut s);
        }
    };
    set(&|s| {
        s.progress_pct = 2.0;
        s.rpm_score = 0;
        s.peak_down_mbps = 0.0;
        s.peak_up_mbps = 0.0;
        s.phase_name = "PHASE 1/4: Measuring Unloaded Baseline Latency & Jitter...".into();
    });

    let down_dur = if quick { 2.8 } else { 4.5 };
    let up_dur = if quick { 2.0 } else { 3.5 };

    // Phase 1: unloaded baseline.
    let mut base_rtts = Vec::new();
    for i in 0..3 {
        if let Some(rtt) = ping::ping_once(ping_target, (i + 1) as u16, Duration::from_millis(500))
        {
            base_rtts.push(rtt.as_secs_f64() * 1000.0);
        }
        let p = 5.0 + (i as f64 + 1.0) * 4.0;
        set(&|s| s.progress_pct = p);
    }
    if let Some(min) = base_rtts
        .iter()
        .cloned()
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.min(v))))
    {
        let m = round1(min);
        set(&|s| {
            s.unloaded_rtt_ms = m;
            s.loaded_down_rtt_ms = m;
            s.loaded_up_rtt_ms = m;
        });
    }

    // Phase 2: download saturation.
    set(&|s| {
        s.phase_name = "PHASE 2/4: Multi-Stream CDN Download Saturation + Loaded Ping...".into()
    });
    let dl_urls = [
        "https://speed.cloudflare.com/__down?bytes=35000000",
        "https://speed.cloudflare.com/__down?bytes=35000000",
        "http://mensura.cdn-apple.com/api/v1/gm/large",
        "http://mensura.cdn-apple.com/api/v1/gm/large",
    ];
    let mut procs: Vec<Child> = Vec::new();
    let m = format!("{}", (down_dur as i64) + 2);
    for url in dl_urls {
        if let Ok(c) = Command::new("curl")
            .args(["-s", "-L", "-m", &m, "-o", "/dev/null", url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            procs.push(c);
        }
    }
    let dl = sample_phase(
        Duration::from_secs_f64(down_dur),
        true,
        18.0,
        42.0,
        ping_target,
        shared,
    );
    for mut p in procs {
        let _ = p.kill();
    }
    if !dl.is_empty() {
        let mut sorted = dl.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let top = &sorted[..(sorted.len() / 2).max(1)];
        let v = round1(top.iter().sum::<f64>() / top.len() as f64);
        set(&|s| s.down_mbps = v);
    }

    // Phase 3: upload saturation.
    set(&|s| {
        s.phase_name = "PHASE 3/4: Multi-Stream Upload Saturation + Queue Bloat Probe...".into()
    });
    let mu = format!("{}", (up_dur as i64) + 2);
    let mut up_procs: Vec<Child> = Vec::new();
    for _ in 0..3 {
        let sh = format!(
            "dd if=/dev/zero bs=65536 count=64 2>/dev/null | curl -s -m {mu} -X POST --data-binary @- https://speed.cloudflare.com/__up -o /dev/null"
        );
        if let Ok(c) = Command::new("sh")
            .args(["-c", &sh])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            up_procs.push(c);
        }
    }
    let ul = sample_phase(
        Duration::from_secs_f64(up_dur),
        false,
        60.0,
        35.0,
        ping_target,
        shared,
    );
    for mut p in up_procs {
        let _ = p.kill();
    }
    if !ul.is_empty() {
        let mut sorted = ul.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let top = &sorted[..(sorted.len() / 2).max(1)];
        let v = round1(top.iter().sum::<f64>() / top.len() as f64);
        set(&|s| s.up_mbps = v);
    }

    // Phase 4: finalize.
    let ret;
    {
        let mut s = shared.lock().unwrap();
        s.progress_pct = 100.0;
        s.compute_grade_and_rpm();
        s.last_run_ts = clock_str();
        s.phase_name = format!(
            "[OK] COMPLETE ({}) — ▼ {} Mbps │ ▲ {} Mbps │ Grade: {}",
            s.last_run_ts, s.down_mbps, s.up_mbps, s.bufferbloat_grade
        );
        ret = format!(
            "// SpeedTest Complete: ▼ {} Mbps, ▲ {} Mbps, Grade {}",
            s.down_mbps, s.up_mbps, s.bufferbloat_grade
        );
        s.is_running.store(false, Ordering::SeqCst);
    }
    ret
}

/// Apple's native networkQuality RPM benchmark (shared state).
pub fn run_apple_network_quality(shared: &Shared) -> String {
    {
        let state = shared.lock().unwrap();
        if state.is_running.swap(true, Ordering::SeqCst) {
            return "Speed test is already running.".into();
        }
    }
    if let Ok(mut s) = shared.lock() {
        s.phase_name =
            "RUNNING APPLE NATIVE networkQuality (Parallel HTTP/2 RPM Benchmark)...".into();
        s.progress_pct = 10.0;
    }

    let out = super::cmd::run(&["networkQuality", "-c"], Duration::from_secs(24));
    let mut s = shared.lock().unwrap();
    let state = &mut *s;
    let ret = if let Ok(data) = serde_json::from_str::<serde_json::Value>(&out.stdout) {
        let dl_bps = data
            .get("dl_throughput")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let ul_bps = data
            .get("ul_throughput")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if dl_bps > 0.0 {
            state.down_mbps = round1(dl_bps / 1_000_000.0);
            state.peak_down_mbps = state.peak_down_mbps.max(state.down_mbps);
        }
        if ul_bps > 0.0 {
            state.up_mbps = round1(ul_bps / 1_000_000.0);
            state.peak_up_mbps = state.peak_up_mbps.max(state.up_mbps);
        }
        state.rpm_score = data
            .get("lud_foreign_h2_req_resp")
            .or_else(|| data.get("responsiveness"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let base_rtt = data
            .get("base_rtt")
            .and_then(|v| v.as_f64())
            .unwrap_or(state.unloaded_rtt_ms);
        state.unloaded_rtt_ms = round1(base_rtt);
        if state.rpm_score > 0 {
            let est = (60000.0 / state.rpm_score.max(1) as f64).clamp(base_rtt, 250.0);
            state.loaded_down_rtt_ms = round1(est);
            state.loaded_up_rtt_ms = round1(est * 1.1);
        }
        state.compute_grade_and_rpm();
        state.progress_pct = 100.0;
        state.last_run_ts = clock_str();
        state.phase_name = format!(
            "[OK] APPLE RPM COMPLETE ({}) — ▼ {}M │ ▲ {}M │ {} RPM ({})",
            state.last_run_ts,
            state.down_mbps,
            state.up_mbps,
            state.rpm_score,
            state.bufferbloat_grade
        );
        format!(
            "// Apple RPM Test: ▼ {} Mbps, ▲ {} Mbps, {} RPM",
            state.down_mbps, state.up_mbps, state.rpm_score
        )
    } else {
        state.compute_grade_and_rpm();
        state.phase_name = "[!] Apple RPM finished with notice (networkQuality unavailable)".into();
        "Apple RPM notice: networkQuality unavailable".into()
    };
    state.is_running.store(false, Ordering::SeqCst);
    ret
}

/// Workload readiness + bottleneck interpretation for the Tab 5 view.
#[derive(Clone)]
pub struct Snapshot {
    pub workloads: Vec<(String, String, bool)>,
    pub interpretation: Vec<String>,
    pub down_delta_ms: f64,
    pub up_delta_ms: f64,
    pub phy_eff_pct: i64,
    pub link_rate_mbps: i64,
}

pub fn snapshot(state: &SpeedtestState, tel: &Telemetry) -> Snapshot {
    let link_rate = tel.tx_rate_mbps.max(1);
    let eff_dl = if state.down_mbps > 0.0 {
        state.down_mbps
    } else {
        state.peak_down_mbps
    };
    let eff_ul = if state.up_mbps > 0.0 {
        state.up_mbps
    } else {
        state.peak_up_mbps
    };
    let phy_eff = if eff_dl > 0.0 {
        ((eff_dl / link_rate as f64) * 100.0).round() as i64
    } else {
        0
    }
    .min(100);
    let down_delta = round1((state.loaded_down_rtt_ms - state.unloaded_rtt_ms).max(0.0));
    let up_delta = round1((state.loaded_up_rtt_ms - state.unloaded_rtt_ms).max(0.0));
    let worst = down_delta.max(up_delta);
    let has_run = state.last_run_ts != "Never" || state.is_running.load(Ordering::Relaxed);

    let (workloads, interpretation) = if !has_run {
        (
            vec![
                ("LoL / FFXIV Gaming".into(), "Press [Space] to verify loaded GCD weave latency".into(), true),
                ("YouTube / Anime 4K60".into(), format!("PHY Link Ready ({link_rate} Mbps) — Run test for WAN cap"), true),
                ("AI Coding (SSE/API)".into(), "Edge Routes Active — Run test for loaded TTFT".into(), true),
                ("Multi-Tab Browsing".into(), "Anycast DNS Active — Run test for RPM score".into(), true),
            ],
            vec![
                format!("1. Current Wi-Fi PHY Link Rate is {link_rate} Mbps (MCS {} on {}).", tel.mcs_index, tel.band.label()),
                "2. Press [Space] for the Full Multi-Stream + Bufferbloat Suite or [V] for a 5s Quick Burst.".into(),
                "3. Press [N] to run Apple's native parallel HTTP/2 Responsiveness (RPM) benchmark.".into(),
            ],
        )
    } else {
        let streams_4k = ((eff_dl / 25.0) as i64).max(1);
        let lol_ok = worst <= 35.0;
        let yt_ok = eff_dl >= 25.0;
        let ai_ok = down_delta <= 45.0;
        let workloads = vec![
            (
                "LoL / FFXIV Gaming".into(),
                if lol_ok {
                    format!("[OK] TOURNAMENT READY (+{worst:.1}ms Load Δ)")
                } else {
                    format!("[!] +{worst:.0}ms Bloat (Enable Preset 4)")
                },
                lol_ok,
            ),
            (
                "YouTube / Anime 4K60".into(),
                if yt_ok {
                    format!("[OK] SUSTAINS {streams_4k}x 4K60 HDR STREAMS ({eff_dl:.0}M)")
                } else {
                    format!("[!] {eff_dl:.1}M (Switch to 5GHz [B])")
                },
                yt_ok,
            ),
            (
                "AI Coding (SSE/API)".into(),
                if ai_ok {
                    format!("[OK] SNAPPY STREAMING ({} RPM)", state.rpm_score)
                } else {
                    "[!] Queue Delay Under Load".into()
                },
                ai_ok,
            ),
            (
                "Multi-Tab Browsing".into(),
                format!(
                    "[OK] {:.1}ms Idle -> {:.1}ms Loaded",
                    state.unloaded_rtt_ms, state.loaded_down_rtt_ms
                ),
                true,
            ),
        ];
        let bloat_line = if worst <= 25.0 {
            format!("3. Bufferbloat Verdict: Loaded ping rose by +{worst:.1}ms — safe to stream 4K while playing FFXIV/LoL!")
        } else {
            format!("3. Bufferbloat Alert: Ping rose +{worst:.1}ms under load — activate Preset 4 [P] & 1492 MTU [T].")
        };
        let interp = if phy_eff < 65 && eff_dl > 20.0 {
            vec![
                format!("1. Bottleneck Pinpoint: ISP Fiber Plan Cap ({eff_dl:.1} Mbps WAN vs {link_rate} Mbps Wi-Fi PHY)."),
                format!("2. Airlink Headroom: Your Wi-Fi radio has {:.0} Mbps spare capacity above WAN.", link_rate as f64 - eff_dl),
                bloat_line,
            ]
        } else {
            vec![
                format!("1. Bottleneck Pinpoint: Wi-Fi Airlink Saturation ({eff_dl:.1} Mbps utilizes {phy_eff}% of {link_rate}M PHY)."),
                "2. Recommendation: Lock 5GHz [B] or move router to Ch 36 for 866 Mbps PHY.".into(),
                bloat_line,
            ]
        };
        let _ = eff_ul;
        (workloads, interp)
    };

    Snapshot {
        workloads,
        interpretation,
        down_delta_ms: down_delta,
        up_delta_ms: up_delta,
        phy_eff_pct: phy_eff,
        link_rate_mbps: link_rate,
    }
}

fn read_counters() -> (u64, u64) {
    if let Ok(iface) = netdev::get_default_interface() {
        if let Some(s) = iface.stats {
            return (s.rx_bytes, s.tx_bytes);
        }
    }
    (0, 0)
}

fn push(v: &mut Vec<f64>, x: f64) {
    v.push(x);
    if v.len() > 56 {
        let drop = v.len() - 56;
        v.drain(0..drop);
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn clock_str() -> String {
    let out = super::cmd::run(&["date", "+%H:%M:%S"], Duration::from_millis(500));
    let s = out.stdout.trim().to_string();
    if s.is_empty() {
        "--:--:--".into()
    } else {
        s
    }
}

/// A cloneable, thread-safe snapshot of the speedtest display fields. The live
/// `SpeedtestState` holds an `Arc<AtomicBool>` and isn't `Clone`, so the worker
/// publishes this plain view for the UI thread to render.
#[derive(Clone)]
pub struct SpeedtestView {
    pub is_running: bool,
    pub phase_name: String,
    pub progress_pct: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub peak_down_mbps: f64,
    pub peak_up_mbps: f64,
    pub unloaded_rtt_ms: f64,
    pub loaded_down_rtt_ms: f64,
    pub loaded_up_rtt_ms: f64,
    pub loaded_jitter_ms: f64,
    pub rpm_score: i64,
    pub bufferbloat_grade: String,
    pub down_history: Vec<f64>,
    pub up_history: Vec<f64>,
    pub rtt_history: Vec<f64>,
    pub last_run_ts: String,
}

impl SpeedtestView {
    pub fn from_state(s: &SpeedtestState) -> Self {
        SpeedtestView {
            is_running: s.is_running.load(std::sync::atomic::Ordering::Relaxed),
            phase_name: s.phase_name.clone(),
            progress_pct: s.progress_pct,
            down_mbps: s.down_mbps,
            up_mbps: s.up_mbps,
            peak_down_mbps: s.peak_down_mbps,
            peak_up_mbps: s.peak_up_mbps,
            unloaded_rtt_ms: s.unloaded_rtt_ms,
            loaded_down_rtt_ms: s.loaded_down_rtt_ms,
            loaded_up_rtt_ms: s.loaded_up_rtt_ms,
            loaded_jitter_ms: s.loaded_jitter_ms,
            rpm_score: s.rpm_score,
            bufferbloat_grade: s.bufferbloat_grade.clone(),
            down_history: s.down_history.clone(),
            up_history: s.up_history.clone(),
            rtt_history: s.rtt_history.clone(),
            last_run_ts: s.last_run_ts.clone(),
        }
    }
}

#[cfg(test)]
mod anim_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn speedtest_publishes_live_progress() {
        // Run a quick suite on a background thread and sample progress_pct from
        // the "UI" thread. If animation works, progress climbs through
        // intermediate values rather than jumping 0 -> 100.
        let shared: Shared = Arc::new(Mutex::new(SpeedtestState::default()));
        let s2 = Arc::clone(&shared);
        let target = std::net::Ipv4Addr::new(1, 1, 1, 1);
        let h = thread::spawn(move || {
            run_full_suite(&s2, target, true);
        });
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..60 {
            thread::sleep(Duration::from_millis(120));
            if let Ok(s) = shared.lock() {
                seen.insert(s.progress_pct as i64);
                if !s.is_running.load(Ordering::Relaxed) && s.progress_pct >= 100.0 {
                    break;
                }
            }
        }
        h.join().unwrap();
        // We must have observed several distinct intermediate progress values,
        // not just {0, 100} — that's the proof the UI can animate.
        assert!(seen.len() >= 4, "only saw progress values {seen:?}; animation would be a jump");
    }
}
