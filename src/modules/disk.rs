use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport, thresholds: &crate::config::ThresholdConfig) {
    let mut data = serde_json::Map::new();

    let mut disks: Vec<Value> = Vec::new();
    let mut critical_disks = Vec::new();
    let mut warning_disks = Vec::new();

    let mut df = run(
        "df",
        &[
            "-h",
            "-P",
            "--type=ext4",
            "--type=ext3",
            "--type=ext2",
            "--type=btrfs",
            "--type=xfs",
            "--type=zfs",
            "--type=f2fs",
            "--type=ntfs",
            "--type=vfat",
            "--type=tmpfs",
            "--type=overlay",
            "--type=squashfs",
            "--total",
        ],
    );
    if df.is_none() {
        df = run("df", &["-hP"]);
    }

    if let Some(df_out) = df {
        for line in df_out.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }
            let dev = parts[0];
            let size = parts[1];
            let used = parts[2];
            let avail = parts[3];
            let pct_str = parts[4].trim_end_matches('%');
            let mount = parts[5];
            let pct: f64 = pct_str.parse().unwrap_or(0.0);
            let mut entry = serde_json::Map::new();
            entry.insert("device".into(), json!(dev));
            entry.insert("mount".into(), json!(mount));
            entry.insert("size".into(), json!(size));
            entry.insert("used".into(), json!(used));
            entry.insert("available".into(), json!(avail));
            entry.insert("usage_pct".into(), json!(pct));

            if dev != "total" {
                if pct > thresholds.disk_critical {
                    critical_disks.push(format!("{} ({} on {})", dev, pct_str, mount));
                } else if pct > thresholds.disk_warning {
                    warning_disks.push(format!("{} ({}% on {})", dev, pct_str, mount));
                }

                let ro = dev_is_readonly(mount);
                entry.insert("read_only".into(), json!(ro));
                let is_real_fs = !dev.starts_with("loop")
                    && !dev.starts_with("squash")
                    && !dev.starts_with("tmpfs")
                    && !dev.starts_with("devtmpfs")
                    && !dev.starts_with("overlay")
                    && dev != "none";
                let is_pseudo_mount = mount.starts_with("/run/")
                    || mount.starts_with("/sys/")
                    || mount.starts_with("/proc")
                    || mount == "/dev"
                    || mount.starts_with("/dev/")
                    || mount == "/run";
                if ro && is_real_fs && !is_pseudo_mount {
                    rep.critical(format!(
                        "{} mounted read-only (may indicate fs error)",
                        mount
                    ));
                }
            }
            disks.push(Value::Object(entry));
        }
    }

    let io_stats = read_diskstats();
    if !io_stats.is_empty() {
        data.insert("io_stats".into(), json!(io_stats));
    }

    let smart = check_smart();
    if !smart.is_empty() {
        data.insert("smart".into(), json!(smart));
    }

    let dirty_fs = check_dirty();
    if !dirty_fs.is_empty() {
        data.insert("dirty_filesystems".into(), json!(dirty_fs));
    }

    if !critical_disks.is_empty() {
        rep.critical(format!(
            "{} disk(s) critically full: {}",
            critical_disks.len(),
            critical_disks.join("; ")
        ));
    }
    if !warning_disks.is_empty() {
        rep.warning(format!(
            "{} disk(s) near capacity: {}",
            warning_disks.len(),
            warning_disks.join("; ")
        ));
    }
    if disks.is_empty() {
        rep.info("No disk mounts detected".to_string());
    }

    for s in &smart {
        if let Some(overall) = s.get("overall_status").and_then(|v| v.as_str()) {
            if overall.to_lowercase().contains("fail") {
                if let Some(dev) = s.get("device").and_then(|v| v.as_str()) {
                    rep.critical(format!("SMART failing: {}", dev));
                }
            }
        }
    }

    data.insert("disks".into(), json!(disks));
    rep.data = Value::Object(data);
    rep.compute_score();
}

fn dev_is_readonly(mount: &str) -> bool {
    read_file("/proc/mounts")
        .and_then(|s| {
            s.lines()
                .find(|l| l.split_whitespace().nth(1) == Some(mount))
                .map(|l| l.split_whitespace().nth(3).unwrap_or("").to_string())
        })
        .map(|opts| opts.split(',').any(|o| o == "ro"))
        .unwrap_or(false)
}

fn read_diskstats() -> Vec<Value> {
    let mut out = Vec::new();
    let lines = read_lines("/proc/diskstats");
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let dev = parts[2];
        if !dev.starts_with("sd")
            && !dev.starts_with("nvme")
            && !dev.starts_with("hd")
            && !dev.starts_with("vd")
            && !dev.starts_with("mmcblk")
        {
            continue;
        }
        let reads = parts[3].parse::<u64>().unwrap_or(0);
        let read_sectors = parts
            .get(5)
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);
        let writes = parts
            .get(7)
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);
        let write_sectors = parts
            .get(9)
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);
        let time_in_queue = parts
            .get(13)
            .and_then(|x| x.parse::<u64>().ok())
            .unwrap_or(0);
        out.push(json!({
            "device": dev,
            "reads_completed": reads,
            "read_sectors": read_sectors,
            "writes_completed": writes,
            "write_sectors": write_sectors,
            "io_queue_ms": time_in_queue,
        }));
    }
    out
}

fn check_smart() -> Vec<Value> {
    let mut out = Vec::new();
    if !which("smartctl") {
        return out;
    }
    let devices = run_lines("lsblk", &["-ndo", "name"])
        .into_iter()
        .filter(|n| {
            n.starts_with("sd")
                || n.starts_with("nvme")
                || n.starts_with("hd")
                || n.starts_with("vd")
        });
    for dev in devices.take(8) {
        let path = format!("/dev/{}", dev);
        let info = run("smartctl", &["-H", "-A", &path]).unwrap_or_default();
        let overall = info
            .lines()
            .find(|l| l.contains("overall health") || l.contains("SMART overall-health"))
            .map(|l| {
                l.split(':')
                    .nth(1)
                    .map(|x| x.trim().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut entry = serde_json::Map::new();
        entry.insert("device".into(), json!(path));
        entry.insert(
            "overall_status".into(),
            json!(if overall.is_empty() {
                "unsupported"
            } else {
                &overall
            }),
        );
        out.push(Value::Object(entry));
    }
    out
}

fn check_dirty() -> Vec<Value> {
    let mut out = Vec::new();
    if let Ok(s) = std::fs::read_to_string("/proc/mounts") {
        for line in s.lines() {
            let mut parts = line.split_whitespace();
            let dev = parts.next().unwrap_or("");
            let mount = parts.next().unwrap_or("");
            let _ = parts.next();
            let opts = parts.next().unwrap_or("");
            if mount == "/" || mount.starts_with("/boot") {
                let needs = match std::fs::read_to_string(format!(
                    "/proc/fs/jbd2/{}/health",
                    dev.split('/').next_back().unwrap_or("")
                )) {
                    Ok(_h) => false,
                    Err(_) => false,
                };
                if !needs && opts.contains("errors=remount-ro") {
                    let _ = mount;
                }
            }
        }
    }
    out.push(json!({"note": "filesystem dirty-bit check requires fsck; skipped"}));
    out
}
