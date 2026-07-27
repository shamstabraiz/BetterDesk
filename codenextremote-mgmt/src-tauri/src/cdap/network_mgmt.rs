//! Network management — interfaces, DNS, routing, Wi-Fi.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
//  Network Interfaces
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub description: String,
    pub status: String,
    pub mac_address: String,
    pub ipv4: String,
    pub ipv6: String,
    pub speed_mbps: u64,
    pub interface_type: String,
}

pub fn get_network_info() -> Result<Vec<NetworkInterface>> {
    let output = run_powershell(
        "Get-NetAdapter | Select-Object Name, InterfaceDescription, Status, \
         MacAddress, LinkSpeed, MediaType | ConvertTo-Json -Depth 2",
    )?;

    let adapters: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();

    // Get IP addresses separately
    let ip_output = run_powershell(
        "Get-NetIPAddress | Where-Object { $_.InterfaceAlias -ne 'Loopback Pseudo-Interface 1' } | \
         Select-Object InterfaceAlias, IPAddress, AddressFamily | ConvertTo-Json -Depth 2",
    )?;

    let ip_addrs: Vec<serde_json::Value> = serde_json::from_str(&ip_output).unwrap_or_default();

    let interfaces = adapters
        .into_iter()
        .map(|a| {
            let name = a
                .get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // Find matching IP addresses
            let ipv4 = ip_addrs
                .iter()
                .find(|ip| {
                    ip.get("InterfaceAlias")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        == name
                        && ip
                            .get("AddressFamily")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            == 2
                })
                .and_then(|ip| ip.get("IPAddress").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();

            let ipv6 = ip_addrs
                .iter()
                .find(|ip| {
                    ip.get("InterfaceAlias")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        == name
                        && ip
                            .get("AddressFamily")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            == 23
                })
                .and_then(|ip| ip.get("IPAddress").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();

            // Parse link speed (e.g. "1 Gbps" -> 1000)
            let speed_str = a
                .get("LinkSpeed")
                .and_then(|v| v.as_str())
                .unwrap_or("0");
            let speed_mbps = parse_link_speed(speed_str);

            NetworkInterface {
                name,
                description: a
                    .get("InterfaceDescription")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                status: a
                    .get("Status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .into(),
                mac_address: a
                    .get("MacAddress")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                ipv4,
                ipv6,
                speed_mbps,
                interface_type: a
                    .get("MediaType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .into(),
            }
        })
        .collect();

    Ok(interfaces)
}

// ---------------------------------------------------------------------------
//  DNS Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct DnsConfig {
    pub interface: String,
    pub servers: Vec<String>,
}

pub fn get_dns_config() -> Result<Vec<DnsConfig>> {
    let output = run_powershell(
        "Get-DnsClientServerAddress -AddressFamily IPv4 | \
         Where-Object { $_.ServerAddresses.Count -gt 0 } | \
         Select-Object InterfaceAlias, @{N='Servers';E={$_.ServerAddresses -join ','}} | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let configs = raw
        .into_iter()
        .map(|v| {
            let servers_str = v
                .get("Servers")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            DnsConfig {
                interface: v
                    .get("InterfaceAlias")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                servers: servers_str.split(',').map(|s| s.trim().to_string()).collect(),
            }
        })
        .collect();

    Ok(configs)
}

// ---------------------------------------------------------------------------
//  Routing Table
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct RouteEntry {
    pub destination: String,
    pub prefix_length: u32,
    pub next_hop: String,
    pub interface_alias: String,
    pub metric: u32,
}

pub fn get_routing_table() -> Result<Vec<RouteEntry>> {
    let output = run_powershell(
        "Get-NetRoute -AddressFamily IPv4 | \
         Select-Object -First 100 DestinationPrefix, NextHop, InterfaceAlias, RouteMetric | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let routes = raw
        .into_iter()
        .map(|v| {
            let prefix = v
                .get("DestinationPrefix")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0.0/0");
            let (dest, prefix_len) = match prefix.split_once('/') {
                Some((d, p)) => (d.to_string(), p.parse::<u32>().unwrap_or(0)),
                None => (prefix.to_string(), 0),
            };

            RouteEntry {
                destination: dest,
                prefix_length: prefix_len,
                next_hop: v
                    .get("NextHop")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0.0")
                    .into(),
                interface_alias: v
                    .get("InterfaceAlias")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                metric: v
                    .get("RouteMetric")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            }
        })
        .collect();

    Ok(routes)
}

// ---------------------------------------------------------------------------
//  Network Commands
// ---------------------------------------------------------------------------

pub fn execute_network_command(
    command: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    match command {
        "interfaces" => {
            let info = get_network_info()?;
            Ok(serde_json::to_value(info)?)
        }
        "dns" => {
            let config = get_dns_config()?;
            Ok(serde_json::to_value(config)?)
        }
        "routes" => {
            let routes = get_routing_table()?;
            Ok(serde_json::to_value(routes)?)
        }
        "ping" => {
            let host = params
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("8.8.8.8");
            validate_hostname(host)?;
            let count = params
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(4)
                .min(20) as u32;

            let output = run_powershell(&format!(
                "Test-Connection -ComputerName '{}' -Count {} -ErrorAction SilentlyContinue | \
                 Select-Object Address, Latency, Status | ConvertTo-Json",
                host, count,
            ))?;

            let results: serde_json::Value =
                serde_json::from_str(&output).unwrap_or(serde_json::json!([]));
            Ok(results)
        }
        "traceroute" => {
            let host = params
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("8.8.8.8");
            validate_hostname(host)?;

            let output = run_powershell(&format!(
                "Test-NetConnection -ComputerName '{}' -TraceRoute -ErrorAction SilentlyContinue | \
                 Select-Object -ExpandProperty TraceRoute | ConvertTo-Json",
                host,
            ))?;

            let hops: serde_json::Value =
                serde_json::from_str(&output).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({ "hops": hops }))
        }
        "port_check" => {
            let host = params
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("localhost");
            let port = params
                .get("port")
                .and_then(|v| v.as_u64())
                .unwrap_or(80)
                .min(65535) as u16;

            validate_hostname(host)?;

            let output = run_powershell(&format!(
                "Test-NetConnection -ComputerName '{}' -Port {} -ErrorAction SilentlyContinue | \
                 Select-Object ComputerName, RemotePort, TcpTestSucceeded | ConvertTo-Json",
                host, port,
            ))?;

            let result: serde_json::Value =
                serde_json::from_str(&output).unwrap_or(serde_json::json!({}));
            Ok(result)
        }
        _ => bail!("Unknown network command: {}", command),
    }
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn run_powershell(script: &str) -> Result<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .context("Failed to run PowerShell")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("PowerShell failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse link speed like "1 Gbps", "100 Mbps" to Mbps.
fn parse_link_speed(s: &str) -> u64 {
    let s = s.trim();
    if let Some(num) = s.strip_suffix(" Gbps") {
        num.trim().parse::<f64>().unwrap_or(0.0) as u64 * 1000
    } else if let Some(num) = s.strip_suffix(" Mbps") {
        num.trim().parse::<f64>().unwrap_or(0.0) as u64
    } else if let Some(num) = s.strip_suffix(" Kbps") {
        (num.trim().parse::<f64>().unwrap_or(0.0) / 1000.0) as u64
    } else {
        0
    }
}

/// Validate hostname/IP to prevent command injection.
fn validate_hostname(host: &str) -> Result<()> {
    if host.is_empty() || host.len() > 253 {
        bail!("Invalid hostname length");
    }
    // Allow alphanumeric, dots, hyphens, colons (IPv6), square brackets
    for c in host.chars() {
        if !(c.is_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')) {
            bail!("Invalid character in hostname: '{}'", c);
        }
    }
    Ok(())
}
