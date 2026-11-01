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
