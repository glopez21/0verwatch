use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport, thresholds: &crate::config::ThresholdConfig) {
    let mut data = serde_json::Map::new();

    let has_systemd = which("systemctl") && exists("/run/systemd/system");
    data.insert("init_system".into(), json!(if has_systemd { "systemd" } else { "init/sysv" }));

    if has_systemd {
        inspect_systemd(&mut data, rep, thresholds);
    } else {
        inspect_init(&mut data, rep);
    }

    let zombie_count = count_zombies();
    data.insert("zombie_processes".into(), json!(zombie_count));

    if zombie_count > thresholds.max_zombies {
        rep.warning(format!("{} zombie process(es) (threshold {})", zombie_count, thresholds.max_zombies));
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}

fn inspect_systemd(data: &mut serde_json::Map<String, Value>, rep: &mut ModuleReport, thresholds: &crate::config::ThresholdConfig) {
    let list = run("systemctl", &["list-units", "--type=service", "--no-legend", "--no-pager", "--plain"]).unwrap_or_default();
    let mut running = 0usize;
    let mut failed_list: Vec<Value> = Vec::new();
    let mut dead = 0usize;
    for line in list.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 { continue; }
        let unit = parts[0];
        let load = parts[1];
        let active = parts[2];
        let sub = parts.get(3).unwrap_or(&"");
        if load == "loaded" || load == "not-found" {
            if active == "active" && (*sub == "running" || *sub == "waiting") {
                running += 1;
            }
            if active == "failed" || *sub == "failed" {
                let desc = parts.get(4..).map(|d| d.join(" ")).unwrap_or_default();
                failed_list.push(json!({"unit": unit, "active": active, "sub": sub, "desc": desc}));
            }
            if active == "inactive" && (*sub == "dead") {
                dead += 1;
            }
        }
    }
    let failed_count = failed_list.len();
    data.insert("running_services".into(), json!(running));
    data.insert("failed_services".into(), json!(failed_count));
    data.insert("inactive_services".into(), json!(dead));
    data.insert("failed_list".into(), json!(failed_list));

    let enabled = run("systemctl", &["list-unit-files", "--type=service", "--state=enabled", "--no-legend", "--no-pager", "--plain"])
        .map(|s| s.lines().count()).unwrap_or(0);
    data.insert("enabled_services".into(), json!(enabled));

    let sockets = run("systemctl", &["list-units", "--type=socket", "--state=active", "--no-legend", "--no-pager", "--plain"])
        .map(|s| s.lines().count()).unwrap_or(0);
    data.insert("active_sockets".into(), json!(sockets));

    let timers = run("systemctl", &["list-units", "--type=timer", "--state=active", "--no-legend", "--no-pager", "--plain"])
        .map(|s| s.lines().count()).unwrap_or(0);
    data.insert("active_timers".into(), json!(timers));

    let state = run("systemctl", &["is-system-running"]).unwrap_or_default();
    data.insert("system_state".into(), json!(state.split_whitespace().next().unwrap_or("unknown")));

    if failed_count > thresholds.max_failed_services {
        rep.critical(format!("{} failed service(s)", failed_count));
        for f in data.get("failed_list").and_then(|v| v.as_array()).unwrap_or(&Vec::new()).iter().take(5) {
            if let Some(unit) = f.get("unit").and_then(|u| u.as_str()) {
                rep.critical(format!("  failed: {}", unit));
            }
        }
    } else if failed_count > 0 {
        rep.info(format!("{} failed service(s)", failed_count));
    } else {
        rep.ok(format!("{} service(s) running, 0 failed", running));
    }

    if state.contains("degraded") {
        rep.warning("systemd reports system state: degraded".to_string());
    }
}

fn inspect_init(data: &mut serde_json::Map<String, Value>, rep: &mut ModuleReport) {
    let services = read_lines("/etc/init.d");
    let mut list: Vec<Value> = Vec::new();
    for svc in &services {
        if svc.starts_with('.') || svc.contains("README") { continue; }
        let status = run(&format!("/etc/init.d/{}", svc), &["status"]).unwrap_or_default();
        let running = status.to_lowercase().contains("running") || status.to_lowercase().contains("active");
        list.push(json!({"service": svc, "running": running, "status": status}));
    }
    let running_count = list.iter().filter(|v| v.get("running").and_then(|x| x.as_bool()).unwrap_or(false)).count();
    data.insert("services".into(), json!(list));
    data.insert("running_services".into(), json!(running_count));
    rep.info(format!("{} init.d services, {} running", list.len(), running_count));
}

fn count_zombies() -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.chars().all(|c| c.is_ascii_digit()) { continue; }
            if let Some(state) = read_first_line(&format!("/proc/{}/stat", name)).and_then(|s| {
                let rest = s.split(')').nth(1)?;
                rest.trim().chars().next().map(|c| c.to_string())
            }) {
                if state == "Z" { count += 1; }
            }
        }
    }
    count
}