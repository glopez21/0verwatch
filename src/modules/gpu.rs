use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

/// (utilization %, temperature °C, power W, throttling)
type GpuStats = (Option<f64>, Option<f64>, Option<f64>, bool);

pub fn inspect(rep: &mut ModuleReport) {
    let mut data = serde_json::Map::new();

    let pci_gpus = detect_pci_gpus();
    data.insert("detected".into(), json!(!pci_gpus.is_empty()));
    data.insert("pci_gpus".into(), json!(pci_gpus.clone()));

    let nvidia = detect_nvidia();
    let amd = detect_amd();
    let intel = detect_intel();

    if !nvidia.is_empty() {
        data.insert("nvidia".into(), json!(nvidia));
    }
    if !amd.is_empty() {
        data.insert("amd".into(), json!(amd));
    }
    if !intel.is_empty() {
        data.insert("intel".into(), json!(intel));
    }

    let driver: Vec<String> = pci_gpus
        .iter()
        .filter_map(|g| {
            g.get("driver")
                .and_then(|v| v.as_str())
                .map(|d| d.to_string())
        })
        .collect();
    for g in &pci_gpus {
        if let Ok((util, temp, power, throttling)) =
            read_drm_sysf(g.get("device_path").and_then(|v| v.as_str()).unwrap_or(""))
        {
            data.insert(
                "drm_sysfs".into(),
                json!({
                    "gpu": g.get("model").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    "utilization_pct": util,
                    "temperature_c": temp,
                    "power_watts": power,
                    "throttling": throttling,
                }),
            );
        }
    }
    let _ = driver;

    if !nvidia.is_empty() {
        for n in &nvidia {
            if let Some(util) = n.get("utilization_pct").and_then(|v| v.as_f64()) {
                if util > 95.0 {
                    rep.warning(format!(
                        "NVIDIA GPU {} util {:.0}%",
                        n.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        util
                    ));
                }
            }
            if let Some(temp) = n.get("temperature_c").and_then(|v| v.as_f64()) {
                if temp > 90.0 {
                    rep.critical(format!(
                        "NVIDIA GPU {} temp {:.0}C",
                        n.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        temp
                    ));
                } else if temp > 80.0 {
                    rep.warning(format!(
                        "NVIDIA GPU {} temp {:.0}C",
                        n.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        temp
                    ));
                }
            }
            if let Some(pw) = n.get("power_watts").and_then(|v| v.as_f64()) {
                if let Some(limit) = n.get("power_limit_watts").and_then(|v| v.as_f64()) {
                    if limit > 0.0 && pw > limit * 0.95 {
                        rep.warning(format!(
                            "NVIDIA GPU near power limit ({:.0}/{:.0}W)",
                            pw, limit
                        ));
                    }
                }
            }
            if let Some(t) = n.get("throttling").and_then(|v| v.as_str()) {
                if t != "None" && !t.is_empty() {
                    rep.warning(format!("NVIDIA GPU throttling: {}", t));
                }
            }
        }
    } else if !amd.is_empty() {
        for a in &amd {
            if let Some(util) = a.get("utilization_pct").and_then(|v| v.as_f64()) {
                if util > 95.0 {
                    rep.warning(format!(
                        "AMD GPU {} util {:.0}%",
                        a.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        util
                    ));
                }
            }
        }
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}

fn detect_pci_gpus() -> Vec<Value> {
    let mut out = Vec::new();
    if !which("lspci") {
        return manual_drm_scan();
    }
    let devs = run("lspci", &["-nnD"]).unwrap_or_default();
    for line in devs.lines() {
        let lower = line.to_lowercase();
        if !(lower.contains("vga compatible")
            || lower.contains("3d controller")
            || lower.contains("display controller"))
        {
            continue;
        }
        let mut entry = serde_json::Map::new();
        let slot = line.split_whitespace().next().unwrap_or("").to_string();
        entry.insert("slot".into(), json!(slot));
        let vendor = if lower.contains("nvidia") {
            "NVIDIA"
        } else if lower.contains("advanced micro devices")
            || lower.contains(" amd]")
            || lower.contains("ati ")
            || lower.contains("ati]")
        {
            "AMD"
        } else if lower.contains("intel") {
            "Intel"
        } else {
            "Unknown"
        };
        entry.insert("vendor".into(), json!(vendor));
        let model = line
            .split_once("]:")
            .map(|(_, rest)| {
                let m = rest.trim();
                let end = m.find(" [").unwrap_or(m.len());
                m[..end].trim().to_string()
            })
            .unwrap_or_else(|| line.to_string());
        entry.insert("model".into(), json!(model));
        let driver = run("lspci", &["-k", "-s", &slot]).and_then(|d| extract_driver(&d));
        entry.insert("driver".into(), json!(driver));
        entry.insert(
            "device_path".into(),
            json!(format!("/sys/bus/pci/devices/{}", slot)),
        );
        out.push(Value::Object(entry));
    }
    if out.is_empty() {
        return manual_drm_scan();
    }
    out
}

fn manual_drm_scan() -> Vec<Value> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains("card") && name.contains("-") {
                continue;
            }
            if name.contains('-') {
                continue;
            }
            let device_path = format!("{}/device", e.path().display());
            let vendor = read_first_line(&format!("{}/vendor", device_path)).unwrap_or_default();
            let device = read_first_line(&format!("{}/device", device_path)).unwrap_or_default();
            let vname = match vendor.as_str() {
                "0x10de" => "NVIDIA",
                "0x1002" => "AMD",
                "0x8086" => "Intel",
                _ => "Unknown",
            };
            let mut entry = serde_json::Map::new();
            entry.insert("slot".into(), json!(name));
            entry.insert("vendor".into(), json!(vname));
            entry.insert("vendor_id".into(), json!(vendor));
            entry.insert("device_id".into(), json!(device));
            entry.insert("device_path".into(), json!(device_path));
            out.push(Value::Object(entry));
        }
    }
    out
}

fn extract_driver(info: &str) -> Option<String> {
    for line in info.lines() {
        if line.contains("Kernel driver in use:") {
            return line.split(':').nth(1).map(|s| s.trim().to_string());
        }
    }
    None
}

fn detect_nvidia() -> Vec<Value> {
    if !which("nvidia-smi") {
        return Vec::new();
    }
    let query = "name,utilization.gpu,utilization.memory,memory.total,memory.used,memory.free,temperature.gpu,power.draw,power.limit,clocks.current.graphics,clocks.max.graphics,pstate,clocks_throttle_reasons.active";
    let details = run(
        "nvidia-smi",
        &[
            &format!("--query-gpu=index,{}", query),
            "--format=csv,noheader,nounits",
        ],
    )
    .unwrap_or_default();
    if details.is_empty() {
        return Vec::new();
    }

    let uuids = run(
        "nvidia-smi",
        &["--query-gpu=index,uuid", "--format=csv,noheader,nounits"],
    )
    .unwrap_or_default();

    let mut out = Vec::new();
    for line in details.lines() {
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if cols.len() < 14 {
            continue;
        }
        let idx: usize = cols[0].parse().unwrap_or(0);
        let name = cols[1];
        let util_gpu: f64 = cols[2].parse().unwrap_or(0.0);
        let util_mem: f64 = cols[3].parse().unwrap_or(0.0);
        let mem_total: f64 = cols[4].parse().unwrap_or(0.0);
        let mem_used: f64 = cols[5].parse().unwrap_or(0.0);
        let mem_free: f64 = cols[6].parse().unwrap_or(0.0);
        let temp: f64 = cols[7].parse().unwrap_or(0.0);
        let power_draw: f64 = cols[8].parse().unwrap_or(0.0);
        let power_limit_raw = cols[9];
        let cur_clk: f64 = cols[10].parse().unwrap_or(0.0);
        let max_clk: f64 = cols[11].parse().unwrap_or(0.0);
        let pstate = cols[12];
        let throttle_raw = cols[13];
        let throttling = if throttle_raw == "0x0000000000000000" || throttle_raw == "0" {
            "None".to_string()
        } else {
            decode_nvidia_throttle(throttle_raw)
        };

        let uuid_line = uuids.lines().nth(idx).unwrap_or("");
        let uuid = uuid_line.split(',').nth(1).unwrap_or("").trim();
        let power_limit: f64 = power_limit_raw.parse().unwrap_or(0.0);

        out.push(json!({
            "index": idx,
            "uuid": uuid,
            "name": name,
            "utilization_pct": util_gpu,
            "mem_utilization_pct": util_mem,
            "memory_total_mib": mem_total,
            "memory_used_mib": mem_used,
            "memory_free_mib": mem_free,
            "memory_pct": if mem_total > 0.0 { (mem_used / mem_total) * 100.0 } else { 0.0 },
            "temperature_c": temp,
            "power_watts": power_draw,
            "power_limit_watts": power_limit,
            "graphics_clock_mhz": cur_clk,
            "max_graphics_clock_mhz": max_clk,
            "pstate": pstate,
            "throttling": throttling,
        }));
    }
    out
}

fn decode_nvidia_throttle(raw: &str) -> String {
    let val = u64::from_str_radix(raw.trim_start_matches("0x"), 16).unwrap_or(0);
    let mut reasons = Vec::new();
    let flags = [
        (1 << 1, "applications_clocks"),
        (1 << 2, "sw_power_cap"),
        (1 << 3, "hw_slowdown"),
        (1 << 4, "sync_boost"),
        (1 << 5, "sw_thermal_slowdown"),
        (1 << 6, "hw_thermal_slowdown"),
        (1 << 7, "hw_power_brake_slowdown"),
    ];
    for (bit, name) in flags {
        if val & bit == bit {
            reasons.push(name);
        }
    }
    if reasons.is_empty() {
        "None".into()
    } else {
        reasons.join(",")
    }
}

fn detect_amd() -> Vec<Value> {
    let mut out = Vec::new();
    if which("rocm-smi") {
        let out_lines = run(
            "rocm-smi",
            &[
                "--showuse",
                "--showmemuse",
                "--showtemp",
                "--showpower",
                "--showclocks",
                "--json",
            ],
        )
        .unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Map<String, Value>>(&out_lines) {
            for (card, v) in &parsed {
                if !card.starts_with("card") {
                    continue;
                }
                if let Value::Object(m) = v {
                    let util = m
                        .get("GPU use (%)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let mem = m
                        .get("GPU memory use (%)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let temp = m
                        .get("Temperature (Sensor edge) (C)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let power = m
                        .get("Average Graphics Package Power (W)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let clk = m
                        .get("GPU clock (MHz)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let mclk = m
                        .get("GPU memory clock (MHz)")
                        .and_then(|x| x.as_str())
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    out.push(json!({
                        "card": card,
                        "utilization_pct": util,
                        "mem_utilization_pct": mem,
                        "temperature_c": temp,
                        "power_watts": power,
                        "graphics_clock_mhz": clk,
                        "memory_clock_mhz": mclk,
                    }));
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains("-") {
                continue;
            }
            let dev = format!("{}/device", e.path().display());
            let vendor = read_first_line(&format!("{}/vendor", dev)).unwrap_or_default();
            if vendor != "0x1002" {
                continue;
            }
            let util = read_first_line(&format!("{}/gpu_busy_percent", dev))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let mem = read_first_line(&format!("{}/mem_busy_percent", dev))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let temp = read_first_line(&format!("{}/hwmon/hwmon0/temp1_input", dev))
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v / 1000.0)
                .unwrap_or(0.0);
            let power = read_first_line(&format!("{}/hwmon/hwmon0/power1_input", dev))
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v / 1_000_000.0)
                .unwrap_or(0.0);
            out.push(json!({
                "card": name,
                "utilization_pct": util,
                "mem_utilization_pct": mem,
                "temperature_c": temp,
                "power_watts": power,
            }));
        }
    }
    out
}

fn detect_intel() -> Vec<Value> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains("-") {
                continue;
            }
            let dev = format!("{}/device", e.path().display());
            let vendor = read_first_line(&format!("{}/vendor", dev)).unwrap_or_default();
            if vendor != "0x8086" {
                continue;
            }
            let mut entry = serde_json::Map::new();
            entry.insert("card".into(), json!(name));
            entry.insert("vendor".into(), json!("Intel"));
            if exists(&format!("{}/gt/gt0/rps_act_freq_mhz", dev)) {
                let act = read_first_line(&format!("{}/gt/gt0/rps_act_freq_mhz", dev))
                    .unwrap_or_default();
                entry.insert("graphics_clock_mhz".into(), json!(act));
            }
            if let Ok(hwmon) = std::fs::read_dir(format!("{}/hwmon", dev)) {
                for h in hwmon.flatten() {
                    let hp = format!("{}/hwmon/{}", dev, h.file_name().to_string_lossy());
                    if let Some(temp) = read_first_line(&format!("{}/temp1_input", hp))
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        entry.insert("temperature_c".into(), json!(temp / 1000.0));
                    }
                    if let Some(pow) = read_first_line(&format!("{}/power1_input", hp))
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        entry.insert("power_watts".into(), json!(pow / 1_000_000.0));
                    }
                }
            }
            if which("intel_gpu_top") {
                let samp = run_timeout(
                    "intel_gpu_top",
                    &["-l", "-s", "1", "-o", "-"],
                    std::time::Duration::from_secs(2),
                )
                .unwrap_or_default();
                for line in samp.lines().take(1) {
                    if line.contains("Freq") {
                        let mut entry2 = entry.clone();
                        entry2.insert("intel_gpu_top_sample".into(), json!(line));
                        let _ = &mut entry2;
                    }
                }
            }
            out.push(Value::Object(entry));
        }
    }
    out
}

fn read_drm_sysf(device_path: &str) -> anyhow::Result<GpuStats> {
    if device_path.is_empty() {
        anyhow::bail!("no path");
    }
    let util = read_first_line(&format!("{}/gpu_busy_percent", device_path))
        .and_then(|s| s.parse::<f64>().ok());
    let temp = read_first_line(&format!("{}/hwmon/hwmon0/temp1_input", device_path))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| v / 1000.0);
    let power = read_first_line(&format!("{}/hwmon/hwmon0/power1_input", device_path))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| v / 1_000_000.0);
    let throttling = false;
    Ok((util, temp, power, throttling))
}
