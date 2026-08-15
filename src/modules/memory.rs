use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

pub fn inspect(rep: &mut ModuleReport, thresholds: &crate::config::ThresholdConfig) {
    let mut system = sysinfo::System::new_all();
    system.refresh_memory();

    let mut data = serde_json::Map::new();

    let total = system.total_memory();
    let used = system.used_memory();
    let available = system.available_memory();
    let meminfo_raw = parse_meminfo();
    let cached = *meminfo_raw.get("Cached").unwrap_or(&0);
    let buffers = *meminfo_raw.get("Buffers").unwrap_or(&0);
    let swap_total = system.total_swap();
    let swap_used = system.used_swap();

    let pct = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };
    let swap_pct = if swap_total > 0 { (swap_used as f64 / swap_total as f64) * 100.0 } else { 0.0 };

    data.insert("total".into(), json!(total));
    data.insert("used".into(), json!(used));
    data.insert("available".into(), json!(available));
    data.insert("cached".into(), json!(cached));
    data.insert("buffers".into(), json!(buffers));
    data.insert("usage_pct".into(), json!((pct * 10.0).round() / 10.0));
    data.insert("swap_total".into(), json!(swap_total));
    data.insert("swap_used".into(), json!(swap_used));
    data.insert("swap_pct".into(), json!((swap_pct * 10.0).round() / 10.0));
    data.insert("total_human".into(), json!(format_bytes(total as f64)));
    data.insert("used_human".into(), json!(format_bytes(used as f64)));
    data.insert("available_human".into(), json!(format_bytes(available as f64)));

    let meminfo = &meminfo_raw;
    if let Some(hp) = meminfo.get("HugePages_Total") {
        data.insert("hugepages_total".into(), json!(hp));
    }
    if let Some(hp) = meminfo.get("HugePages_Free") {
        data.insert("hugepages_free".into(), json!(hp));
    }
    if let Some(sz) = meminfo.get("Hugepagesize") {
        data.insert("hugepage_size_kb".into(), json!(sz));
    }
    if let Some(commit) = meminfo.get("Committed_AS") {
        data.insert("committed_as".into(), json!(commit));
    }
    if let Some(commitlim) = meminfo.get("CommitLimit") {
        data.insert("commit_limit".into(), json!(commitlim));
    }

    let oom_kills = count_oom_kills();
    data.insert("oom_kills".into(), json!(oom_kills));

    let psi = read_psi();
    if let Some(p) = &psi.memory {
        data.insert("psi_memory".into(), json!(p));
    }
    if let Some(p) = &psi.cpu {
        data.insert("psi_cpu".into(), json!(p));
    }
    if let Some(p) = &psi.io {
        data.insert("psi_io".into(), json!(p));
    }

    let edac = read_edac();
    if !edac.is_empty() {
        data.insert("edac_errors".into(), json!(edac));
    }

    if pct > thresholds.mem_critical {
        rep.critical(format!("Memory usage {:.1}% exceeds critical threshold {:.0}%", pct, thresholds.mem_critical));
    } else if pct > thresholds.mem_warning {
        rep.warning(format!("Memory usage {:.1}% exceeds warning threshold {:.0}%", pct, thresholds.mem_warning));
    }

    if swap_total > 0 && swap_pct > 80.0 {
        rep.warning(format!("Swap usage high ({:.1}%)", swap_pct));
    }

    if oom_kills > 0 {
        rep.critical(format!("{} OOM kill(s) recorded in kernel logs", oom_kills));
    }

    if let Some(p) = &psi.memory {
        if let Some(avg10) = p.get("avg10").and_then(|v| v.as_f64()) {
            if avg10 > 30.0 {
                rep.warning(format!("Memory pressure high (avg10 = {:.1}% over last 10s)", avg10));
            }
        }
    }

    if !edac.is_empty() {
        let total_err: u64 = edac.iter().filter_map(|e| e.get("count").and_then(|v| v.as_u64())).sum();
        if total_err > 0 {
            rep.critical(format!("{} EDAC memory error(s) detected", total_err));
        }
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}

fn parse_meminfo() -> HashMap<String, u64> {
    let mut out = HashMap::new();
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            if let Some((k, rest)) = line.split_once(':') {
                let key = k.trim().to_string();
                let val: Option<u64> = rest.trim().split_whitespace().next().and_then(|x| x.parse().ok());
                if let Some(v) = val {
                    out.insert(key, v);
                }
            }
        }
    }
    out
}

fn count_oom_kills() -> u64 {
    if !which("dmesg") {
        return 0;
    }
    let lines = run_lines("dmesg", &["--color=never"]);
    lines.iter().filter(|l| {
        let lower = l.to_lowercase();
        lower.contains("out of memory") || lower.contains("oom-kill") || lower.contains("oom_reaper")
    }).count() as u64
}

struct Psi {
    memory: Option<Value>,
    cpu: Option<Value>,
    io: Option<Value>,
}

fn read_psi() -> Psi {
    Psi {
        memory: read_psi_file("/proc/pressure/memory"),
        cpu: read_psi_file("/proc/pressure/cpu"),
        io: read_psi_file("/proc/pressure/io"),
    }
}

fn read_psi_file(path: &str) -> Option<Value> {
    let content = read_file(path)?;
    let mut full = serde_json::Map::new();
    let mut some = serde_json::Map::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let target = if parts[0] == "full" { &mut full } else if parts[0] == "some" { &mut some } else { continue };
        for kv in &parts[1..] {
            if let Some((k, v)) = kv.split_once('=') {
                let val: f64 = v.parse().unwrap_or(0.0);
                target.insert(format!("avg{}", k.trim_start_matches("avg")), json!(val));
            }
        }
    }
    Some(json!({
        "some": some,
        "full": full,
    }))
}

fn read_edac() -> Vec<Value> {
    let mut out = Vec::new();
    let dirs: Vec<std::path::PathBuf> = match std::fs::read_dir("/sys/devices/system/edac/mc") {
        Ok(d) => d.flatten().map(|e| e.path()).collect(),
        Err(_) => Vec::new(),
    };
    if dirs.is_empty() { return out; }
    for mc_dir in dirs {
        let name = mc_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut counts = HashMap::new();
        for counter in &["ce_count", "ue_count"] {
            let path = mc_dir.join("ce_count").with_file_name(counter);
            if let Ok(count) = std::fs::read_to_string(&path) {
                let n: u64 = count.trim().parse().unwrap_or(0);
                counts.insert(counter.to_string(), n);
            }
        }
        let ce = counts.get("ce_count").copied().unwrap_or(0);
        let ue = counts.get("ue_count").copied().unwrap_or(0);
        out.push(json!({"controller": name, "correctable": ce, "uncorrectable": ue, "count": ce + ue}));
    }
    out
}