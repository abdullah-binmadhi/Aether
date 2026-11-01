//! DNS resolver racer & applier. Ported from dns_engine.py.

use std::time::{Duration, Instant};

use super::cmd;

#[derive(Clone)]
pub struct DnsCandidate {
    pub name: String,
    pub primary: String,
    pub secondary: String,
    pub ecs_local: bool,
}

#[derive(Clone)]
pub struct DnsResult {
    pub name: String,
    pub primary: String,
    pub secondary: String,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub reachable: bool,
}

const TEST_DOMAINS: &[&str] = &["apple.com", "cloudflare.com", "google.com"];

fn static_anycast() -> Vec<DnsCandidate> {
    vec![
        cand("Cloudflare Anycast", "1.1.1.1", "1.0.0.1"),
        cand("Quad9 Security", "9.9.9.9", "149.112.112.112"),
        cand("Google Public DNS", "8.8.8.8", "8.8.4.4"),
        cand("Cisco OpenDNS", "208.67.222.222", "208.67.220.220"),
    ]
}

fn cand(name: &str, p: &str, s: &str) -> DnsCandidate {
    DnsCandidate {
        name: name.into(),
        primary: p.into(),
        secondary: s.into(),
        ecs_local: false,
    }
}

/// Discover the ISP/DHCP resolver (ECS-local) from `ipconfig getpacket en0`.
fn discover_isp_candidate() -> Vec<DnsCandidate> {
    let known = [
        "1.1.1.1",
        "1.0.0.1",
        "9.9.9.9",
        "149.112.112.112",
        "8.8.8.8",
        "8.8.4.4",
        "208.67.222.222",
        "208.67.220.220",
    ];
    let out = cmd::run(
        &["ipconfig", "getpacket", "en0"],
        Duration::from_millis(1500),
    );
    let text = out.stdout;

    let router_ip = extract_brace(&text, "router")
        .and_then(|s| s.split(',').next().map(|t| t.trim().to_string()));

    let mut isp_ips: Vec<String> = Vec::new();
    if let Some(dns_block) = extract_brace(&text, "domain_name_server") {
        for tok in dns_block.split(',') {
            let ip = tok.trim().to_string();
            if !ip.is_empty() && !known.contains(&ip.as_str()) && !isp_ips.contains(&ip) {
                isp_ips.push(ip);
            }
        }
    }
    if isp_ips.is_empty() {
        if let Some(r) = router_ip {
            if !r.is_empty() {
                isp_ips.push(r);
            }
        }
    }
    if isp_ips.is_empty() {
        return Vec::new();
    }
    let primary = isp_ips[0].clone();
    let secondary = isp_ips.get(1).cloned().unwrap_or_else(|| "8.8.8.8".into());
    vec![DnsCandidate {
        name: "Local DHCP/ISP (ECS)".into(),
        primary,
        secondary,
        ecs_local: true,
    }]
}

/// Extract the `{...}` payload following a `key (...)` line in ipconfig output.
fn extract_brace(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with(key) {
            if let (Some(open), Some(close)) = (t.find('{'), t.rfind('}')) {
                if close > open {
                    return Some(t[open + 1..close].to_string());
                }
            }
        }
    }
    None
}

pub fn get_candidates() -> Vec<DnsCandidate> {
    let mut v = discover_isp_candidate();
    v.extend(static_anycast());
    v
}

fn benchmark(c: &DnsCandidate) -> DnsResult {
    let server = format!("@{}", c.primary);
    let mut latencies = Vec::new();
    for dom in TEST_DOMAINS {
        let t0 = Instant::now();
        let out = cmd::run(
            &["dig", &server, "+time=1", "+tries=1", dom],
            Duration::from_millis(1400),
        );
        if let Some(ms) = parse_query_time(&out.stdout) {
            latencies.push(ms);
        } else if out.success {
            latencies.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
    if latencies.is_empty() {
        DnsResult {
            name: c.name.clone(),
            primary: c.primary.clone(),
            secondary: c.secondary.clone(),
            avg_ms: 999.0,
            min_ms: 999.0,
            reachable: false,
        }
    } else {
        let avg = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let min = latencies.iter().cloned().fold(f64::MAX, f64::min);
        DnsResult {
            name: c.name.clone(),
            primary: c.primary.clone(),
            secondary: c.secondary.clone(),
            avg_ms: (avg * 10.0).round() / 10.0,
            min_ms: (min * 10.0).round() / 10.0,
            reachable: true,
        }
    }
}

fn parse_query_time(s: &str) -> Option<f64> {
    // "Query time: 12 msec"
    for line in s.lines() {
        if let Some(idx) = line.find("Query time:") {
            let tail = &line[idx + "Query time:".len()..];
            let num: String = tail
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = num.parse::<f64>() {
                return Some(n);
            }
        }
    }
    None
}

pub struct DnsEngine {
    pub service_name: String,
    pub last_results: Vec<DnsResult>,
}

impl DnsEngine {
    pub fn new(service_name: &str) -> Self {
        DnsEngine {
            service_name: service_name.to_string(),
            last_results: Vec::new(),
        }
    }

    pub fn race_resolvers(&mut self) -> Vec<DnsResult> {
        let mut results: Vec<DnsResult> = get_candidates().iter().map(benchmark).collect();
        results.sort_by(|a, b| {
            a.avg_ms
                .partial_cmp(&b.avg_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.last_results = results.clone();
        results
    }

    pub fn is_cellular_hotspot_active(&self) -> bool {
        let out = cmd::run(
            &["ipconfig", "getifaddr", "en0"],
            Duration::from_millis(1000),
        );
        let ip = out.stdout.trim();
        ip.starts_with("172.20.10.") || ip.starts_with("192.168.43.")
    }

    pub fn apply_dns(&self, servers: &[String], force: bool) -> String {
        if self.is_cellular_hotspot_active() && !force {
            return self.reset_to_dhcp_dns();
        }
        let mut argv = vec!["networksetup", "-setdnsservers", &self.service_name];
        for s in servers {
            argv.push(s);
        }
        cmd::run(&argv, Duration::from_millis(3000));
        Self::flush_cache();
        format!("Bound DNS to {} & flushed cache.", servers.join(", "))
    }

    pub fn reset_to_dhcp_dns(&self) -> String {
        cmd::run(
            &[
                "networksetup",
                "-setdnsservers",
                &self.service_name,
                "Empty",
            ],
            Duration::from_millis(3000),
        );
        Self::flush_cache();
        "Reset DNS to DHCP router defaults.".into()
    }

    pub fn race_and_apply_fastest(&mut self) -> String {
        let ranked = self.race_resolvers();
        let reachable: Vec<&DnsResult> = ranked.iter().filter(|r| r.reachable).collect();
        if reachable.is_empty() {
            return "DNS race could not reach resolvers; keeping current DNS.".into();
        }
        let isp: Vec<&&DnsResult> = reachable
            .iter()
            .filter(|r| r.name.contains("ISP"))
            .collect();
        let (winner, servers): (&DnsResult, Vec<String>) =
            if let Some(w) = isp.first().filter(|r| r.avg_ms < 180.0) {
                let mut s = vec![w.primary.clone()];
                if !s.contains(&w.secondary) {
                    s.push(w.secondary.clone());
                }
                for extra in ["8.8.8.8", "1.1.1.1"] {
                    if !s.iter().any(|x| x == extra) {
                        s.push(extra.into());
                    }
                }
                (**w, s)
            } else {
                let w = reachable[0];
                let mut s = vec![w.primary.clone(), "8.8.8.8".into(), "1.1.1.1".into()];
                s.dedup();
                (w, s)
            };
        self.apply_dns(&servers, false);
        let tail = servers
            .iter()
            .skip(1)
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "DNS Winner: {} ({} @ {}ms) + ECS Hybrid ({}).",
            winner.name, winner.primary, winner.avg_ms, tail
        )
    }

    pub fn flush_cache() -> String {
        cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(2000));
        cmd::run(
            &["killall", "-HUP", "mDNSResponder"],
            Duration::from_millis(2000),
        );
        "Flushed mDNSResponder & system DNS cache.".into()
    }
}
