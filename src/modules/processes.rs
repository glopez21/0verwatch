use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport) {
    let mut system = sysinfo::System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut data = serde_json::Map::new();

    let total = system.processes().len();
    data.insert("total".into(), json!(total));

    let mut by_state: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let zombies: Vec<Value> = Vec::new();
    let d_states: Vec<Value> = Vec::new();
    let mut zombies_v = zombies;
    let mut d_states_v = d_states;

    let mut top_cpu: Vec<(sysinfo::Pid, String, f64, u64)> = Vec::new();
    let mut top_mem: Vec<(sysinfo::Pid, String, u64)> = Vec::new();

    for (pid, proc_) in system.processes() {
        let state = match proc_.status() {
            sysinfo::ProcessStatus::Run => "Running",
            sysinfo::ProcessStatus::Sleep => "Sleep",
            sysinfo::ProcessStatus::Idle => "Idle",
            sysinfo::ProcessStatus::Zombie => "Zombie",
            sysinfo::ProcessStatus::Dead => "Dead",
            sysinfo::ProcessStatus::UninterruptibleDiskSleep => "D-state",
            sysinfo::ProcessStatus::Unknown(_) => "Unknown",
            _ => "Other",
        }.to_string();
        *by_state.entry(state).or_insert(0) += 1;

        if proc_.status() == sysinfo::ProcessStatus::Zombie {
            zombies_v.push(json!({"pid": pid.as_u32(), "name": proc_.name().to_string_lossy().to_string()}));
        }
        if proc_.status() == sysinfo::ProcessStatus::UninterruptibleDiskSleep {
            d_states_v.push(json!({"pid": pid.as_u32(), "name": proc_.name().to_string_lossy().to_string()}));
        }
        let cpu = proc_.cpu_usage();
        if cpu > 1.0 {
            top_cpu.push((*pid, proc_.name().to_string_lossy().to_string(), cpu.into(), proc_.memory()));
        }
        top_mem.push((*pid, proc_.name().to_string_lossy().to_string(), proc_.memory()));
    }

    data.insert("state_counts".into(), json!(by_state));
    data.insert("zombies".into(), json!(zombies_v.len()));
    data.insert("zombie_list".into(), json!(zombies_v.iter().take(10).cloned().collect::<Vec<_>>()));
    data.insert("d_states".into(), json!(d_states_v.len()));
    data.insert("d_state_list".into(), json!(d_states_v.iter().take(10).cloned().collect::<Vec<_>>()));

    top_cpu.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    top_cpu.truncate(10);
    let top: Vec<Value> = top_cpu.iter().map(|(p, n, c, m)| json!({
        "pid": p.as_u32(), "name": n, "cpu_pct": (c*10.0).round()/10.0, "mem_kb": m
    })).collect();
    data.insert("top_cpu".into(), json!(top));

    top_mem.sort_by(|a, b| b.2.cmp(&a.2));
    top_mem.truncate(10);
    let topm: Vec<Value> = top_mem.iter().map(|(p, n, m)| json!({
        "pid": p.as_u32(), "name": n, "mem_kb": m, "mem_human": format_bytes(*m as f64)
    })).collect();
    data.insert("top_mem".into(), json!(topm));

    if !zombies_v.is_empty() {
        rep.warning(format!("{} zombie process(es)", zombies_v.len()));
    }
    if d_states_v.len() > 10 {
        rep.warning(format!("{} processes stuck in D (uninterruptible) state", d_states_v.len()));
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}