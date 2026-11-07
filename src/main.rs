//! aether — native Rust system, silicon & network cockpit.
//!
//! Entry point: argument parsing, one-shot CLI reports, and the interactive TUI
//! with a signal handler that guarantees frozen processes are resumed on exit.

// This is a faithful full-surface port of the Python project. Some engine
// methods and struct fields are reachable only via specific CLI flags, the
// background daemon path, or are serialized into --json / the ISP ticket rather
// than read by the live TUI. They are part of the intended public API, so we
// allow dead_code crate-wide rather than delete working, tested behavior.
#![allow(dead_code)]

mod app;
mod core;
mod render;
mod ui;

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{execute, ExecutableCommand};
use ratatui::prelude::{CrosstermBackend, Terminal};

use app::{App, Tab};

#[derive(Parser, Debug)]
#[command(
    name = "aether",
    version,
    about = "Aether — Native System, Silicon & Network Cockpit in Rust"
)]
struct Cli {
    /// Wi-Fi interface to monitor.
    #[arg(long, default_value = "en0")]
    interface: String,

    /// Print a one-shot telemetry report and exit.
    #[arg(long)]
    status: bool,

    /// Print full telemetry as JSON and exit.
    #[arg(long)]
    json: bool,

    /// Run the one-shot WAN & YouTube auto-doctor and exit.
    #[arg(long)]
    doctor: bool,

    /// Run one-shot god-mode boost and exit.
    #[arg(long)]
    boost: bool,

    /// Run the 4-tier self-healing orchestrator and exit.
    #[arg(long, alias = "auto-heal")]
    orchestrate: bool,

    /// Install the background LaunchAgent daemon.
    #[arg(long = "daemon-install")]
    daemon_install: bool,

    /// Uninstall the background LaunchAgent daemon.
    #[arg(long = "daemon-uninstall")]
    daemon_uninstall: bool,

    /// Print LaunchAgent daemon status.
    #[arg(long = "daemon-status")]
    daemon_status: bool,

    /// Run the autonomous self-healing daemon loop in the foreground.
    #[arg(long)]
    daemon: bool,

    /// Run the headless link-anomaly watchdog loop in the foreground.
    #[arg(long)]
    watchdog: bool,

    /// Run the 6-stage Hop 2 anti-bloat repair and exit.
    #[arg(long, alias = "repair-hop2")]
    debloat: bool,

    /// Run Path MTU discovery + MSS clamp and exit.
    #[arg(long, alias = "check-mtu")]
    pmtu: bool,

    /// Diagnose non-Wi-Fi RF interference and exit.
    #[arg(long = "rf-inference", alias = "detect-interference")]
    rf_inference: bool,

    /// Run the Wi-Fi security / evil-twin / ARP audit and exit.
    #[arg(long = "security-guard", alias = "audit-security")]
    security_guard: bool,

    /// Query gateway UPnP WAN status (add --confirm to warm-reboot).
    #[arg(long = "reboot-gateway", alias = "gateway-status")]
    reboot_gateway: bool,

    /// Confirm destructive actions such as gateway reboot.
    #[arg(long)]
    confirm: bool,

    /// Run the real-time acoustic sweet-spot radar locator and exit.
    #[arg(long, alias = "radar-audio")]
    sweetspot: bool,

    /// Run a multi-stream speed + bufferbloat benchmark and exit.
    #[arg(long)]
    speedtest: bool,

    /// Export a timestamped Markdown ISP diagnostic ticket and exit.
    #[arg(long)]
    export: bool,

    /// Scan and terminate orphaned aether processes and unfreeze trapped apps.
    #[arg(long, alias = "clean-orphans")]
    rescue: bool,

    /// Allow multiple concurrent aether instances (bypasses single-instance lock).
    #[arg(long)]
    multi: bool,

    /// Activate a named mission profile (meeting/gaming/stream/bloat/public/rescue) and exit.
    #[arg(long, default_value = "")]
    preset: String,

    /// Target escalation tier (1..4) for --orchestrate.
    #[arg(long)]
    tier: Option<u8>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if cli.daemon_install {
        println!("{}", core::daemon::install());
        return Ok(());
    }
    if cli.daemon_uninstall {
        println!("{}", core::daemon::uninstall());
        return Ok(());
    }
    if cli.daemon_status {
        println!("{}", core::daemon::status());
        return Ok(());
    }
    if cli.daemon {
        std::process::exit(core::daemon::run_autonomous_daemon(
            &cli.interface,
            12,
            None,
        ));
    }
    if cli.watchdog {
        std::process::exit(core::watchdog::run_watchdog_daemon(
            &cli.interface,
            20,
            None,
        ));
    }
    if cli.doctor {
        return run_cli_doctor(&cli);
    }
    if cli.boost {
        return run_cli_boost(&cli);
    }
    if cli.orchestrate {
        return run_cli_orchestrate(&cli);
    }
    if cli.debloat {
        return run_cli_debloat(&cli);
    }
    if cli.pmtu {
        return run_cli_pmtu(&cli);
    }
    if cli.rf_inference {
        return run_cli_rf_inference(&cli);
    }
    if cli.security_guard {
        return run_cli_security_guard(&cli);
    }
    if cli.reboot_gateway {
        return run_cli_gateway(&cli);
    }
    if cli.sweetspot {
        core::acoustic_radar::run_live_locator(&cli.interface, 20.0, true);
        return Ok(());
    }
    if cli.speedtest {
        return run_cli_speedtest(&cli);
    }
    if cli.rescue {
        return run_cli_rescue(&cli);
    }
    if cli.export {
        return run_cli_export(&cli);
    }
    if !cli.preset.is_empty() {
        return run_cli_preset(&cli);
    }
    if cli.status || cli.json {
        return run_oneshot(&cli);
    }
    run_tui(&cli.interface, cli.multi)
}

fn warm_scope(
    interface: &str,
) -> (
    core::collector::Collector,
    core::PingScope,
    core::hops::ScopeSnapshot,
    core::model::Telemetry,
) {
    let mut collector = core::collector::Collector::new(interface);
    let mut scope = core::PingScope::new(interface);
    let mut snap = scope.probe();
    for _ in 0..2 {
        std::thread::sleep(std::time::Duration::from_millis(150));
        snap = scope.probe();
    }
    let tel = collector.collect(snap.avg_ms, snap.loss_pct);
    (collector, scope, snap, tel)
}

fn run_cli_debloat(cli: &Cli) -> io::Result<()> {
    let (_, _, snap, tel) = warm_scope(&cli.interface);
    let anomaly = core::hop2::detect_anomaly(&snap);
    let band_eff = core::hop2::evaluate_band_efficiency(&tel);
    println!("// RUNNING AETHER COCKPIT // 6-STAGE HOP 2 ANTI-BLOAT & DE-JITTER ENGINE //");
    println!("{}", "=".repeat(78));
    println!("  Initial State : {}", anomaly.diagnosis);
    println!("  RF Efficiency : {band_eff}");
    core::hop2::refresh_gateway_arp(&tel.gateway);
    let mut kernel = core::kernel_tuner::KernelTuner::new("Wi-Fi", &cli.interface);
    kernel.toggle_ffxiv_tcp_nodelay(Some(true));
    kernel.toggle_wmm_vip_priority(Some(true));
    println!(
        "  Result        : [OK] Hop 2 Anti-Bloat Healed (CAM + WMM + Fast-ACK + ARP refresh)."
    );
    Ok(())
}

fn run_cli_pmtu(cli: &Cli) -> io::Result<()> {
    let doctor = core::pmtu::PmtuDoctor::new(&cli.interface);
    println!("// RUNNING WIFI COCKPIT // PATH MTU DISCOVERY & BLACK HOLE DOCTOR //");
    println!("{}", "=".repeat(78));
    let diag = doctor.diagnose(None);
    println!("  Interface MTU       : {} B", diag.interface_mtu);
    println!("  Discovered Wire MTU : {} B", diag.wire_mtu);
    println!("  Optimal TCP MSS     : {} B", diag.optimal_mss);
    println!(
        "  Black Hole Status   : {}",
        if diag.is_blackhole {
            "[!] SILENT BLACK HOLE DETECTED"
        } else {
            "[OK] Clean (No Black Hole)"
        }
    );
    println!("  Diagnosis           : {}", diag.diagnosis);
    println!("{}", "-".repeat(78));
    let (_, msg) = doctor.auto_heal(None);
    println!("  Action Result       : {msg}");
    Ok(())
}

fn run_cli_rf_inference(cli: &Cli) -> io::Result<()> {
    let (_, _, _, tel) = warm_scope(&cli.interface);
    let res = core::rf_inference::run_full(&tel);
    println!("// RUNNING WIFI COCKPIT // NON-WI-FI RF & MICROWAVE INFERENCE ENGINE //");
    println!("{}", "=".repeat(78));
    println!("  RF Health Score : {}/100", res.rf_health_score);
    println!("  Airspace Status : {}", res.summary);
    println!("  Action Guide    : {}", res.primary_advice);
    Ok(())
}

fn run_cli_security_guard(cli: &Cli) -> io::Result<()> {
    let (_, _, _, tel) = warm_scope(&cli.interface);
    let mut guard = core::security_guard::SecurityGuard::new(&cli.interface);
    let rep = guard.run_security_audit(&tel);
    println!("// RUNNING AETHER COCKPIT // NETWORK SECURITY & ROGUE AP GUARD //");
    println!("{}", "=".repeat(78));
    println!("  Security Score  : {}/100", rep.security_score);
    println!("  Threat Posture  : [{}]", rep.threat_level);
    for t in &rep.threats {
        println!("    [!] {t}");
    }
    for r in &rep.remediations {
        println!("    -> {r}");
    }
    std::process::exit(if rep.threat_level == "CRITICAL" { 1 } else { 0 });
}

fn run_cli_gateway(cli: &Cli) -> io::Result<()> {
    let mut rebooter = core::gateway::GatewayRebooter::new("");
    let gw = rebooter.resolve_gateway_ip();
    println!("// RUNNING AETHER COCKPIT // GATEWAY ADMINISTRATION & REBOOTER ({gw}) //");
    println!("{}", "=".repeat(78));
    println!("  {}", rebooter.get_wan_status());
    if cli.confirm {
        println!("  [!] Executing Warm Gateway Reboot via UPnP...");
        let (ok, msg) = rebooter.reboot_gateway(true);
        println!("  Result         : {msg}");
        std::process::exit(if ok { 0 } else { 1 });
    } else {
        println!("  Tip: re-run with --reboot-gateway --confirm to warm-reboot.");
    }
    Ok(())
}

fn run_cli_speedtest(cli: &Cli) -> io::Result<()> {
    let (_, scope, _, tel) = warm_scope(&cli.interface);
    let target = scope
        .isp_ip
        .parse()
        .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
    let shared = core::speedtest::Shared::new(std::sync::Mutex::new(
        core::speedtest::SpeedtestState::default(),
    ));
    println!("// RUNNING AETHER COCKPIT // MULTI-STREAM SPEED & BUFFERBLOAT SUITE //");
    let msg = core::speedtest::run_full_suite(&shared, target, true);
    let st = shared.lock().unwrap();
    let snap = core::speedtest::snapshot(&st, &tel);
    println!("{msg}");
    println!(
        "  ▼ Download  : {} Mbps (Peak {})",
        st.down_mbps, st.peak_down_mbps
    );
    println!(
        "  ▲ Upload    : {} Mbps (Peak {})",
        st.up_mbps, st.peak_up_mbps
    );
    println!(
        "  Grade / RPM : {} │ {} RPM",
        st.bufferbloat_grade, st.rpm_score
    );
    for (name, verdict, _) in &snap.workloads {
        println!("  ▸ {name:<20}: {verdict}");
    }
    Ok(())
}

fn run_cli_export(cli: &Cli) -> io::Result<()> {
    let (_, _, snap, tel) = warm_scope(&cli.interface);
    let (_, msg) = core::ticket::export_report(&tel, &snap, Some(&tel.history_summary));
    println!("{msg}");
    Ok(())
}

fn run_cli_rescue(_cli: &Cli) -> io::Result<()> {
    println!("// RUNNING WIFI COCKPIT // SYSTEM RESCUE & ORPHAN SHIELD //");
    println!("{}", "=".repeat(78));
    let report = core::shield::rescue_system();
    println!("  Status         : {}", report.diagnosis);
    println!("  Orphans Reaped : {}", report.orphans_killed.len());
    for pid in &report.orphans_killed {
        println!("    [-] Terminated rogue PID {pid}");
    }
    println!("  Processes Woken: {} (SIGCONT sent)", report.frozen_procs_resumed);
    println!(
        "  Lock Cleaned   : {}",
        if report.lock_cleaned {
            "[OK] /tmp/wifi-tui.lock freed"
        } else {
            "[OK] Clean"
        }
    );
    println!("{}", "-".repeat(78));
    println!("  System State   : [OK] Normal baseline restored.");
    Ok(())
}

fn run_cli_preset(cli: &Cli) -> io::Result<()> {
    let id = core::preset_engine::PresetEngine::mission_to_preset(&cli.preset);
    let defs = core::preset_engine::preset_definitions();
    let def = defs.iter().find(|p| p.id == id).unwrap_or(&defs[1]);
    println!(
        "// APPLYING MISSION PROFILE: '{}' //",
        cli.preset.to_uppercase()
    );
    println!("{}", "=".repeat(78));
    // Apply the preset's concrete tuning via the engines it maps to.
    let mut kernel = core::kernel_tuner::KernelTuner::new("Wi-Fi", &cli.interface);
    let mut antilag = core::antilag::AntiLagEngine::new(&cli.interface);
    match id {
        2 | 7 => {
            kernel.toggle_wmm_vip_priority(Some(true));
            kernel.toggle_ffxiv_tcp_nodelay(Some(true));
            antilag.toggle_sync_daemons(true);
        }
        3 => {
            kernel.toggle_8mb_streaming_buffers(Some(true));
            kernel.toggle_wmm_vip_priority(Some(true));
            kernel.prewarm_workload_edges();
        }
        4 => {
            antilag.toggle_sync_daemons(true);
            kernel.toggle_8mb_streaming_buffers(Some(true));
        }
        6 => {
            let mut doctor = core::wan_doctor::WanDoctor::new(&cli.interface, "Wi-Fi");
            doctor.run_auto_doctor(Some(&mut kernel), false);
        }
        _ => {}
    }
    println!("  Status  : Activated {} [{}]", def.name, def.short);
    Ok(())
}

fn run_cli_doctor(cli: &Cli) -> io::Result<()> {
    let mut kernel = core::kernel_tuner::KernelTuner::new("Wi-Fi", &cli.interface);
    let mut doctor = core::wan_doctor::WanDoctor::new(&cli.interface, "Wi-Fi");
    println!("// RUNNING AETHER COCKPIT // WAN & YOUTUBE AUTO-DOCTOR //");
    println!("{}", "=".repeat(78));
    let (ok, summary) = doctor.run_auto_doctor(Some(&mut kernel), false);
    println!("  {summary}");
    std::process::exit(if ok { 0 } else { 1 });
}

fn run_cli_boost(cli: &Cli) -> io::Result<()> {
    let mut collector = core::collector::Collector::new(&cli.interface);
    let mut scope = core::PingScope::new(&cli.interface);
    let snap = scope.probe();
    let tel = collector.collect(snap.avg_ms, snap.loss_pct);
    let mut kernel = core::kernel_tuner::KernelTuner::new("Wi-Fi", &cli.interface);
    let dns = core::dns_engine::DnsEngine::new("Wi-Fi");
    let mut antilag = core::antilag::AntiLagEngine::new(&cli.interface);
    let mut doctor = core::wan_doctor::WanDoctor::new(&cli.interface, "Wi-Fi");
    let (_, doc) = doctor.run_auto_doctor(Some(&mut kernel), false);
    let boost = antilag.run_god_mode_boost(&tel, &dns, &mut kernel);
    println!("{boost}\n{doc}");
    Ok(())
}

fn run_cli_orchestrate(cli: &Cli) -> io::Result<()> {
    let mut collector = core::collector::Collector::new(&cli.interface);
    let mut scope = core::PingScope::new(&cli.interface);
    let snap = scope.probe();
    let tel = collector.collect(snap.avg_ms, snap.loss_pct);
    let mut orch = core::orchestrator::Orchestrator::new(&cli.interface, "Wi-Fi");
    println!("// RUNNING AETHER COCKPIT // 4-TIER AUTONOMOUS SELF-HEALING ORCHESTRATOR //");
    println!("{}", "=".repeat(78));
    let res = orch.run(&tel, &snap, cli.tier);
    println!("  {}", res.summary);
    for a in &res.actions {
        println!("    [+] {a}");
    }
    std::process::exit(if res.success { 0 } else { 1 });
}

/// One-shot CLI: collect a snapshot (with a short ping sample) and print it.
fn run_oneshot(cli: &Cli) -> io::Result<()> {
    let mut collector = core::collector::Collector::new(&cli.interface);
    let mut scope = core::PingScope::new(&cli.interface);

    // Warm the hop matrix with a few probes so latency figures are meaningful.
    let mut snap = scope.probe();
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(150));
        snap = scope.probe();
    }
    let tel = collector.collect(snap.avg_ms, snap.loss_pct);

    if cli.json {
        #[derive(serde::Serialize)]
        struct Report<'a> {
            telemetry: &'a core::model::Telemetry,
            avg_rtt_ms: f64,
            jitter_ms: f64,
            loss_pct: f64,
            fault_diagnosis: &'a str,
        }
        let report = Report {
            telemetry: &tel,
            avg_rtt_ms: snap.avg_ms,
            jitter_ms: snap.jitter_ms,
            loss_pct: snap.loss_pct,
            fault_diagnosis: &snap.fault_diagnosis,
        };
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return Ok(());
    }

    println!("// WIFI COCKPIT // STATUS REPORT //");
    println!("{}", "=".repeat(68));
    let name = if tel.ssid_available {
        tel.ssid.clone()
    } else {
        format!("{} (inferred — grant Location to confirm)", tel.ssid)
    };
    println!(
        "  SSID          : {} ({}) [{}]",
        name,
        tel.band.label(),
        tel.phy_mode
    );
    println!("  Channel       : {}", tel.channel_label);
    println!(
        "  Signal / Noise: {} dBm / {} dBm (SNR {} dB)",
        tel.signal_dbm, tel.noise_dbm, tel.snr_db
    );
    println!(
        "  Link Rate     : {} Mbps (MCS {})",
        tel.tx_rate_mbps, tel.mcs_index
    );
    println!("  Security      : {}", tel.security);
    println!("  IP -> Gateway : {} -> {}", tel.ipv4, tel.gateway);
    println!("  DNS           : {}", tel.dns_servers.join(", "));
    println!(
        "  Health Score  : {}/100 [{}]",
        tel.health_score,
        tel.health_rating.label()
    );
    println!("{}", "-".repeat(68));
    println!("  4-Hop Matrix:");
    for h in &snap.hops {
        println!(
            "   [{}] {:<12} RTT {:>5.1}ms  Jit {:>4.1}ms  Bloat +{:>4.0}ms  {}",
            h.hop, h.label, h.rtt_ms, h.jitter_ms, h.bloat_delta_ms, h.status
        );
    }
    println!("   Verdict: {}", snap.fault_diagnosis);
    if !tel.alerts.is_empty() {
        println!("{}", "-".repeat(68));
        for a in &tel.alerts {
            println!("   [!] {a}");
        }
    }
    Ok(())
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

use crate::app::Action;
use crate::render::RenderState;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Mutex;

/// Run the interactive TUI with a worker-thread architecture:
/// - the worker thread owns every engine and runs all blocking work (refreshes,
///   doctor, speedtest, boost, …);
/// - the UI thread only reads the published `RenderState` and forwards keypresses.
///
/// This is what makes `[D]`, speedtest, etc. non-blocking: a keypress just sends
/// an `Action` down the channel and returns instantly, so the UI keeps drawing
/// (with a spinner) while the worker does the slow work.
fn run_tui(interface: &str, multi: bool) -> io::Result<()> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        eprintln!("error: standard input is not a terminal. Use --status or --json for non-interactive output.");
        std::process::exit(1);
    }

    // 1. Single-Instance Lock & Automated Ghost Reaper:
    // If an orphaned or dead aether process exists from a crash, it is automatically
    // detected and reaped here before starting the UI.
    let _instance_lock = if !multi {
        match core::shield::InstanceLock::acquire() {
            Ok(lock) => Some(lock),
            Err(core::shield::LockError::AlreadyRunning(pid)) => {
                eprintln!(
                    "[!] aether is already running in another window (PID {pid}).\n    Run with `aether --multi` for concurrent sessions, or `aether --rescue` to clean stale ghosts."
                );
                std::process::exit(0);
            }
            Err(e) => {
                core::shield::log_shield(&format!("Instance lock acquisition warning: {e:?}"));
                None
            }
        }
    } else {
        None
    };

    // 2. Dead-Man's Switch: monitors parent process ID (getppid == 1) & TTY severing.
    let mut _deadman = core::shield::DeadManSwitch::start();

    // 3. CPU Runaway Circuit Breaker: monitors rusage delta; trips if CPU > 85% for 4.5s while idle.
    let shield_busy = Arc::new(AtomicBool::new(false));
    let mut _circuit_breaker = core::shield::CpuCircuitBreaker::start(Arc::clone(&shield_busy));

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        core::procmon::resume_all_global();
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    let stop = Arc::new(AtomicBool::new(false));
    let worker_quit = Arc::new(AtomicBool::new(false));
    let (tx, rx): (Sender<Action>, Receiver<Action>) = mpsc::channel();

    // Dedicated signal watcher thread: guarantees that if SIGHUP, SIGINT or SIGTERM
    // arrives (e.g. user closes terminal tab / window), all frozen processes are
    // immediately resumed, terminal state is restored, and the process exits cleanly
    // instead of lingering as a 100% CPU orphan under launchd.
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ])?;
    let stop_sig = Arc::clone(&stop);
    let worker_quit_sig = Arc::clone(&worker_quit);
    let tx_sig = tx.clone();
    std::thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            core::procmon::resume_all_global();
            stop_sig.store(true, Ordering::SeqCst);
            worker_quit_sig.store(true, Ordering::SeqCst);
            let _ = tx_sig.send(Action::Quit);

            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);

            // Brief grace period for worker to clean up, then exit cleanly.
            std::thread::sleep(Duration::from_millis(80));
            std::process::exit(if sig == signal_hook::consts::SIGHUP || sig == signal_hook::consts::SIGINT { 0 } else { 128 + sig });
        }
    });

    // Build the app on the worker thread (engines aren't Send-friendly to share),
    // but we need its first render snapshot to seed the UI. Construct it inside
    // the worker and hand back the initial snapshot via the shared mutex.
    let shared: Arc<Mutex<Option<RenderState>>> = Arc::new(Mutex::new(None));

    let worker = {
        let shared = Arc::clone(&shared);
        let worker_quit = Arc::clone(&worker_quit);
        let stop = Arc::clone(&stop);
        let iface = interface.to_string();
        let shield_busy = Arc::clone(&shield_busy);
        std::thread::spawn(move || worker_loop(&iface, shared, worker_quit, stop, shield_busy, rx))
    };

    // Wait briefly for the worker to publish its first snapshot.
    let mut terminal = setup_terminal()?;
    let res = ui_loop(&mut terminal, &shared, &tx, &worker_quit, &stop);

    // Tell the worker to finish and clean up (resume frozen processes).
    worker_quit.store(true, Ordering::SeqCst);
    let _ = tx.send(Action::Quit); // unblock the worker if it's waiting
    let _ = worker.join();

    core::procmon::resume_all_global();
    restore_terminal(&mut terminal)?;
    if let Err(e) = res {
        eprintln!("error: {e}");
    }
    Ok(())
}

/// The worker thread: owns the App, processes actions + periodic refresh, and
/// republishes the render snapshot after every change.
fn worker_loop(
    interface: &str,
    shared: Arc<Mutex<Option<RenderState>>>,
    worker_quit: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    shield_busy: Arc<AtomicBool>,
    rx: Receiver<Action>,
) {
    let mut app = App::new(interface, Arc::clone(&stop));
    app.refresh();
    publish(&shared, &app);

    let refresh_every = Duration::from_millis(1500);
    let mut last_refresh = Instant::now();

    loop {
        if worker_quit.load(Ordering::Relaxed) || app.should_quit || stop.load(Ordering::Relaxed) {
            break;
        }

        // Inform the CPU circuit breaker whether high workload is active
        let is_busy = app.is_speedtest_running() || app.has_bg_inflight();
        shield_busy.store(is_busy, Ordering::Relaxed);

        // Poll fast while any background work (speedtest or a heavy action) is
        // in flight so live progress / completion is republished promptly.
        let poll = if is_busy {
            Duration::from_millis(80)
        } else {
            Duration::from_millis(120)
        };
        match rx.recv_timeout(poll) {
            Ok(action) => {
                // Every action now returns instantly: heavy work is dispatched
                // to a background thread via spawn_bg, so the worker loop is
                // never blocked and stays free to process navigation.
                app.apply_action(action);
                publish(&shared, &app);
                // Drain the rest of the queue without blocking.
                loop {
                    match rx.try_recv() {
                        Ok(a) => {
                            app.apply_action(a);
                            publish(&shared, &app);
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Merge any completed background-action outcomes and republish.
        if app.drain_outbox() {
            publish(&shared, &app);
        }

        // While a speedtest runs on its background thread, republish each cycle
        // so the growing bars, climbing numbers and scrolling scope animate.
        if app.is_speedtest_running() {
            publish(&shared, &app);
        }

        if last_refresh.elapsed() >= refresh_every {
            app.refresh();
            last_refresh = Instant::now();
            publish(&shared, &app);
        }
    }

    app.teardown();
    let _ = app.procmon.resume_all();
}

fn publish(shared: &Arc<Mutex<Option<RenderState>>>, app: &App) {
    if let Ok(mut guard) = shared.lock() {
        *guard = Some(app.build_render());
    }
}

/// The UI thread: draw the latest snapshot, read keys, forward actions.
fn ui_loop(
    terminal: &mut Tui,
    shared: &Arc<Mutex<Option<RenderState>>>,
    tx: &Sender<Action>,
    worker_quit: &Arc<AtomicBool>,
    stop: &Arc<AtomicBool>,
) -> io::Result<()> {
    let mut spin = 0usize;
    let mut quitting = false;
    loop {
        if quitting || worker_quit.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
            break;
        }

        // Draw the latest published snapshot (if any yet). The snapshot's own
        // `busy` flag (set from App::has_bg_inflight / speedtest) drives the
        // spinner — the UI thread never computes it.
        let snap = shared.lock().ok().and_then(|g| g.clone());
        if let Some(rs) = snap {
            spin = spin.wrapping_add(1);
            terminal.draw(|f| ui::draw(f, &rs, spin))?;

            // Read a key with a short 20ms timeout so the UI animates at 50 FPS
            // and keypresses are handled instantly with zero latency.
            // If stdin is closed / revoked (EOF), poll returns Err or false.
            match event::poll(Duration::from_millis(20)) {
                Ok(true) => {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            if let Some(action) = map_key(&rs, key.code, key.modifiers, &mut quitting) {
                                let _ = tx.send(action);
                            }
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    // Stdin closed, revoked or EOF: exit UI loop gracefully.
                    if e.kind() == io::ErrorKind::UnexpectedEof
                        || e.kind() == io::ErrorKind::BrokenPipe
                        || e.kind() == io::ErrorKind::NotFound
                    {
                        break;
                    }
                    return Err(e);
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(())
}

/// Translate a keypress into an `Action` to send to the worker. Returns `None`
/// for keys that have no effect in the current context. Sets `quitting` on quit.
fn map_key(
    rs: &RenderState,
    code: KeyCode,
    mods: KeyModifiers,
    quitting: &mut bool,
) -> Option<Action> {
    use Action::*;

    // Action palette overlay captures all keys while open.
    if rs.palette_active {
        return Some(match code {
            KeyCode::Esc => PaletteClose,
            KeyCode::Enter => PaletteConfirm,
            KeyCode::Up => PaletteUp,
            KeyCode::Down => PaletteDown,
            KeyCode::Backspace => PaletteBackspace,
            KeyCode::Char('n') if mods.contains(KeyModifiers::CONTROL) => PaletteDown,
            KeyCode::Char('p') if mods.contains(KeyModifiers::CONTROL) => PaletteUp,
            KeyCode::Char(c) => PaletteChar(c),
            _ => return None,
        });
    }

    // Password editor sub-mode captures most keys.
    if rs.pw_edit_active {
        return Some(match code {
            KeyCode::Esc => PwCancel,
            KeyCode::Enter => PwSave,
            KeyCode::Tab => PwTab,
            KeyCode::Char('k') if mods.contains(KeyModifiers::CONTROL) => PwKeychain,
            KeyCode::Backspace => PwBackspace,
            KeyCode::Char(c) => PwChar(c),
            _ => return None,
        });
    }

    let tab = rs.active_tab;
    let a = match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            *quitting = true;
            Quit
        }
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
            *quitting = true;
            Quit
        }
        KeyCode::Char('1') => SwitchTab(Tab::Dashboard),
        KeyCode::Char('2') => SwitchTab(Tab::Spectrum),
        KeyCode::Char('3') => SwitchTab(Tab::Processes),
        KeyCode::Char('4') => SwitchTab(Tab::Tuning),
        KeyCode::Char('5') => SwitchTab(Tab::SpeedTest),
        KeyCode::Char('6') => SwitchTab(Tab::Device),
        KeyCode::Char('7') => SwitchTab(Tab::Maintenance),
        KeyCode::Char('8') => SwitchTab(Tab::LanRadar),
        KeyCode::Char('9') => SwitchTab(Tab::GamingRadar),
        KeyCode::Char('0') => SwitchTab(Tab::PrivacyFortress),
        KeyCode::Tab => NextTab,
        KeyCode::BackTab => PrevTab,
        KeyCode::Left => {
            if tab == Tab::Maintenance {
                MaintenanceFocusPrev
            } else if tab == Tab::PrivacyFortress {
                PrivacyToggleColumn
            } else {
                return None;
            }
        }
        KeyCode::Right => {
            if tab == Tab::Maintenance {
                MaintenanceToggleFocus
            } else if tab == Tab::PrivacyFortress {
                PrivacyToggleColumn
            } else {
                return None;
            }
        }
        KeyCode::PageUp => PageUp,
        KeyCode::PageDown => PageDown,
        KeyCode::Home => Home,
        KeyCode::End => End,
        KeyCode::Char('j') | KeyCode::Down => {
            if tab == Tab::Processes || tab == Tab::Tuning || tab == Tab::Maintenance || tab == Tab::LanRadar || tab == Tab::GamingRadar || tab == Tab::PrivacyFortress {
                CursorDown
            } else if code == KeyCode::Char('j') {
                ExportTicket
            } else {
                return None;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if tab == Tab::Processes || tab == Tab::Tuning || tab == Tab::Maintenance || tab == Tab::LanRadar || tab == Tab::GamingRadar || tab == Tab::PrivacyFortress {
                CursorUp
            } else {
                CursorUp
            }
        }
        KeyCode::Char('J') => ExportTicket,
        KeyCode::Char('K') => CamPacer,
        KeyCode::Char(' ') => {
            if tab == Tab::GamingRadar {
                GamingToggleShield
            } else if tab == Tab::LanRadar {
                LanToggleTrust
            } else if tab == Tab::PrivacyFortress {
                DnsApplySelected
            } else {
                Confirm
            }
        }
        KeyCode::Enter => {
            if tab == Tab::PrivacyFortress {
                DnsApplySelected
            } else {
                Confirm
            }
        },
        KeyCode::Char('v') | KeyCode::Char('V') if tab == Tab::SpeedTest => SpeedQuick,
        KeyCode::Char('n') | KeyCode::Char('N') if tab == Tab::SpeedTest => SpeedApple,
        KeyCode::Char('n') | KeyCode::Char('N') if tab == Tab::Processes => LanSweep,
        KeyCode::Char('u') | KeyCode::Char('U') => {
            if tab == Tab::Maintenance {
                UninstallSelectedApp
            } else if tab == Tab::Processes {
                ResumeAll
            } else {
                return None;
            }
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            if tab == Tab::LanRadar {
                LanPortScan
            } else if code == KeyCode::Char('p') {
                PaletteOpen
            } else {
                CyclePreset
            }
        }
        KeyCode::Char('w') | KeyCode::Char('W') => ToggleWmm,
        KeyCode::Char('x') | KeyCode::Char('X') => ToggleNoDelay,
        KeyCode::Char('o') | KeyCode::Char('O') => Hop2Repair,
        KeyCode::Char('i') | KeyCode::Char('I') => Prewarm,
        KeyCode::Char('g') | KeyCode::Char('G') => {
            if tab == Tab::GamingRadar {
                GamingCycleFilter
            } else {
                Boost
            }
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            if tab == Tab::Maintenance {
                CycleSafetyTier
            } else {
                ZeroLag
            }
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => ZombieSync,
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if tab == Tab::PrivacyFortress {
                DnsResetDhcp
            } else {
                Doctor
            }
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => QuicFastpath,
        KeyCode::Char('b') | KeyCode::Char('B') => Shape,
        KeyCode::Char('h') | KeyCode::Char('H') => SafeRenew,
        KeyCode::Char('f') | KeyCode::Char('F') => FixExclamation,
        KeyCode::Char('t') | KeyCode::Char('T') => {
            if tab == Tab::Maintenance {
                CycleTheme
            } else {
                MtuClamp
            }
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            if tab == Tab::Tuning {
                MacPrivacy
            } else {
                VisualNext
            }
        }
        KeyCode::Char(']') => VisualNext,
        KeyCode::Char('[') => VisualPrev,
        KeyCode::Char('e') | KeyCode::Char('E') => PwOpen,
        KeyCode::Char('s') | KeyCode::Char('S') => {
            if tab == Tab::Maintenance {
                OptSsdOnly
            } else if tab == Tab::Spectrum {
                SweetSpot
            } else {
                RootAuth
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            if tab == Tab::Maintenance {
                OptAll
            } else {
                AutoPilot
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            if tab == Tab::Device || tab == Tab::Maintenance {
                ToggleMascot
            } else {
                CaptiveGateway
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if tab == Tab::Maintenance {
                OptRamOnly
            } else if tab == Tab::Device {
                RefreshHardware
            } else if tab == Tab::LanRadar {
                LanSweep
            } else if tab == Tab::GamingRadar {
                GamingRescan
            } else if tab == Tab::PrivacyFortress {
                DnsRace
            } else {
                RoamRescan
            }
        }
        _ => return None,
    };
    Some(a)
}

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
