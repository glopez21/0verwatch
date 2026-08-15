use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport) {
    let mut data = serde_json::Map::new();

    let mut ifaces: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "lo" { continue; }
            let mut entry = serde_json::Map::new();
            entry.insert("interface".into(), json!(name));
            let carrier = read_first_line(&format!("/sys/class/net/{}/carrier", name))
                .map(|s| s == "1").unwrap_or(false);
            entry.insert("link_up".into(), json!(carrier));
            let speed = read_first_line(&format!("/sys/class/net/{}/speed", name))
                .unwrap_or_default();
            entry.insert("speed_mbps".into(), json!(speed));
            let mtu = read_first_line(&format!("/sys/class/net/{}/mtu", name))
                .unwrap_or_default();
            entry.insert("mtu".into(), json!(mtu));
            let mac = read_first_line(&format!("/sys/class/net/{}/address", name))
                .unwrap_or_default();
            entry.insert("mac".into(), json!(mac));

            let (rx_bytes, tx_bytes, rx_errs, tx_errs, rx_drops, tx_drops) = read_dev_stat(&name);
            entry.insert("rx_bytes".into(), json!(rx_bytes));
            entry.insert("tx_bytes".into(), json!(tx_bytes));
            entry.insert("rx_errors".into(), json!(rx_errs));
            entry.insert("tx_errors".into(), json!(tx_errs));
            entry.insert("rx_drops".into(), json!(rx_drops));
            entry.insert("tx_drops".into(), json!(tx_drops));

            if rx_errs + tx_errs + rx_drops + tx_drops > 100 {
                rep.warning(format!("{}: high error/drop count ({} err, {} drop)", name, rx_errs + tx_errs, rx_drops + tx_drops));
            }
            ifaces.push(Value::Object(entry));
        }
    }
    data.insert("interfaces".into(), json!(ifaces));

    let tcp = parse_netstat("/proc/net/tcp");
    let tcp6 = parse_netstat("/proc/net/tcp6");
    data.insert("tcp_connections".into(), json!(tcp.total + tcp6.total));
    data.insert("tcp_states".into(), json!({
        "ESTABLISHED": tcp.established + tcp6.established,
        "LISTEN": tcp.listen + tcp6.listen,
        "TIME_WAIT": tcp.time_wait + tcp6.time_wait,
        "CLOSE_WAIT": tcp.close_wait + tcp6.close_wait,
    }));

    let listening: Vec<Value> = tcp.listening.iter().chain(tcp6.listening.iter()).cloned().collect();
    data.insert("listening_ports".into(), json!(listening));

    let retrans = parse_tcp_retrans();
    data.insert("tcp_retransmits".into(), json!(retrans));
    if retrans > 1000 {
        rep.warning(format!("High TCP retransmits: {}", retrans));
    }

    let conntrack = read_first_line("/proc/sys/net/netfilter/nf_conntrack_count");
    let conntrack_max = read_first_line("/proc/sys/net/netfilter/nf_conntrack_max");
    if let (Some(c), Some(cm)) = (conntrack, conntrack_max) {
        let count: u64 = c.parse().unwrap_or(0);
        let maxv: u64 = cm.parse().unwrap_or(0);
        data.insert("conntrack_count".into(), json!(count));
        data.insert("conntrack_max".into(), json!(maxv));
        if maxv > 0 {
            let pct = (count as f64 / maxv as f64) * 100.0;
            if pct > 80.0 {
                rep.warning(format!("Conntrack table {:.0}% full", pct));
            }
        }
    }

    let dns = read_first_line("/etc/resolv.conf").as_deref().map(|s| {
        s.lines().filter(|l| l.starts_with("nameserver")).filter_map(|l| l.split_whitespace().nth(1).map(|x| x.to_string())).collect::<Vec<_>>()
    }).unwrap_or_default();
    data.insert("dns_resolvers".into(), json!(dns));

    rep.data = Value::Object(data);
    rep.compute_score();
}

struct TcpSummary {
    total: usize,
    established: usize,
    listen: usize,
    time_wait: usize,
    close_wait: usize,
    listening: Vec<Value>,
}

fn parse_netstat(path: &str) -> TcpSummary {
    let mut s = TcpSummary { total: 0, established: 0, listen: 0, time_wait: 0, close_wait: 0, listening: Vec::new() };
    if let Ok(content) = std::fs::read_to_string(path) {
        for (i, line) in content.lines().enumerate() {
            if i < 1 { continue; }
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 4 { continue; }
            s.total += 1;
            let local = cols[1];
            let state = cols[3];
            let port = u16::from_str_radix(&local.split(':').last().unwrap_or("0"), 16).unwrap_or(0);
            match state {
                "01" => s.established += 1,
                "0A" => { s.listen += 1; s.listening.push(json!({"port": port, "proto": if path.contains("tcp6") { "tcp6" } else { "tcp" }})); },
                "06" => s.time_wait += 1,
                "08" => s.close_wait += 1,
                _ => {}
            }
        }
    }
    s
}

fn read_dev_stat(name: &str) -> (u64, u64, u64, u64, u64, u64) {
    let content = read_file(&format!("/proc/net/dev")).unwrap_or_default();
    for line in content.lines() {
        if let Some(rest) = line.split(':').nth(1) {
            let colon_idx = line.find(':').unwrap_or(0);
            let iface: &str = line[..colon_idx].trim();
            if iface == name {
                let p: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if p.len() >= 16 {
                    return (p[0], p[8], p[2], p[10], p[3], p[11]);
                }
            }
        }
    }
    (0, 0, 0, 0, 0, 0)
}

fn parse_tcp_retrans() -> u64 {
    let content = read_file("/proc/net/snmp").unwrap_or_default();
    let mut in_tcp = false;
    for line in content.lines() {
        if line.starts_with("Tcp:") {
            if !in_tcp { in_tcp = true; continue; }
            let parts: Vec<&str> = line.split(':').nth(1).unwrap_or("").split_whitespace().collect();
            if parts.len() > 12 {
                return parts[12].parse().unwrap_or(0);
            }
        }
    }
    0
}