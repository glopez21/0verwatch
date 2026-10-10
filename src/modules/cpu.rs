use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport, thresholds: &crate::config::ThresholdConfig) {
    let mut system = sysinfo::System::new_all();
    system.refresh_cpu_all();
    std::thread::sleep(std::time::Duration::from_millis(500));
    system.refresh_cpu_all();

    let mut data = serde_json::Map::new();

    let model = read_first_line("/proc/cpuinfo")
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1).map(|x| x.trim().to_string()))
        })
        .unwrap_or_default();
    let vendor = read_first_line("/proc/cpuinfo")
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("vendor_id"))
                .and_then(|l| l.split(':').nth(1).map(|x| x.trim().to_string()))
        })
        .unwrap_or_default();

    let phys = system.physical_core_count().unwrap_or(0);
    let logical = system.cpus().len();

    data.insert("model".into(), json!(model));
    data.insert("vendor".into(), json!(vendor));
    data.insert("physical_cores".into(), json!(phys));
    data.insert("logical_cores".into(), json!(logical));

    let load = read_first_line("/proc/loadavg").unwrap_or_default();
    let parts: Vec<&str> = load.split_whitespace().collect();
    let l1 = parts
        .first()
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0);
    let l5 = parts
        .get(1)
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0);
    let l15 = parts
        .get(2)
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0);
    data.insert("load_1m".into(), json!(l1));
    data.insert("load_5m".into(), json!(l5));
    data.insert("load_15m".into(), json!(l15));

    let global_usage = system.global_cpu_usage() as f64;
    let cores: Vec<Value> = system
        .cpus()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let freq_mhz = read_first_line(&format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
                i
            ))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|v| v / 1000);
            let governor = read_first_line(&format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor",
                i
            ))
            .unwrap_or_default();
            json!({
                "core": i,
                "name": c.name(),
                "usage_pct": c.cpu_usage() as f64,
                "freq_mhz": freq_mhz,
                "governor": governor,
            })
        })
        .collect();

    let max_freq = read_first_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|v| v / 1000);
    let min_freq = read_first_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|v| v / 1000);
    let governor = read_first_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .unwrap_or_default();

    data.insert("global_usage_pct".into(), json!(global_usage));
    data.insert("cores".into(), json!(cores));
    data.insert("max_freq_mhz".into(), json!(max_freq));
    data.insert("min_freq_mhz".into(), json!(min_freq));
    data.insert("governor".into(), json!(governor));

    let mut ctx_switches = 0u64;
    if let Some(ctxt_line) = read_file("/proc/stat").and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("ctxt "))
            .map(|l| l.trim().to_string())
    }) {
        ctx_switches = ctxt_line
            .split_whitespace()
            .nth(1)
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
    }
    let procs_running = read_file("/proc/stat")
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("procs_running "))
                .map(|l| {
                    l.split_whitespace()
                        .nth(1)
                        .map(|x| x.to_string())
                        .unwrap_or_default()
                })
        })
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(0);
    data.insert("context_switches".into(), json!(ctx_switches));
    data.insert("procs_running".into(), json!(procs_running));

    let temp = read_temps();
    if !temp.is_empty() {
        data.insert("temperatures_c".into(), json!(temp));
    }

    if global_usage > thresholds.cpu_critical {
        rep.critical(format!(
            "CPU usage {:.1}% exceeds critical threshold {:.0}%",
            global_usage, thresholds.cpu_critical
        ));
    } else if global_usage > thresholds.cpu_warning {
        rep.warning(format!(
            "CPU usage {:.1}% exceeds warning threshold {:.0}%",
            global_usage, thresholds.cpu_warning
        ));
    }

    let critical_load = (logical as f64) * thresholds.load_avg_factor;
    if l1 > (logical as f64) {
        rep.critical(format!("Load avg {} exceeds total cores ({})", l1, logical));
    } else if l1 > critical_load {
        rep.warning(format!(
            "Load avg {} exceeds {}x logical cores ({})",
            l1, thresholds.load_avg_factor, logical
        ));
    }

    if let Some(max) = max_freq {
        if let Some(min) = min_freq {
            data.insert("turbo_enabled".into(), json!(max > min));
        }
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}

fn read_temps() -> Vec<Value> {
    let mut out = Vec::new();
    let hwmon = match std::fs::read_dir("/sys/class/hwmon") {
        Ok(d) => d,
        Err(_) => return out,
    };
    for entry in hwmon.flatten() {
        let name = read_first_line(&format!("{}/name", entry.path().display())).unwrap_or_default();
        if !name.contains("core")
            && !name.contains("cpu")
            && !name.contains("k10temp")
            && !name.contains("coretemp")
        {
            continue;
        }
        let temp_files: Vec<_> = std::fs::read_dir(entry.path())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("temp")
                    && e.file_name().to_string_lossy().ends_with("_input")
            })
            .collect();
        for tf in temp_files {
            let label_path = tf
                .path()
                .with_file_name(tf.file_name().to_string_lossy().replace("_input", "_label"));
            let label = read_first_line(&label_path.display().to_string()).unwrap_or_default();
            let temp = read_first_line(&tf.path().display().to_string())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
                / 1000.0;
            out.push(
                json!({"sensor": name, "label": label, "temp_c": (temp * 10.0).round() / 10.0}),
            );
        }
    }
    out
}
