//! 4-tier autonomous self-healing orchestrator. Ported from orchestrator.py.

use std::time::Duration;

use super::antilag::AntiLagEngine;
use super::cmd;
use super::gateway::GatewayRebooter;
use super::hops::ScopeSnapshot;
use super::kernel_tuner::KernelTuner;
use super::model::{Band, Telemetry};
use super::pmtu::PmtuDoctor;
use super::rf_inference;
use super::roaming;
use super::security_guard::SecurityGuard;
use super::ticket;

pub struct HealthEval {
    pub health_score: i64,
    pub is_healthy: bool,
    pub degradations: Vec<String>,
    pub recommended_tier: u8,
}

pub struct TierResult {
    pub success: bool,
    pub tier_executed: u8,
    pub summary: String,
    pub actions: Vec<String>,
}

pub struct Orchestrator {
    pub interface: String,
    pub last_tier_executed: u8,
    kernel: KernelTuner,
    antilag: AntiLagEngine,
    pmtu: PmtuDoctor,
    gateway: GatewayRebooter,
    security: SecurityGuard,
}

impl Orchestrator {
    pub fn new(interface: &str, service: &str) -> Self {
        Orchestrator {
            interface: interface.to_string(),
            last_tier_executed: 0,
            kernel: KernelTuner::new(service, interface),
            antilag: AntiLagEngine::new(interface),
            pmtu: PmtuDoctor::new(interface),
            gateway: GatewayRebooter::new(""),
            security: SecurityGuard::new(interface),
        }
    }

    pub fn evaluate_health(&mut self, tel: &Telemetry, scope: &ScopeSnapshot) -> HealthEval {
        let avg_ms = scope.avg_ms;
        let jitter_ms = scope.jitter_ms;
        let loss_pct = scope.loss_pct;
        let bloat_ms = tel.hop2_bloat_ms;
        let tx_retry = tel.tx_retry_pct;
        let wan_online = true; // derived elsewhere; assume online unless offline band
        let wan_online = wan_online && tel.band != Band::Offline;

        let mut score = 100i64;
        let mut degr = Vec::new();

        if !wan_online || loss_pct >= 50.0 {
            score -= 60;
            degr.push("Severe WAN packet loss / offline".into());
        } else if loss_pct >= 5.0 {
            score -= 30;
            degr.push(format!("High packet loss ({loss_pct:.1}%)"));
        }
        if avg_ms > 100.0 {
            score -= 25;
            degr.push(format!("Elevated latency ({avg_ms:.1}ms)"));
        } else if avg_ms > 50.0 {
            score -= 10;
        }
        if jitter_ms > 5.0 {
            score -= 15;
            degr.push(format!("High airlink jitter ({jitter_ms:.1}ms)"));
        }
        if bloat_ms > 18.0 {
            score -= 20;
            degr.push(format!("Hop 2 Bufferbloat (+{bloat_ms:.1}ms)"));
        }
        if tx_retry > 15.0 {
            score -= 15;
            degr.push(format!("High L2 frame retries ({tx_retry:.1}%)"));
        }
        let rf = rf_inference::run_full(tel);
        if rf.has_interference {
            score -= 15;
            degr.push(format!("RF Interference: {}", rf.summary));
        }
        if tel.security_score < 70 {
            score -= 20;
            degr.push(format!(
                "Security Alert: Posture {}/100",
                tel.security_score
            ));
        }
        score = score.clamp(0, 100);
        let is_healthy = score >= 85 && degr.is_empty();

        let recommended_tier = if !wan_online || loss_pct >= 40.0 {
            4
        } else if tel.sticky_client || (tx_retry > 20.0 && rf.has_interference) {
            3
        } else if bloat_ms > 15.0 || score < 70 {
            2
        } else if jitter_ms > 5.0 || score < 85 {
            1
        } else {
            0
        };

        HealthEval {
            health_score: score,
            is_healthy,
            degradations: degr,
            recommended_tier,
        }
    }

    pub fn execute_tier(&mut self, tier: u8, tel: &Telemetry, scope: &ScopeSnapshot) -> TierResult {
        let gw = if tel.gateway != "N/A" {
            tel.gateway.clone()
        } else {
            String::new()
        };
        let mut actions = Vec::new();
        match tier {
            1 => {
                self.antilag.toggle_awdl(None);
                actions.push("AWDL AirDrop Muted (Zero-Lag Active)".into());
                if !gw.is_empty() {
                    self.security.flush_arp_cache(&gw);
                    actions.push(format!("Gateway ARP Refreshed ({gw})"));
                }
                cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(1500));
                cmd::run(
                    &["killall", "-HUP", "mDNSResponder"],
                    Duration::from_millis(1500),
                );
                actions.push("Local DNS & mDNSResponder Flushed".into());
                self.last_tier_executed = 1;
                TierResult {
                    success: true,
                    tier_executed: 1,
                    summary: "[OK] Tier 1 Executed: Client PHY/MAC Pacing & Cleaning Complete."
                        .into(),
                    actions,
                }
            }
            2 => {
                self.antilag.toggle_awdl(None);
                let (_, msg) = self.pmtu.auto_heal(None);
                actions.push(format!("PMTU Clamping: {msg}"));
                self.kernel.toggle_ffxiv_tcp_nodelay(Some(true));
                self.kernel.toggle_wmm_vip_priority(Some(true));
                self.kernel.toggle_8mb_streaming_buffers(Some(true));
                actions.push("Kernel TCP Fast-ACK + 8MB Buffers + WMM VIP Enabled".into());
                let n = AntiLagEngine::debloat_background_queues();
                if n > 0 {
                    actions.push(format!("Taskpolicy QoS: Clamped {n} background daemons"));
                }
                self.last_tier_executed = 2;
                TierResult {
                    success: true,
                    tier_executed: 2,
                    summary: "[OK] Tier 2 Executed: Kernel Transport & Queue De-Bloating Complete."
                        .into(),
                    actions,
                }
            }
            3 => {
                let eval = roaming::evaluate_roaming(tel);
                actions.push(format!("Roaming Evaluation: {}", eval.verdict));
                if tel.band == Band::Band2_4GHz {
                    actions.push("RF Steering Guide: Recommend migrating to 5GHz UNII-1/3 for cleaner spectrum".into());
                } else {
                    actions.push(format!("RF State: Verified on 5GHz Ch {}", tel.channel_num));
                }
                cmd::run(
                    &["ipconfig", "set", &self.interface, "DHCP"],
                    Duration::from_millis(2000),
                );
                actions.push("Link-Safe DHCP Lease Soft-Renewed".into());
                self.last_tier_executed = 3;
                TierResult {
                    success: true,
                    tier_executed: 3,
                    summary: "[OK] Tier 3 Executed: Radio Band & AP Steering Complete.".into(),
                    actions,
                }
            }
            4 => {
                // The Python Tier 4 called a missing method; the correct mapping is
                // ForceTermination via the UPnP gateway rebooter.
                let (_, upnp_msg) = self.gateway.force_wan_reconnect();
                actions.push(format!("UPnP Gateway NAT Flush: {upnp_msg}"));
                let (path, ticket_msg) =
                    ticket::export_report(tel, scope, Some(&tel.history_summary));
                actions.push(format!("ISP Proof Ticket Exported: {path}"));
                self.last_tier_executed = 4;
                TierResult {
                    success: true,
                    tier_executed: 4,
                    summary: format!("[OK] Tier 4 Executed: Gateway NAT Flushed & ISP Ticket Generated ({ticket_msg})."),
                    actions,
                }
            }
            _ => TierResult {
                success: false,
                tier_executed: 0,
                summary: "[!] Invalid Tier Specified.".into(),
                actions: vec![],
            },
        }
    }

    pub fn run(
        &mut self,
        tel: &Telemetry,
        scope: &ScopeSnapshot,
        force_tier: Option<u8>,
    ) -> TierResult {
        let eval = self.evaluate_health(tel, scope);
        let target = force_tier.unwrap_or(eval.recommended_tier);
        if target == 0 {
            return TierResult {
                success: true,
                tier_executed: 0,
                summary: "[OK] Airlink & Transport Optimal — No escalation needed.".into(),
                actions: vec!["Zero-Defect Link Confirmed".into()],
            };
        }
        self.execute_tier(target, tel, scope)
    }
}
