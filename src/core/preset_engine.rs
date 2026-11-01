//! Preset brain: issue detection, recommendations, tuning controls, cockpit
//! analysis. Ported from preset_engine.py.
//!
//! The Python PresetEngine held references to every other engine and mutated
//! them in `apply_preset`. To keep Rust ownership clean, the detection/analysis
//! logic (pure) lives here, and the mutating `apply_*`/`toggle_*` paths take an
//! `Engines` bundle by `&mut` from the app layer.

use super::model::{Band, Telemetry};
use super::spectrum::SpectrumReport;
use super::wan_doctor::WanSnapshot;

#[derive(Clone)]
pub struct PresetDef {
    pub id: u8,
    pub short: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
}

pub fn preset_definitions() -> Vec<PresetDef> {
    vec![
        PresetDef { id: 1, short: "1:AUTO-ADAPT", name: "◆ AUTO-ADAPTIVE VIP BRAIN", desc: "Closed-loop v5.0: heals YouTube GGC/ECS DNS, IPv6 Asymmetry, VIP WMM, CAM Pacer & 32MB BDP buffers." },
        PresetDef { id: 2, short: "2:FFXIV/LoL", name: "▸ FFXIV / LoL ZERO-JITTER VIP", desc: "25Hz CAM DSCP-EF Pacer + WMM Voice Priority + TCP delayed_ack=0 + Rexmt 30ms + AWDL Muted." },
        PresetDef { id: 3, short: "3:4K-STREAM", name: "▸ 4K ANIME/YOUTUBE & AI SURGE", desc: "32MB High-BDP TCP Window + ECS-Smart YouTube GGC DNS + IPv6 Guard + QUIC Fast-Path + MSS 1452." },
        PresetDef { id: 4, short: "4:ANTI-BLOAT", name: "▸ BUFFERBLOAT & UPLOAD CRUSHER", desc: "Clamps 1492 Fiber MTU, freezes background upload hogs (SIGSTOP) & enforces fq_codel ECN." },
        PresetDef { id: 5, short: "5:FORTRESS", name: "▸ PUBLIC / CAFE FORTRESS", desc: "Randomizes MAC (02:xx), locks Quad9+Cloudflare Anycast DNS & activates ARP MITM guard." },
        PresetDef { id: 6, short: "6:RESCUE", name: "▸ EMERGENCY WAN & LINK DOCTOR", desc: "Runs 7-stage WAN Doctor: heals ECS DNS, IPv6 guard, MTU 1492, reaps zombie sockets & re-handshakes." },
        PresetDef { id: 7, short: "7:MEETING", name: "▸ MEETING ARMOR (ZOOM/TEAMS)", desc: "Ultra-low jitter voice priority: WMM AC_VO + continuous AWDL suppression + CAM 25Hz pacer + audio anti-stutter." },
    ]
}

/// A single tuning-control row for Tab 4.
#[derive(Clone)]
pub struct Control {
    pub id: &'static str,
    pub key: &'static str,
    pub name: String,
    pub active: bool,
    pub status: String,
}

#[derive(Clone)]
pub struct CockpitAnalysis {
    pub analysis_lines: Vec<String>,
    pub recommended_action: String,
    pub workloads: Vec<(String, String, bool)>,
}

pub struct PresetEngine {
    pub selected_preset_id: u8,
    pub effective_preset_id: u8,
    pub auto_reason: String,
    pub detected_issues: Vec<String>,
    pub applied_actions: Vec<String>,
    pub before_after_summary: String,
    pub cursor_idx: usize,
    last_auto_applied: u8,
    peak_seen_jitter: f64,
}

impl PresetEngine {
    pub fn new() -> Self {
        PresetEngine {
            selected_preset_id: 1,
            effective_preset_id: 2,
            auto_reason: "Closed-Loop VIP Priority Active (LoL/FFXIV + 4K Stream + AI Ready)"
                .into(),
            detected_issues: Vec::new(),
            applied_actions: vec![
                "CAM 25Hz DSCP-EF Pacer: ON".into(),
                "VIP WMM Priority: ON".into(),
                "FFXIV No-Delay: ON".into(),
                "8MB Buffers: ON".into(),
            ],
            before_after_summary: "Probing live RTT, Jitter, Bufferbloat & Airtime telemetry..."
                .into(),
            cursor_idx: 0,
            last_auto_applied: 0,
            peak_seen_jitter: 0.0,
        }
    }

    /// Analyze telemetry and return the recommended preset id (1..7), updating
    /// `detected_issues`, `auto_reason`, and `before_after_summary`.
    #[allow(clippy::too_many_arguments)] // live scope metrics passed by value
    pub fn detect_issues_and_recommend(
        &mut self,
        tel: &Telemetry,
        scope_avg_ms: f64,
        scope_jitter_ms: f64,
        scope_loss_pct: f64,
        scope_bloat_ms: f64,
        up_kbs: f64,
        down_kbs: f64,
        arp_status: &str,
        wd: &WanSnapshot,
    ) -> u8 {
        let _ = scope_avg_ms;
        self.peak_seen_jitter = self.peak_seen_jitter.max(scope_jitter_ms);

        let band = tel.band;
        let sig = tel.signal_dbm;
        let mcs = tel.mcs_index;
        let sec = tel.security.clone();
        let ch_num = tel.channel_num;
        let has_5g_sib = tel.sibling_5g_ssid.is_some();

        let mut has_collision = false;
        for other in &tel.neighbors {
            if other.channel_num == ch_num && ch_num > 0 && other.signal_dbm >= -70 {
                has_collision = true;
                self.detected_issues.push(format!(
                    "Ch {ch_num} Co-Channel Clash ({} dBm) -> VIP WMM Mitigating",
                    other.signal_dbm
                ));
                break;
            }
        }

        let mut issues = Vec::new();
        if scope_loss_pct >= 8.0 || tel.ssid == "Disconnected" {
            issues.push(format!(
                "Packet Loss ({scope_loss_pct:.1}%) -> Link Rescue Needed"
            ));
        }
        let arp_bad = arp_status.contains("[!]") || arp_status.contains('⚠');
        if arp_bad || sec.contains("None") || sec.contains("Open") {
            issues.push("Open/Untrusted Wi-Fi -> Fortress Active".into());
        }
        if band == Band::Band2_4GHz && has_5g_sib && !tel.is_hotspot {
            issues.push("2.4GHz Band Downgrade -> Press [B] for 5GHz".into());
        }
        if band.is_fast() && sig >= -65 && mcs > 0 && mcs <= 4 {
            issues.push(format!(
                "MCS Rate Decay (MCS {mcs} @ {sig} dBm) -> CAM Pacer Warming"
            ));
        }
        if scope_bloat_ms > 20.0 || up_kbs > 350.0 {
            issues.push(format!(
                "Upload Bufferbloat (+{scope_bloat_ms:.0}ms Δ) -> Throttling Hogs"
            ));
        }
        if scope_jitter_ms > 10.0 {
            issues.push(format!(
                "RF Contention Jitter ({scope_jitter_ms:.1}ms) -> 25Hz CAM DSCP-EF Active"
            ));
        }
        if tel.captive_portal {
            issues.push(tel.captive_summary.clone());
        }
        if tel.sticky_client {
            issues.push(tel.roam_verdict.clone());
        }
        if tel.tx_retry_pct >= 6.0 {
            issues.push(format!(
                "High L2 Retries ({:.1}%) -> VIP WMM Mitigating",
                tel.tx_retry_pct
            ));
        }
        if tel.is_hotspot && tel.hotspot_armor_active {
            issues.push("Hotspot Armor Active (MTU 1440B + Scavenger QoS)".into());
        }
        if !wd.ecs_dns_optimal {
            issues.push("ECS-Stripping Primary DNS (1.1.1.1) -> Misroutes YouTube GGC".into());
        }
        if wd.ipv6_asymmetric {
            issues.push(format!(
                "IPv6 Route Asymmetry ({:.0}ms vs {:.0}ms IPv4) -> Guard Needed",
                wd.ipv6_rtt_ms, wd.yt_rtt_ms
            ));
        }
        if wd.zombie_socket_count >= 5 {
            let p = wd.zombie_procs.first().cloned().unwrap_or_default();
            issues.push(format!(
                "{} Stuck Zombie Sockets ({p}) -> Press [Z]",
                wd.zombie_socket_count
            ));
        }
        if issues.is_empty() {
            let yt = if wd.yt_rtt_ms > 0.0 {
                format!(" │ YT {:.0}ms/{:.1}M", wd.yt_rtt_ms, wd.yt_single_mbps)
            } else {
                String::new()
            };
            issues.push(format!(
                "[OK] Zero Critical Anomalies (VIP Airtime + ECS GGC + IPv6 Guard{yt})"
            ));
        }
        self.detected_issues = issues;

        let tx_r = tel.tx_rate_mbps;
        let yt_info = if wd.yt_rtt_ms > 0.0 {
            format!(" │ YT: {:.0}ms ({:.1}M)", wd.yt_rtt_ms, wd.yt_single_mbps)
        } else {
            String::new()
        };
        self.before_after_summary = if self.peak_seen_jitter > 5.0 {
            format!(
                "Peak Jitter: {:.1}ms -> Current: {:.1}ms │ Link: {tx_r}M (MCS {mcs}){yt_info}",
                self.peak_seen_jitter, scope_jitter_ms
            )
        } else {
            format!(
                "Live Jitter: {:.1}ms │ Bloat Δ: +{:.1}ms │ Link: {tx_r}M (MCS {mcs}){yt_info}",
                scope_jitter_ms, scope_bloat_ms
            )
        };

        // Recommendation ladder.
        if scope_loss_pct >= 8.0 {
            self.auto_reason = format!("Auto-Triggered by Packet Loss ({scope_loss_pct:.1}%)");
            return 6;
        }
        if arp_bad || sec.contains("None") || sec.contains("Open") {
            self.auto_reason = "Auto-Triggered by Open/Untrusted Network Security".into();
            return 5;
        }
        let two_four_down = band == Band::Band2_4GHz && has_5g_sib && !tel.is_hotspot;
        let mcs_decay = band.is_fast() && sig >= -65 && mcs > 0 && mcs <= 3;
        if two_four_down || mcs_decay || down_kbs > 1200.0 {
            self.auto_reason = if two_four_down {
                "Auto-Triggered by 2.4GHz Downgrade (5GHz Sibling Available)".into()
            } else {
                format!("Auto-Triggered for High-Speed 4K Stream / MCS={mcs} Recovery")
            };
            return 3;
        }
        if scope_bloat_ms > 20.0 || up_kbs > 350.0 {
            self.auto_reason =
                format!("Auto-Triggered by Upload Bufferbloat (+{scope_bloat_ms:.0}ms)");
            return 4;
        }
        self.auto_reason = if has_collision {
            format!("Ch {ch_num} Collision Detected -> Enforcing VIP WMM + 25Hz CAM Pacer")
        } else {
            "Locked for Lowest Jitter (LoL/FFXIV + AI Coding + 4K Ready)".into()
        };
        2
    }

    /// Build the Tab 4 control list (11 real controls + root lock = 12 entries).
    pub fn get_tuning_controls(
        &self,
        tel: &Telemetry,
        ks: &std::collections::HashMap<String, String>,
        wd: &WanSnapshot,
        root_authorized: bool,
        sync_frozen: bool,
    ) -> Vec<Control> {
        let zero_lag = tel.zero_lag_mode;
        let cam_on = tel.cam_pacer_active;
        let wmm_on = ks
            .get("wmm_vip_active")
            .map(|v| v == "true")
            .unwrap_or(true);
        let nodelay_on = ks
            .get("ffxiv_nodelay_active")
            .map(|v| v == "true")
            .unwrap_or(true);
        let buf8_on = ks
            .get("buffers_8mb_active")
            .map(|v| v == "true")
            .unwrap_or(true);
        let band_5g = tel.band.is_fast();
        let mtu = tel.mtu;
        let sib_5g = tel.sibling_5g_ssid.clone().unwrap_or_else(|| "5GHz".into());
        let dns0 = tel
            .dns_servers
            .first()
            .cloned()
            .unwrap_or_else(|| "Auto".into());
        let root_ok = root_authorized || ks.get("wmm_wifi").map(|v| v == "1").unwrap_or(false);
        let ipv6_guarded = wd.ipv6_mode == "Link-Local Guard" || !tel.ipv6_active;
        let delayed_ack = ks.get("delayed_ack").cloned().unwrap_or_else(|| "0".into());

        vec![
            Control {
                id: "wmm_vip",
                key: "W",
                name: "802.11e WMM VIP Airtime Priority (CWmin=3)".into(),
                active: wmm_on,
                status: if wmm_on {
                    "VIP AC_VO/VI ON".into()
                } else {
                    "BEST-EFFORT".into()
                },
            },
            Control {
                id: "cam_pacer",
                key: "K",
                name: "25Hz CAM Anti-Sleep & DSCP-EF Radio Pacer".into(),
                active: cam_on,
                status: if cam_on {
                    "25Hz ACTIVE (<0.5KB/s)".into()
                } else {
                    "PAUSED (DTIM Sleep)".into()
                },
            },
            Control {
                id: "ffxiv_nodelay",
                key: "X",
                name: "FFXIV/LoL TCP No-Delay & 30ms Fast Rexmt".into(),
                active: nodelay_on,
                status: if nodelay_on {
                    format!("ACK={delayed_ack} REXMT=30ms")
                } else {
                    "DEFAULT (ACK=3)".into()
                },
            },
            Control {
                id: "buf_8mb",
                key: "O",
                name: "32MB High-BDP YouTube/4K Buffers + MSS 1452".into(),
                active: buf8_on,
                status: if buf8_on {
                    "32MB BDP + MSS 1452".into()
                } else {
                    "4MB STANDARD".into()
                },
            },
            Control {
                id: "wan_doctor",
                key: "D",
                name: "ECS-Smart YouTube GGC DNS + WAN Auto-Doctor".into(),
                active: wd.ecs_dns_optimal,
                status: if wd.ecs_dns_optimal {
                    format!("ECS GGC ({dns0})")
                } else {
                    "MISROUTED (1.1.1.1)".into()
                },
            },
            Control {
                id: "ipv6_guard",
                key: "6",
                name: "IPv6 Asymmetry & Zombie Prefix Guard".into(),
                active: ipv6_guarded,
                status: if ipv6_guarded {
                    "LINK-LOCAL GUARD".into()
                } else {
                    format!("AUTO ({:.0}ms)", wd.ipv6_rtt_ms)
                },
            },
            Control {
                id: "quic_fastpath",
                key: "Y",
                name: "YouTube QUIC->HTTP/2 Fast-Path (Anti-UDP Policer)".into(),
                active: wd.quic_bypass_active,
                status: if wd.quic_bypass_active {
                    "TCP HTTP/2 ENFORCED".into()
                } else {
                    "UDP QUIC DEFAULT".into()
                },
            },
            Control {
                id: "zero_lag",
                key: "L",
                name: "Zero-Lag AWDL & AirDrop Hopping Silencer".into(),
                active: zero_lag,
                status: if zero_lag {
                    "MUTED (Zero-Lag)".into()
                } else {
                    "HOPPING (Standard)".into()
                },
            },
            Control {
                id: "sync_freeze",
                key: "Z",
                name: "Zombie Socket Reaper & Sync Daemon Freezer".into(),
                active: sync_frozen && wd.zombie_socket_count < 5,
                status: if wd.zombie_socket_count == 0 {
                    "CLEAN (0 Stuck)".into()
                } else {
                    format!("{} STUCK SOCKETS", wd.zombie_socket_count)
                },
            },
            Control {
                id: "band_5g",
                key: "B",
                name: format!("5GHz/6GHz High-Speed Sibling Lock ({sib_5g})"),
                active: band_5g,
                status: if band_5g {
                    format!("LOCKED ({})", tel.band.label())
                } else {
                    "2.4GHz SLOW".into()
                },
            },
            Control {
                id: "mtu_clamp",
                key: "T",
                name: "Fiber/PPPoE Packet Fragmentation Clamper".into(),
                active: mtu == 1492,
                status: if mtu == 1492 {
                    format!("CLAMPED ({mtu} B)")
                } else {
                    format!("STANDARD ({mtu} B)")
                },
            },
            Control {
                id: "root_auth",
                key: "S",
                name: "One-Time Permanent Kernel Sudoers Lock".into(),
                active: root_ok,
                status: if root_ok {
                    "KERNEL LOCKED".into()
                } else {
                    "PRESS [S] ONCE".into()
                },
            },
        ]
    }

    #[allow(clippy::too_many_arguments)] // live scope metrics passed by value
    pub fn generate_cockpit_analysis(
        &self,
        tel: &Telemetry,
        scope_jitter_ms: f64,
        scope_bloat_ms: f64,
        scope_avg_ms: f64,
        cam_on: bool,
        spectrum: &SpectrumReport,
        wd: &WanSnapshot,
        root_authorized: bool,
    ) -> CockpitAnalysis {
        let ch_num = tel.channel_num;
        let mcs = tel.mcs_index;
        let tx_rate = tel.tx_rate_mbps;
        let snr = tel.snr_db;
        let air_load = tel.airtime_load_pct;
        let retry_pct = tel.tx_retry_pct;

        let mut lines = Vec::new();

        // macOS badge line.
        if !tel.apple_probe_ok && !tel.apple_probe_detail.is_empty() {
            lines.push(format!(
                "[!] macOS [!] Exclamation Mark: {}. Press [F] to restore DHCP DNS.",
                tel.apple_probe_detail
            ));
        } else if tel.macos_weak_security {
            lines.push("[!] macOS Weak Security: Router uses legacy WPA/TKIP. Switch router to WPA2/WPA3 Personal (AES) to clear badge.".into());
        } else if !tel.macos_private_mac
            && !matches!(tel.ssid.as_str(), "Disconnected" | "Radio Off" | "")
        {
            lines.push("[i] macOS Privacy Warning: Private Wi-Fi address is OFF. Enable in macOS Settings > Wi-Fi > Network Details.".into());
        }

        // Line 1.
        if tel.is_hotspot {
            let armor = if tel.hotspot_armor_active {
                "ARMOR ON (MTU 1440B)"
            } else {
                "Detected"
            };
            lines.push(format!(
                "1. Cellular Hotspot {armor}: Ch {ch_num} (SNR {snr}dB) │ {}",
                tel.roam_verdict
            ));
        } else if !spectrum.collisions.is_empty() {
            let col_sig = spectrum.collisions[0].signal_dbm;
            lines.push(format!(
                "1. Ch {ch_num} Clash ({col_sig}dBm): Neighbor shares block. {}",
                tel.channel_advisor_line
            ));
        } else {
            lines.push(format!(
                "1. Airspace Clean (Ch {ch_num}, SNR {snr}dB): {}",
                tel.roam_verdict
            ));
        }

        // Line 2.
        if cam_on {
            lines.push(format!("2. CAM 25Hz [K] ON: Jit {scope_jitter_ms:.1}ms │ Link {tx_rate}M (MCS {mcs}) │ Airtime {air_load}% (Retry {retry_pct:.1}%)."));
        } else {
            lines.push("2. CAM Pacer PAUSED: Radio may enter DTIM sleep during idle. Press [K] to lock <2ms jitter.".into());
        }

        // Line 3.
        let yt_rtt = if wd.yt_rtt_ms > 0.0 {
            wd.yt_rtt_ms
        } else {
            scope_avg_ms
        };
        let yt_mbps = if wd.yt_single_mbps > 0.0 {
            format!("{:.1}M", wd.yt_single_mbps)
        } else {
            "Probing".into()
        };
        lines.push(format!("3. YouTube GGC ({}): RTT {yt_rtt:.0}ms │ Stream {yt_mbps} │ IPv6 {} │ Bloat +{scope_bloat_ms:.1}ms.", wd.ggc_node, wd.ipv6_mode));

        // Line 4.
        if !tel.history_summary.is_empty() {
            lines.push(format!("4. {}", tel.history_summary));
        }

        // Recommended action.
        let rec = if !tel.apple_probe_ok {
            ">> [!] macOS [!] EXCLAMATION MARK ACTIVE: Press [F] to restore dynamic DHCP DNS & resolve Apple probe.".to_string()
        } else if tel.captive_portal {
            ">> [!] CAPTIVE PORTAL DETECTED: Press [C] to Breakout DNS & Open Login Page."
                .to_string()
        } else if tel.sticky_client {
            format!(
                ">> {} — Press [R] to Smart-Roam to stronger AP.",
                tel.roam_verdict
            )
        } else if !wd.ecs_dns_optimal || wd.ipv6_asymmetric {
            ">> Press [D] to run Full WAN & YouTube Auto-Doctor (Fixes ECS GGC DNS + IPv6 Route Guard).".to_string()
        } else if !root_authorized {
            ">> Press [S] once to lock 32MB High-BDP Kernel TCP Window + WMM VIP, [D] Doctor, [J] Export Ticket.".to_string()
        } else if !spectrum.collisions.is_empty() {
            format!(">> VIP WMM + 32MB BDP locked! Optional: move router 5GHz to Ch {} or press [5] SpeedTest.", spectrum.recommended_5g_channel)
        } else {
            ">> All VIP Priority, ECS GGC & 32MB BDP modules active! [D] Doctor │ [J] Export ISP Ticket.".to_string()
        };

        // Workloads.
        let lol_ok = scope_jitter_ms <= 8.0 && scope_bloat_ms <= 15.0;
        let yt_ok = (tx_rate >= 70 || tel.is_hotspot) && wd.ecs_dns_optimal && !wd.ipv6_asymmetric;
        let ai_ok = !tel.dns_servers.is_empty();
        let workloads = vec![
            (
                "LoL / FFXIV Gaming".to_string(),
                if lol_ok {
                    "[OK] VIP READY (0% Stall)".to_string()
                } else {
                    format!("[!] JIT {scope_jitter_ms:.0}ms (CAM Mitigating)")
                },
                lol_ok,
            ),
            (
                "YouTube 1080p60/4K".to_string(),
                if yt_ok {
                    format!("[OK] {yt_rtt:.0}ms GGC ({yt_mbps} ECS)")
                } else {
                    "[!] MISROUTED (Press [D])".to_string()
                },
                yt_ok,
            ),
            (
                "AI Coding (SSE/API)".to_string(),
                if ai_ok {
                    format!(
                        "[OK] DNS {} + TFO",
                        tel.dns_servers
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "Auto".into())
                    )
                } else {
                    "[!] Unbound DNS".to_string()
                },
                ai_ok,
            ),
        ];

        CockpitAnalysis {
            analysis_lines: lines,
            recommended_action: rec,
            workloads,
        }
    }

    pub fn cycle_preset(&mut self) -> u8 {
        let n = preset_definitions().len() as u8;
        self.selected_preset_id = (self.selected_preset_id % n) + 1;
        self.selected_preset_id
    }

    pub fn mission_to_preset(mission: &str) -> u8 {
        match mission.to_lowercase().trim() {
            "meeting" | "zoom" | "teams" => 7,
            "gaming" | "competitive" | "lol" | "ffxiv" => 2,
            "stream" | "4k" | "youtube" => 3,
            "bloat" | "anti-bloat" => 4,
            "public" | "fortress" | "cafe" => 5,
            "rescue" | "doctor" => 6,
            _ => 2,
        }
    }

    pub fn mark_auto_applied(&mut self, id: u8) {
        self.last_auto_applied = id;
    }
    pub fn last_auto_applied(&self) -> u8 {
        self.last_auto_applied
    }
}
