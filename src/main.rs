//! wifi-tui — native Rust macOS Wi-Fi cockpit.
//!
//! Entry point: argument parsing, one-shot CLI reports, and the interactive TUI
//! with a signal handler that guarantees frozen processes are resumed on exit.

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
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if cli.status || cli.json {
        return run_oneshot(&cli);
    }
    run_tui(&cli.interface)
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
                    handle_key(app, key.code, key.modifiers);
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Char('1') => app.active_tab = Tab::Dashboard,
        KeyCode::Char('2') => app.active_tab = Tab::Spectrum,
        KeyCode::Char('3') => app.active_tab = Tab::Processes,
        KeyCode::Tab => app.active_tab = app.active_tab.next(),
        KeyCode::BackTab => app.active_tab = app.active_tab.prev(),
        KeyCode::Char('k') | KeyCode::Up => {
            if app.active_tab == Tab::Processes {
                app.proc_cursor = app.proc_cursor.saturating_sub(1);
            } else {
                app.status_msg = app.scope.toggle_cam_pacer();
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.active_tab == Tab::Processes && !app.procmon.last.is_empty() {
                app.proc_cursor = (app.proc_cursor + 1).min(app.procmon.last.len() - 1);
            }
        }
        KeyCode::Char(' ') | KeyCode::Enter => {
            if app.active_tab == Tab::Processes {
                if let Some(p) = app.procmon.last.get(app.proc_cursor).cloned() {
                    app.status_msg = app.procmon.toggle_pause(p.pid, &p.name);
                }
            }
        }
        KeyCode::Char('u') | KeyCode::Char('U') => {
            let n = app.procmon.resume_all();
            app.status_msg = format!("Resumed {n} frozen process(es) via SIGCONT.");
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.refresh();
            app.status_msg = "Rescanned RF airspace & hop matrix.".into();
        }
        _ => {}
    }
}
