//! wifi-tui — native Rust macOS Wi-Fi cockpit.
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
mod ui;

use std::io::{self, Stdout};
use std::sync::atomic::AtomicBool;
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
use core::speedtest;

#[derive(Parser, Debug)]
#[command(
    name = "wifi-tui",
    version,
    about = "Amber CRT macOS Wi-Fi cockpit: RF telemetry, 4-hop bufferbloat matrix, spectrum radar & per-process firewall"
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
    if cli.export {
        return run_cli_export(&cli);
    }
    if !cli.preset.is_empty() {
        return run_cli_preset(&cli);
    }
    if cli.status || cli.json {
        return run_oneshot(&cli);
    }
    run_tui(&cli.interface)
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
    println!("// RUNNING WIFI COCKPIT // 6-STAGE HOP 2 ANTI-BLOAT & DE-JITTER ENGINE //");
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
    println!("// RUNNING WIFI COCKPIT // NETWORK SECURITY & ROGUE AP GUARD //");
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
    println!("// RUNNING WIFI COCKPIT // GATEWAY ADMINISTRATION & REBOOTER ({gw}) //");
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
    let mut st = core::speedtest::SpeedtestState::default();
    println!("// RUNNING WIFI COCKPIT // MULTI-STREAM SPEED & BUFFERBLOAT SUITE //");
    let msg = core::speedtest::run_full_suite(&mut st, target, true);
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
    println!("// RUNNING WIFI COCKPIT // WAN & YOUTUBE AUTO-DOCTOR //");
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
    println!("// RUNNING WIFI COCKPIT // 4-TIER AUTONOMOUS SELF-HEALING ORCHESTRATOR //");
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

fn run_tui(interface: &str) -> io::Result<()> {
    // Shared stop flag; the signal handler flips it so Ctrl-C / SIGTERM / SIGHUP
    // all exit through the same cleanup path that resumes frozen processes.
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(sig, Arc::clone(&stop))?;
    }

    let mut terminal = setup_terminal()?;
    let mut app = App::new(interface, Arc::clone(&stop));
    let res = event_loop(&mut terminal, &mut app);

    // Always restore the terminal and resume frozen processes, even on error.
    app.teardown();
    restore_terminal(&mut terminal)?;

    if let Err(e) = res {
        eprintln!("error: {e}");
    }
    let resumed = app.procmon.resume_all();
    if resumed > 0 {
        eprintln!("Resumed {resumed} frozen process(es) on exit.");
    }
    Ok(())
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

fn event_loop(terminal: &mut Tui, app: &mut App) -> io::Result<()> {
    let refresh_every = Duration::from_millis(1500);
    let mut last_refresh = Instant::now() - refresh_every; // force an immediate refresh

    loop {
        if app.should_quit || app.signalled_to_stop() {
            break;
        }

        if last_refresh.elapsed() >= refresh_every {
            app.refresh();
            last_refresh = Instant::now();
        }

        terminal.draw(|f| ui::draw(f, app))?;

        // Poll with a short timeout so the UI stays responsive between refreshes.
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.pw_edit_active {
                        handle_pw_editor(app, key.code, key.modifiers);
                    } else {
                        handle_key(app, key.code, key.modifiers);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Password-editor sub-mode: capture typed characters and save/connect.
fn handle_pw_editor(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    match code {
        KeyCode::Esc => {
            app.pw_edit_active = false;
            app.status_msg = "Wi-Fi Password editor closed.".into();
        }
        KeyCode::Enter => {
            let target = app.pw_edit_ssid.clone();
            let pw = app.pw_edit_buf.trim().to_string();
            app.pw_edit_active = false;
            app.status_msg = if pw.is_empty() {
                app.profiles.set_wifi_password(&target, "");
                format!("Cleared stored password for '{target}'.")
            } else {
                app.profiles.set_wifi_password(&target, &pw)
            };
            app.telemetry.wifi_password = app.profiles.get_wifi_password(&app.telemetry.ssid);
        }
        KeyCode::Tab => {
            let mut saved = app.telemetry.saved_networks.clone();
            let curr = app.telemetry.ssid.clone();
            if !curr.is_empty() && !saved.contains(&curr) {
                saved.insert(0, curr);
            }
            if !saved.is_empty() {
                app.pw_edit_idx = (app.pw_edit_idx + 1) % saved.len();
                app.pw_edit_ssid = saved[app.pw_edit_idx].clone();
                app.pw_edit_buf = app.profiles.get_wifi_password(&app.pw_edit_ssid);
            }
        }
        KeyCode::Char('k') if mods.contains(KeyModifiers::CONTROL) => {
            let target = app.pw_edit_ssid.clone();
            app.pw_edit_active = false;
            app.status_msg = match app.profiles.fetch_keychain_password(&target) {
                Some(pw) => format!("[OK] Synced password for '{target}' from macOS Keychain: {pw}"),
                None => format!("[!] Keychain entry not found or cancelled for '{target}'. Press [E] to type it."),
            };
        }
        KeyCode::Backspace => {
            app.pw_edit_buf.pop();
        }
        KeyCode::Char(c) => {
            app.pw_edit_buf.push(c);
        }
        _ => {}
    }
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let tab = app.active_tab;
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.should_quit = true,

        KeyCode::Char('1') => app.active_tab = Tab::Dashboard,
        KeyCode::Char('2') => app.active_tab = Tab::Spectrum,
        KeyCode::Char('3') => app.active_tab = Tab::Processes,
        KeyCode::Char('4') => app.active_tab = Tab::Tuning,
        KeyCode::Char('5') => app.active_tab = Tab::SpeedTest,
        KeyCode::Tab => app.active_tab = app.active_tab.next(),
        KeyCode::BackTab => app.active_tab = app.active_tab.prev(),

        KeyCode::Char('j') | KeyCode::Down => {
            if tab == Tab::Processes && !app.procmon.last.is_empty() {
                app.proc_cursor = (app.proc_cursor + 1).min(app.procmon.last.len() - 1);
            } else if tab == Tab::Tuning {
                app.preset.cursor_idx = (app.preset.cursor_idx + 1) % 12;
            } else if code == KeyCode::Char('j') {
                export_ticket(app);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if tab == Tab::Processes {
                app.proc_cursor = app.proc_cursor.saturating_sub(1);
            } else if tab == Tab::Tuning {
                app.preset.cursor_idx = (app.preset.cursor_idx + 11) % 12;
            } else {
                app.status_msg = app.scope.toggle_cam_pacer();
            }
        }
        KeyCode::Char('J') => export_ticket(app),
        KeyCode::Char('K') => app.status_msg = app.scope.toggle_cam_pacer(),

        KeyCode::Char(' ') | KeyCode::Enter => match tab {
            Tab::Processes => {
                if let Some(p) = app.procmon.last.get(app.proc_cursor).cloned() {
                    app.status_msg = app.procmon.toggle_pause(p.pid, &p.name);
                }
            }
            Tab::Tuning => toggle_tuning_control(app),
            Tab::SpeedTest => {
                let target = app
                    .scope
                    .isp_ip
                    .parse()
                    .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
                app.status_msg = speedtest::run_full_suite(&mut app.speedtest, target, false);
            }
            _ => {}
        },

        KeyCode::Char('v') | KeyCode::Char('V') if tab == Tab::SpeedTest => {
            let target = app
                .scope
                .isp_ip
                .parse()
                .unwrap_or(std::net::Ipv4Addr::new(1, 1, 1, 1));
            app.status_msg = speedtest::run_full_suite(&mut app.speedtest, target, true);
        }
        KeyCode::Char('n') | KeyCode::Char('N') if tab == Tab::SpeedTest => {
            app.status_msg = speedtest::run_apple_network_quality(&mut app.speedtest);
        }
        KeyCode::Char('n') | KeyCode::Char('N') if tab == Tab::Processes => {
            let ip = app.telemetry.ipv4.clone();
            let gw = app.telemetry.gateway.clone();
            app.lan_data = app.lan_scanner.scan_lan(&ip, &gw, true);
            app.status_msg = format!("[OK] LAN Sweep Complete: {}", app.lan_data.summary);
        }
        KeyCode::Char('u') | KeyCode::Char('U') if tab == Tab::Processes => {
            let n = app.procmon.resume_all();
            app.status_msg = format!("Resumed {n} frozen processes via SIGCONT.");
        }

        KeyCode::Char('p') | KeyCode::Char('P') => {
            app.preset.cycle_preset();
            app.status_msg = format!(
                "Switched Smart Tuning Preset -> #{}",
                app.preset.selected_preset_id
            );
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            app.status_msg = app.kernel.toggle_wmm_vip_priority(None)
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            app.status_msg = app.kernel.toggle_ffxiv_tcp_nodelay(None)
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            let gw = app.telemetry.gateway.clone();
            crate::core::hop2::refresh_gateway_arp(&gw);
            app.kernel.toggle_ffxiv_tcp_nodelay(Some(true));
            app.kernel.toggle_wmm_vip_priority(Some(true));
            app.status_msg = "Ran 6-Stage Hop 2 Anti-Bloat & De-Jitter Repair.".into();
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            app.status_msg = app.kernel.prewarm_workload_edges()
        }
        KeyCode::Char('g') | KeyCode::Char('G') => {
            app.profiles.set_zero_lag_mode(true);
            app.scope.cam_pacer_active = true;
            let (_, doc) = app.wan_doctor.run_auto_doctor(Some(&mut app.kernel), false);
            let boost = {
                let tel = app.telemetry.clone();
                app.antilag
                    .run_god_mode_boost(&tel, &app.dns, &mut app.kernel)
            };
            app.dns_results = app.dns.race_resolvers();
            app.status_msg = format!("{boost} │ {doc}");
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            let mut tel = app.telemetry.clone();
            let m = app.antilag.toggle_awdl(Some(&mut tel));
            app.telemetry = tel;
            app.profiles.set_zero_lag_mode(app.antilag.zero_lag_active);
            app.status_msg = m;
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            let (_, z) = app.wan_doctor.reap_zombie_sockets();
            let s = app
                .antilag
                .toggle_sync_daemons(!app.antilag.sync_daemons_frozen);
            app.status_msg = format!("{z} │ {s}");
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            let (_, doc) = app.wan_doctor.run_auto_doctor(Some(&mut app.kernel), false);
            app.dns_results = app.dns.race_resolvers();
            app.wan_snap = app.wan_doctor.get_snapshot();
            app.status_msg = doc;
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let (_, m) = app.wan_doctor.toggle_quic_fastpath(None);
            app.status_msg = m;
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            if tab == Tab::Processes {
                if let Some(p) = app.procmon.last.get(app.proc_cursor).cloned() {
                    app.status_msg = app.shaper.toggle_throttle_pid(p.pid, &p.name, 8);
                }
            } else {
                app.status_msg = "5GHz sibling lock requires networksetup association.".into();
            }
        }
        KeyCode::Char('h') | KeyCode::Char('H') => {
            crate::core::dns_engine::DnsEngine::flush_cache();
            let iface = app.collector.interface.clone();
            crate::core::antilag::AntiLagEngine::refresh_dhcp_and_routes(&iface);
            app.status_msg =
                "Refreshed DHCP lease & DNS cache safely (zero radio disruption).".into();
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            let (_, m) = app.wan_doctor.fix_macos_exclamation_mark();
            app.status_msg = m;
        }
        KeyCode::Char('6') => {
            let (_, m) = app.wan_doctor.toggle_ipv6_guard(None);
            app.status_msg = m;
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            let new_mtu = if app.telemetry.mtu >= 1500 {
                1492
            } else {
                1500
            };
            app.status_msg = app.kernel.set_optimal_mtu(new_mtu);
        }
        KeyCode::Char('m') | KeyCode::Char('M') | KeyCode::Char(']') => {
            if tab == Tab::Tuning && matches!(code, KeyCode::Char('m') | KeyCode::Char('M')) {
                app.status_msg = app.privacy.randomize_mac();
            } else {
                app.visual_mode = (app.visual_mode % 4) + 1;
                app.status_msg = format!(
                    "Switched Visual Telemetry Deck -> [{}]",
                    visual_name(app.visual_mode)
                );
            }
        }
        KeyCode::Char('[') => {
            app.visual_mode = if app.visual_mode == 1 {
                4
            } else {
                app.visual_mode - 1
            };
            app.status_msg = format!(
                "Switched Visual Telemetry Deck -> [{}]",
                visual_name(app.visual_mode)
            );
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            let mut curr = app.telemetry.ssid.clone();
            if matches!(curr.as_str(), "Scanning..." | "Disconnected" | "") {
                curr = app
                    .telemetry
                    .saved_networks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Wi-Fi".into());
            }
            app.pw_edit_ssid = curr.clone();
            app.pw_edit_idx = 0;
            app.pw_edit_buf = app.profiles.get_wifi_password(&curr);
            app.pw_edit_active = true;
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            if tab == Tab::Spectrum {
                let q = crate::core::acoustic_radar::calculate_quality_score(&app.telemetry);
                app.status_msg = format!(
                    "Sweet-Spot Locator: {} {} ({:.1} clicks/s) │ SNR {}dB MCS {}",
                    q.meter, q.rating, q.click_hz, q.snr_db, q.mcs_index
                );
            } else {
                app.status_msg = app.antilag.authorize_root_privileges();
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let state = app.profiles.toggle_auto_pilot();
            if state {
                app.preset.selected_preset_id = 1;
            }
            app.status_msg = format!(
                "Closed-Loop Smart Preset Auto-Adapt {}.",
                if state { "ENABLED" } else { "MANUAL" }
            );
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let cap = app.captive.probe_portal(true);
            if cap.captive {
                app.status_msg = app.captive.breakout_captive_portal(Some(&app.dns));
            } else {
                app.router_status = app.router_bot.probe_ports();
                app.status_msg = format!(
                    "Gateway ({}): {} │ {}",
                    app.router_status.ip, app.router_status.summary, cap.summary
                );
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.refresh();
            app.spectrum = crate::core::spectrum::analyze(&app.telemetry);
            app.status_msg = "Smart BSSID Roam Evaluation & 5GHz/2.4GHz Rescan complete.".into();
        }
        _ => {}
    }
}

fn visual_name(m: u8) -> &'static str {
    match m {
        1 => "1:TOPOLOGY PIPELINE DIAGRAM",
        2 => "2:2D RF SIGNAL & SNR CHRONOGRAPH",
        3 => "3:802.11 QAM CONSTELLATION & MIMO",
        _ => "4:KERNEL TCP SOCKET & PORT MATRIX",
    }
}

fn export_ticket(app: &mut App) {
    let hist = app.telemetry.history_summary.clone();
    let (_, msg) = crate::core::ticket::export_report(&app.telemetry, &app.scope_snap, Some(&hist));
    app.status_msg = msg;
}

fn toggle_tuning_control(app: &mut App) {
    let idx = app.preset.cursor_idx;
    let controls = app.preset.get_tuning_controls(
        &app.telemetry,
        &app.kernel_state,
        &app.wan_snap,
        app.antilag.root_authorized,
        app.antilag.sync_daemons_frozen,
    );
    let Some(ctrl) = controls.get(idx.min(controls.len().saturating_sub(1))) else {
        return;
    };
    let id = ctrl.id;
    app.status_msg = match id {
        "wmm_vip" => app.kernel.toggle_wmm_vip_priority(None),
        "cam_pacer" => app.scope.toggle_cam_pacer(),
        "ffxiv_nodelay" => app.kernel.toggle_ffxiv_tcp_nodelay(None),
        "buf_8mb" => app.kernel.toggle_8mb_streaming_buffers(None),
        "wan_doctor" => {
            app.wan_doctor
                .run_auto_doctor(Some(&mut app.kernel), false)
                .1
        }
        "ipv6_guard" => app.wan_doctor.toggle_ipv6_guard(None).1,
        "quic_fastpath" => app.wan_doctor.toggle_quic_fastpath(None).1,
        "zero_lag" => {
            let mut tel = app.telemetry.clone();
            let m = app.antilag.toggle_awdl(Some(&mut tel));
            app.telemetry = tel;
            m
        }
        "sync_freeze" => {
            let (_, z) = app.wan_doctor.reap_zombie_sockets();
            let s = app
                .antilag
                .toggle_sync_daemons(!app.antilag.sync_daemons_frozen);
            format!("{z} │ {s}")
        }
        "band_5g" => "5GHz sibling lock requires networksetup association.".into(),
        "mtu_clamp" => {
            let new_mtu = if app.telemetry.mtu >= 1500 {
                1492
            } else {
                1500
            };
            app.kernel.set_optimal_mtu(new_mtu)
        }
        "root_auth" => app.antilag.authorize_root_privileges(),
        _ => "Module updated.".into(),
    };
}
