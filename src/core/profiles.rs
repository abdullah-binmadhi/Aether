//! Per-SSID profiles + password store. Ported from profiles.py.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::cmd;

fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(".config/wifi-tui/profiles.json")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Profile {
    #[serde(default = "default_true")]
    pub mute_awdl: bool,
    #[serde(default = "default_dns_mode")]
    pub dns_mode: String,
    #[serde(default = "default_true")]
    pub ipv6_enabled: bool,
    #[serde(default)]
    pub force_5ghz: bool,
    #[serde(default = "default_one")]
    pub preferred_preset: i64,
    #[serde(default)]
    pub password: String,
}

fn default_true() -> bool {
    true
}
fn default_one() -> i64 {
    1
}
fn default_dns_mode() -> String {
    "fastest".into()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GlobalCfg {
    #[serde(default = "default_true")]
    pub auto_pilot_enabled: bool,
    #[serde(default = "default_one")]
    pub active_preset_id: i64,
    #[serde(default)]
    pub zero_lag_mode: bool,
    #[serde(default)]
    pub sync_daemons_frozen: bool,
}

impl Default for GlobalCfg {
    fn default() -> Self {
        GlobalCfg {
            auto_pilot_enabled: true,
            active_preset_id: 1,
            zero_lag_mode: false,
            sync_daemons_frozen: false,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    #[serde(default)]
    global: GlobalCfg,
    #[serde(default)]
    profiles: HashMap<String, Profile>,
}

pub struct ProfileManager {
    data: Data,
}

impl ProfileManager {
    pub fn new() -> Self {
        let mut m = ProfileManager {
            data: Data::default(),
        };
        m.load();
        m
    }

    fn load(&mut self) {
        if let Ok(text) = std::fs::read_to_string(config_path()) {
            if let Ok(d) = serde_json::from_str::<Data>(&text) {
                self.data = d;
            }
        }
    }

    pub fn save(&self) -> bool {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(&self.data) {
            Ok(json) => {
                if std::fs::write(&path, json).is_ok() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                    }
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    fn ensure_profile(&mut self, ssid: &str) -> &mut Profile {
        if !self.data.profiles.contains_key(ssid) {
            let upper = ssid.to_uppercase();
            let is_high = ["5G", "6G", "FAST", "PRO"]
                .iter()
                .any(|t| upper.contains(t));
            self.data.profiles.insert(
                ssid.to_string(),
                Profile {
                    mute_awdl: true,
                    dns_mode: "fastest".into(),
                    ipv6_enabled: true,
                    force_5ghz: is_high,
                    preferred_preset: 1,
                    password: String::new(),
                },
            );
        }
        self.data.profiles.get_mut(ssid).unwrap()
    }

    pub fn get_wifi_password(&self, ssid: &str) -> String {
        if ssid.is_empty() || matches!(ssid, "Disconnected" | "Scanning..." | "N/A") {
            return String::new();
        }
        if let Some(p) = self.data.profiles.get(ssid) {
            let pw = p.password.trim();
            if !pw.is_empty() {
                return pw.to_string();
            }
        }
        // Dual-band sibling share by prefix.
        let base = ssid
            .split('.')
            .next()
            .unwrap_or(ssid)
            .split('_')
            .next()
            .unwrap_or(ssid)
            .trim()
            .to_lowercase();
        if base.len() >= 3 {
            for (k, v) in &self.data.profiles {
                if k.to_lowercase().starts_with(&base) && !v.password.trim().is_empty() {
                    return v.password.trim().to_string();
                }
            }
        }
        String::new()
    }

    pub fn set_wifi_password(&mut self, ssid: &str, password: &str) -> String {
        if ssid.is_empty() || matches!(ssid, "Disconnected" | "Scanning..." | "N/A") {
            return "[!] No active SSID selected.".into();
        }
        let pw = password.trim().to_string();
        let n = pw.len();
        self.ensure_profile(ssid).password = pw;
        self.save();
        format!("[OK] Saved password for '{ssid}' ({n} chars).")
    }

    pub fn fetch_keychain_password(&mut self, ssid: &str) -> Option<String> {
        if ssid.is_empty() || matches!(ssid, "Disconnected" | "Scanning..." | "N/A") {
            return None;
        }
        let attempts: [&[&str]; 2] = [
            &[
                "security",
                "find-generic-password",
                "-D",
                "AirPort network password",
                "-a",
                ssid,
                "-w",
            ],
            &["security", "find-generic-password", "-a", ssid, "-w"],
        ];
        for argv in attempts {
            let out = cmd::run(argv, Duration::from_secs(30));
            if out.success {
                let pw = out.stdout.trim().to_string();
                if !pw.is_empty() {
                    self.set_wifi_password(ssid, &pw);
                    return Some(pw);
                }
            }
        }
        None
    }

    pub fn toggle_auto_pilot(&mut self) -> bool {
        self.data.global.auto_pilot_enabled = !self.data.global.auto_pilot_enabled;
        self.save();
        self.data.global.auto_pilot_enabled
    }

    pub fn auto_pilot_enabled(&self) -> bool {
        self.data.global.auto_pilot_enabled
    }

    pub fn zero_lag_mode(&self) -> bool {
        self.data.global.zero_lag_mode
    }

    pub fn set_zero_lag_mode(&mut self, v: bool) {
        self.data.global.zero_lag_mode = v;
        self.save();
    }
}
