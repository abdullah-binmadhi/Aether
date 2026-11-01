//! LaunchAgent install/uninstall/status. Ported from daemon.py.

use std::time::Duration;

use super::cmd;

const LABEL: &str = "com.antigravity.wifi-tui";

fn plist_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn log_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(".config/wifi-tui/daemon.log")
}

fn generate_plist() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "wifi-tui".into());
    let log = log_path().display().to_string();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key><array><string>{exe}</string><string>--daemon</string></array>
    <key>EnvironmentVariables</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#
    )
}

pub fn install() -> String {
    let path = plist_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Some(dir) = log_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(&path, generate_plist()).is_err() {
        return format!(
            "[!] Failed to install LaunchAgent: write error ({})",
            path.display()
        );
    }
    let ps = path.display().to_string();
    cmd::run(&["launchctl", "unload", &ps], Duration::from_millis(2000));
    cmd::run(
        &["launchctl", "load", "-w", &ps],
        Duration::from_millis(3000),
    );
    format!("[OK] Installed & started LaunchAgent: {ps}")
}

pub fn uninstall() -> String {
    let path = plist_path();
    let ps = path.display().to_string();
    if path.exists() {
        cmd::run(
            &["launchctl", "unload", "-w", &ps],
            Duration::from_millis(2000),
        );
        let _ = std::fs::remove_file(&path);
        format!("[OK] Uninstalled LaunchAgent: {ps}")
    } else {
        "[OK] LaunchAgent was not installed.".into()
    }
}

pub fn status() -> String {
    let path = plist_path();
    let installed = path.exists();
    let list = cmd::run(&["launchctl", "list"], Duration::from_millis(2000));
    let loaded = list.stdout.contains(LABEL);
    let state = if loaded {
        "ACTIVE (RUNNING)"
    } else if installed {
        "INSTALLED (STOPPED)"
    } else {
        "NOT INSTALLED"
    };
    format!("LaunchAgent `{LABEL}`: {state} │ Plist: {}", path.display())
}

/// Autonomous self-healing daemon loop. Ported from daemon.run_autonomous_daemon.
/// This is what the installed LaunchAgent executes via `wifi-tui --daemon`.
pub fn run_autonomous_daemon(interface: &str, interval_sec: u64, max_ticks: Option<u64>) -> i32 {
    use super::antilag::AntiLagEngine;
    use super::collector::Collector;
    use super::history::HistoryLogger;
    use super::hotspot::HotspotEngine;
    use super::kernel_tuner::KernelTuner;
    use super::power_hook::WakeReconnector;
    use super::profiles::ProfileManager;
    use super::roaming;
    use super::wan_doctor::WanDoctor;
    use super::PingScope;

    let mut collector = Collector::new(interface);
    let mut scope = PingScope::new(interface);
    let mut kernel = KernelTuner::new("Wi-Fi", interface);
    let mut antilag = AntiLagEngine::new(interface);
    let mut wan_doctor = WanDoctor::new(interface, "Wi-Fi");
    let mut hotspot = HotspotEngine::new();
    let mut wake = WakeReconnector::new(interface);
    let mut history = HistoryLogger::new();
    let profiles = ProfileManager::new();

    let mut last_ssid = String::new();
    let mut last_gw = String::new();
    let mut tick = 0u64;

    println!(
        "// WIFI COCKPIT AUTONOMOUS SELF-HEALING DAEMON STARTED (Interval: {interval_sec}s) //"
    );
    loop {
        tick += 1;
        let snap = scope.probe();
        let tel = collector.collect(snap.avg_ms, snap.loss_pct);
        let curr_ssid = tel.ssid.clone();
        let curr_gw = tel.gateway.clone();

        // 1. Lid-open / wake-from-sleep recovery.
        if let Some(msg) = wake.check_and_heal_wake(if last_ssid.is_empty() {
            &curr_ssid
        } else {
            &last_ssid
        }) {
            println!("[{}] {msg}", clock());
        }

        // 2. Offline recovery OR network-switch auto-doctor + hotspot armor.
        if !tel.radio_on || !tel.link_active {
            let (_, doc) = wan_doctor.run_auto_doctor(Some(&mut kernel), false);
            println!("[{}] Offline Recovery -> {doc}", clock());
        } else if curr_ssid != last_ssid || curr_gw != last_gw {
            last_ssid = curr_ssid.clone();
            last_gw = curr_gw.clone();
            if let Some(hot) = hotspot.evaluate_auto_armor(&tel, true, Some(&mut antilag)) {
                println!("[{}] {hot}", clock());
            }
            let (_, doc) = wan_doctor.run_auto_doctor(Some(&mut kernel), false);
            println!("[{}] Network Switch ({curr_ssid}) -> {doc}", clock());
        }

        // 3. Periodic sticky-client roam + dead-zone logging.
        if tick.is_multiple_of(4) {
            let roam = roaming::evaluate_roaming(&tel);
            if roam.sticky_client {
                if let Some(best) = &roam.best_candidate {
                    println!(
                        "[{}] Roam suggested -> '{}' ({} Ch{})",
                        clock(),
                        best.ssid,
                        best.band.label(),
                        best.channel_num
                    );
                }
            }
        }
        history.record_sample(
            &tel,
            snap.avg_ms,
            snap.jitter_ms,
            snap.loss_pct,
            snap.max_bloat_delta_ms,
            0.0,
        );
        let _ = &profiles;

        if max_ticks.map(|m| tick >= m).unwrap_or(false) {
            break;
        }
        std::thread::sleep(Duration::from_secs(interval_sec));
    }
    0
}

fn clock() -> String {
    let out = cmd::run(&["date", "+%H:%M:%S"], Duration::from_millis(500));
    let s = out.stdout.trim().to_string();
    if s.is_empty() {
        "--:--:--".into()
    } else {
        s
    }
}
