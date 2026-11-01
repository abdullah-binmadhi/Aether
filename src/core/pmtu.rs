//! Path-MTU discovery & MSS clamp. Ported from pmtu_doctor.py.

use std::time::Duration;

use super::cmd;

pub struct PmtuDiag {
    pub target: String,
    pub interface_mtu: i64,
    pub wire_mtu: i64,
    pub kernel_mss: i64,
    pub optimal_mss: i64,
    pub is_blackhole: bool,
    pub status: String,
    pub diagnosis: String,
}

pub struct PmtuDoctor {
    interface: String,
    default_target: String,
}

impl PmtuDoctor {
    pub fn new(interface: &str) -> Self {
        PmtuDoctor {
            interface: interface.to_string(),
            default_target: "1.1.1.1".into(),
        }
    }

    pub fn get_interface_mtu(&self) -> i64 {
        let out = cmd::run(&["ifconfig", &self.interface], Duration::from_millis(1500));
        for tok in out.stdout.split_whitespace().collect::<Vec<_>>().windows(2) {
            if tok[0] == "mtu" {
                if let Ok(n) = tok[1].parse::<i64>() {
                    return n;
                }
            }
        }
        1500
    }

    pub fn get_kernel_mss(&self) -> i64 {
        let out = cmd::run(
            &["sysctl", "-n", "net.inet.tcp.mssdflt"],
            Duration::from_millis(1000),
        );
        out.stdout.trim().parse::<i64>().unwrap_or(1460)
    }

    /// (ok, signature). ok means a DF ping of `payload_size` bytes got through.
    fn probe_df_ping(&self, payload_size: i64, target: Option<&str>) -> (bool, String) {
        let dest = target.unwrap_or(&self.default_target);
        let ps = payload_size.to_string();
        let out = cmd::run(
            &["ping", "-D", "-c", "1", "-W", "1200", "-s", &ps, dest],
            Duration::from_millis(2000),
        );
        let combined = format!("{}{}", out.stdout, out.stderr);
        let low = combined.to_lowercase();
        // Preserve Python precedence: (rc==0 and "1 packets received") or "1 received".
        if (out.success && combined.contains("1 packets received"))
            || combined.contains("1 received")
        {
            (true, combined)
        } else if combined.contains("Message too long")
            || low.contains("frag needed")
            || low.contains("too large")
        {
            (false, "FragNeeded".into())
        } else {
            (false, "Dropped".into())
        }
    }

    fn binary_search_path_mtu(&self, target: Option<&str>, min_mtu: i64, max_mtu: i64) -> i64 {
        let mut low = min_mtu - 28;
        let mut high = max_mtu - 28;
        let mut best = low;
        if self.probe_df_ping(high, target).0 {
            return max_mtu;
        }
        while low <= high {
            let mid = (low + high) / 2;
            if self.probe_df_ping(mid, target).0 {
                best = mid;
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        best + 28
    }

    pub fn diagnose(&self, target: Option<&str>) -> PmtuDiag {
        let dest = target.unwrap_or(&self.default_target).to_string();
        let iface_mtu = self.get_interface_mtu();
        let kernel_mss = self.get_kernel_mss();
        let wire_mtu = self.binary_search_path_mtu(Some(&dest), 1280, 1500);
        let optimal_mss = wire_mtu - 40;

        let mut is_blackhole = false;
        if wire_mtu < iface_mtu {
            let (_, sig) = self.probe_df_ping(iface_mtu - 28, Some(&dest));
            if sig == "Dropped" {
                is_blackhole = true;
            }
        }

        let status = if wire_mtu == 1500 {
            "[OK] Standard Gigabit / Fiber MTU (1500B)".to_string()
        } else if wire_mtu == 1492 {
            "[!] PPPoE DSL / Fiber MTU Detected (1492B)".to_string()
        } else if wire_mtu <= 1420 {
            format!("[!] VPN / Encapsulation MTU Detected ({wire_mtu}B)")
        } else {
            format!("[!] Constrained Path MTU ({wire_mtu}B)")
        };

        let mut parts = vec![if wire_mtu == 1500 {
            "Clean wire MTU 1500B — No fragmentation overhead.".to_string()
        } else if wire_mtu == 1492 {
            "ISP uses PPPoE encapsulation (8B overhead). Wire MTU is 1492B.".to_string()
        } else if wire_mtu <= 1420 {
            format!(
                "Wire MTU restricted to {wire_mtu}B (likely WireGuard, IPSec, or CGNAT tunnel)."
            )
        } else {
            format!("Discovered Path MTU of {wire_mtu}B on path to {dest}.")
        }];
        if is_blackhole {
            parts.push(format!("SILENT BLACK HOLE DETECTED: Intermediate router drops packets >{wire_mtu}B without returning ICMP FragNeeded. TCP connections will stall unless MSS is clamped."));
        } else if iface_mtu > wire_mtu {
            parts.push(format!("MTU Mismatch: en0 is set to {iface_mtu}B but wire allows {wire_mtu}B. Clamping kernel MSS to {optimal_mss}B will prevent fragmentation."));
        }

        PmtuDiag {
            target: dest,
            interface_mtu: iface_mtu,
            wire_mtu,
            kernel_mss,
            optimal_mss,
            is_blackhole,
            status,
            diagnosis: parts.join(" │ "),
        }
    }

    pub fn clamp_tcp_mss(&self, optimal_mss: i64) -> (bool, String) {
        let pair = format!("net.inet.tcp.mssdflt={optimal_mss}");
        if cmd::ok(
            &["sudo", "-n", "sysctl", "-w", &pair],
            Duration::from_millis(1500),
        ) {
            return (true, format!("[OK] Clamped kernel TCP MSS to {optimal_mss}B (`net.inet.tcp.mssdflt={optimal_mss}`)"));
        }
        let out = cmd::run(&["sysctl", "-w", &pair], Duration::from_millis(1500));
        if out.success {
            (true, format!("[OK] Clamped kernel TCP MSS to {optimal_mss}B (`net.inet.tcp.mssdflt={optimal_mss}`)"))
        } else {
            (
                false,
                format!(
                    "Failed to clamp TCP MSS (requires root): {}",
                    out.stderr.trim()
                ),
            )
        }
    }

    pub fn auto_heal(&self, target: Option<&str>) -> (bool, String) {
        let diag = self.diagnose(target);
        if diag.wire_mtu < diag.interface_mtu
            || diag.is_blackhole
            || diag.kernel_mss > diag.optimal_mss
        {
            let (ok, msg) = self.clamp_tcp_mss(diag.optimal_mss);
            (ok, format!("{} ── {}", diag.status, msg))
        } else {
            (
                true,
                format!(
                    "{} ── Wire MTU and Kernel MSS ({}B) are Optimal.",
                    diag.status, diag.kernel_mss
                ),
            )
        }
    }
}
