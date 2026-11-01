//! UPnP gateway admin & reboot. Ported from gateway_rebooter.py.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use super::cmd;

const DESCRIPTOR_PORTS: &[(u16, &str)] = &[
    (1900, "/rootDesc.xml"),
    (49152, "/wps_device.xml"),
    (5000, "/rootDesc.xml"),
    (52869, "/gatedesc.xml"),
    (80, "/rootDesc.xml"),
    (8080, "/rootDesc.xml"),
];

pub struct GatewayRebooter {
    pub gateway_ip: String,
    control: Option<(String, String)>, // (control_url, service_type)
}

impl GatewayRebooter {
    pub fn new(gateway_ip: &str) -> Self {
        GatewayRebooter {
            gateway_ip: gateway_ip.to_string(),
            control: None,
        }
    }

    pub fn resolve_gateway_ip(&mut self) -> String {
        if !self.gateway_ip.is_empty() && self.gateway_ip != "N/A" {
            return self.gateway_ip.clone();
        }
        let out = cmd::run(
            &["route", "-n", "get", "default"],
            Duration::from_millis(1500),
        );
        let gw = out
            .stdout
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("gateway:")
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_else(|| "192.168.100.1".into());
        self.gateway_ip = gw.clone();
        gw
    }

    fn http_get(host: &str, port: u16, path: &str, timeout: Duration) -> Option<String> {
        let addr = format!("{host}:{port}");
        let mut stream = TcpStream::connect(&addr).ok()?;
        stream.set_read_timeout(Some(timeout)).ok()?;
        stream.set_write_timeout(Some(timeout)).ok()?;
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: UPnP/1.1 wifi-tui/5.0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).ok()?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).ok()?;
        Some(buf)
    }

    pub fn discover_control_url(&mut self, timeout: Duration) -> Option<(String, String)> {
        if let Some(c) = &self.control {
            return Some(c.clone());
        }
        let gw = self.resolve_gateway_ip();
        for (port, path) in DESCRIPTOR_PORTS {
            if let Some(body) = Self::http_get(&gw, *port, path, timeout) {
                for svc in ["WANIPConnection", "WANPPPConnection"] {
                    if body.contains(svc) {
                        if let Some(ctrl) = extract_control_url(&body, svc) {
                            let base = format!("http://{gw}:{port}");
                            let control_url = if ctrl.starts_with("http") {
                                ctrl
                            } else {
                                format!(
                                    "{base}{}",
                                    if ctrl.starts_with('/') {
                                        ctrl
                                    } else {
                                        format!("/{ctrl}")
                                    }
                                )
                            };
                            let service_type = format!("urn:schemas-upnp-org:service:{svc}:1");
                            self.control = Some((control_url.clone(), service_type.clone()));
                            return Some((control_url, service_type));
                        }
                    }
                }
            }
        }
        None
    }

    pub fn send_soap(&mut self, action: &str) -> (bool, String) {
        let (control_url, service_type) =
            match self.discover_control_url(Duration::from_millis(1500)) {
                Some(c) => c,
                None => {
                    return (
                        false,
                        "UPnP IGD service not discovered or disabled on router.".into(),
                    )
                }
            };
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\r\n  <s:Body>\r\n    <u:{action} xmlns:u=\"{service_type}\">\r\n      \r\n    </u:{action}>\r\n  </s:Body>\r\n</s:Envelope>"
        );
        match post_soap(&control_url, &service_type, action, &body) {
            Ok(resp) => (true, resp),
            Err(e) => (false, format!("Connection to {control_url} failed: {e}")),
        }
    }

    pub fn get_wan_status(&mut self) -> String {
        let gw = self.resolve_gateway_ip();
        if self
            .discover_control_url(Duration::from_millis(1500))
            .is_none()
        {
            return format!(
                "Gateway {gw}: UPnP IGD not discovered (Web GUI likely on port 80/443)"
            );
        }
        let (_, status_xml) = self.send_soap("GetStatusInfo");
        let (_, ext_xml) = self.send_soap("GetExternalIPAddress");
        let conn = tag(&status_xml, "NewConnectionStatus").unwrap_or_else(|| "Connected".into());
        let uptime = tag(&status_xml, "NewUptime")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let ext_ip = tag(&ext_xml, "NewExternalIPAddress").unwrap_or_else(|| "N/A".into());
        format!(
            "Gateway {gw} [{conn}]: Ext IP {ext_ip} │ Uptime {:.1}h",
            uptime / 3600.0
        )
    }

    pub fn force_wan_reconnect(&mut self) -> (bool, String) {
        let gw = self.resolve_gateway_ip();
        let (ok, res) = self.send_soap("ForceTermination");
        if ok {
            (true, format!("[OK] Gateway {gw}: Flushed NAT state & refreshed WAN connection via UPnP ForceTermination."))
        } else {
            (
                false,
                format!("[!] Gateway {gw}: ForceTermination failed: {res}"),
            )
        }
    }

    pub fn reboot_gateway(&mut self, confirm: bool) -> (bool, String) {
        if !confirm {
            return (
                false,
                "Reboot command aborted: Requires explicit confirm=True.".into(),
            );
        }
        let gw = self.resolve_gateway_ip();
        let (ok, _) = self.send_soap("Reboot");
        if ok {
            return (
                true,
                format!("[OK] Gateway {gw}: Warm reboot successfully initiated via UPnP."),
            );
        }
        let (ok2, _) = self.force_wan_reconnect();
        if ok2 {
            (
                true,
                format!("[OK] Gateway {gw}: WAN session recycled via ForceTermination."),
            )
        } else {
            (false, format!("[!] Gateway {gw}: Remote reboot unsupported via UPnP. Please visit http://{gw} directly."))
        }
    }
}

fn post_soap(
    control_url: &str,
    service_type: &str,
    action: &str,
    body: &str,
) -> Result<String, String> {
    // Parse host:port and path from control_url.
    let rest = control_url.strip_prefix("http://").ok_or("bad url")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rfind(':') {
        Some(i) => (
            &hostport[..i],
            hostport[i + 1..].parse::<u16>().unwrap_or(80),
        ),
        None => (hostport, 80u16),
    };
    let mut stream = TcpStream::connect((host, port)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nSOAPAction: \"{service_type}#{action}\"\r\nContent-Length: {}\r\nConnection: Close\r\nUser-Agent: wifi-tui/5.0 UPnP-Client\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn extract_control_url(xml: &str, svc: &str) -> Option<String> {
    // Find the service block then its <controlURL>.
    let marker = format!("urn:schemas-upnp-org:service:{svc}:1");
    let start = xml.find(&marker)?;
    let tail = &xml[start..];
    tag(tail, "controlURL")
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let s = xml.find(&open)? + open.len();
    let e = xml[s..].find(&close)? + s;
    Some(xml[s..e].trim().to_string())
}
