use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;

pub fn inspect(rep: &mut ModuleReport) {
    let mut data = serde_json::Map::new();

    let version = read_first_line("/proc/version").unwrap_or_default();
    let release = read_first_line("/proc/sys/kernel/osrelease").unwrap_or_default();
    let ostype = read_first_line("/proc/sys/kernel/ostype").unwrap_or_default();

    data.insert("release".into(), json!(release));
    data.insert("os_type".into(), json!(ostype));
    data.insert("version".into(), json!(version));

    let cmdline = read_first_line("/proc/cmdline").unwrap_or_default();
    data.insert("boot_cmdline".into(), json!(cmdline));

    let tainted = read_first_line("/proc/sys/kernel/tainted").unwrap_or_default();
    let taint_num = tainted.parse::<u64>().unwrap_or(0);
    let taint_flags = decode_taint(taint_num, &version);
    data.insert("tainted".into(), json!(taint_num > 0));
    data.insert("taint_flags".into(), json!(taint_flags));

    let kp_restrict = read_first_line("/proc/sys/kernel/kptr_restrict").unwrap_or_default();
    let dmesg_restrict = read_first_line("/proc/sys/kernel/dmesg_restrict").unwrap_or_default();
    let panic_on_oops = read_first_line("/proc/sys/kernel/panic_on_oops").unwrap_or_default();
    let panic_timeout = read_first_line("/proc/sys/kernel/panic").unwrap_or_default();
    let random_boot = read_first_line("/proc/sys/kernel/random/boot_id").unwrap_or_default();

    data.insert("kptr_restrict".into(), json!(kp_restrict));
    data.insert("dmesg_restrict".into(), json!(dmesg_restrict));
    data.insert("panic_on_oops".into(), json!(panic_on_oops));
    data.insert("panic_timeout".into(), json!(panic_timeout));
    data.insert("boot_id".into(), json!(random_boot));

    let hostname = read_first_line("/proc/sys/kernel/hostname").unwrap_or_default();
    data.insert("hostname".into(), json!(hostname));

    let uptime = uptime_secs();
    data.insert("uptime_seconds".into(), json!(uptime as u64));
    data.insert("uptime".into(), json!(format_duration(uptime)));

    let modules = read_lines("/proc/modules");
    let mod_count = modules.len();
    let mod_names: Vec<String> = modules.iter()
        .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
        .collect();
    data.insert("loaded_modules".into(), json!(mod_count));
    data.insert("module_names".into(), json!(mod_names));

    let dmesg_critical = scan_dmesg();
    data.insert("dmesg_errors".into(), json!(dmesg_critical.errors));
    data.insert("dmesg_warnings".into(), json!(dmesg_critical.warnings));
    data.insert("dmesg_oops".into(), json!(dmesg_critical.oops));
    data.insert("dmesg_panics".into(), json!(dmesg_critical.panics));
    data.insert("dmesg_samples".into(), json!(dmesg_critical.samples));

    if taint_num > 0 {
        rep.critical(format!("Kernel is tainted (flags: {})", taint_flags.join(", ")));
    } else {
        rep.ok("Kernel not tainted".to_string());
    }

    if dmesg_critical.panics > 0 {
        rep.critical(format!("{} kernel panic(s) in dmesg", dmesg_critical.panics));
    }
    if dmesg_critical.oops > 0 {
        rep.critical(format!("{} kernel oops(es) in dmesg", dmesg_critical.oops));
    }
    if dmesg_critical.errors > 10 {
        rep.warning(format!("{} error-level kernel messages", dmesg_critical.errors));
    } else if dmesg_critical.errors > 0 {
        rep.info(format!("{} error-level kernel messages", dmesg_critical.errors));
    }
    if dmesg_critical.warnings > 20 {
        rep.warning(format!("{} warning-level kernel messages", dmesg_critical.warnings));
    }

    if kp_restrict == "0" {
        rep.warning("kptr_restrict is 0; kernel pointers exposed to userspace".to_string());
    }
    if dmesg_restrict == "0" {
        rep.info("dmesg_restrict is 0; dmesg readable by non-root".to_string());
    }
    if panic_on_oops == "0" {
        rep.info("panic_on_oops disabled; oops won't halt on error".to_string());
    }
    if uptime < 300.0 {
        rep.warning("System rebooted recently (< 5 min uptime)".to_string());
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}

use serde_json::Value;

struct DmesgScan {
    errors: usize,
    warnings: usize,
    oops: usize,
    panics: usize,
    samples: Vec<String>,
}

fn scan_dmesg() -> DmesgScan {
    let mut scan = DmesgScan { errors: 0, warnings: 0, oops: 0, panics: 0, samples: Vec::new() };
    let lines = if which("dmesg") {
        run_lines("dmesg", &["--color=never"])
    } else {
        Vec::new()
    };
    for line in &lines {
        let lower = line.to_lowercase();
        if lower.contains("panic") || lower.contains(" BUG:") {
            scan.panics += 1;
            if scan.samples.len() < 8 {
                scan.samples.push(line.clone());
            }
        } else if lower.contains("oops") || lower.contains(" call trace:") || lower.contains("dump_stack") || lower.contains("segfault") {
            scan.oops += 1;
            if scan.samples.len() < 8 {
                scan.samples.push(line.clone());
            }
        } else if lower.contains(" error") || lower.contains("fail") || lower.contains("critical") {
            scan.errors += 1;
            if scan.samples.len() < 8 {
                scan.samples.push(line.clone());
            }
        } else if lower.contains("warn") {
            scan.warnings += 1;
        }
    }
    scan
}



fn decode_taint(n: u64, _ver: &str) -> Vec<String> {
    let flags = [
        ('P', "proprietary module"),
        ('O', "out-of-tree module"),
        ('E', "unsigned module"),
        ('S', "softlockup"),
        ('R', "force-loaded module"),
        ('M', "machine check error"),
        ('B', "bad page reference"),
        ('U', "userspace oops"),
        ('D', "kernel recently died"),
        ('A', "ACPI table overridden"),
        ('W', "taINT_warn"),
        ('C', "staging driver loaded"),
        ('I', "firmware workaround"),
        ('H', "hardware error"),
        ('T', "taint on module"),
        ('G', "options removed"),
        ('X', "externally-built module"),
        ('N', "unsigned module2"),
        ('L', "soft lockup"),
        ('K', "live patch"),
        ('Z', "zeroed page"),
    ];
    let mut out = Vec::new();
    for (i, (c, desc)) in flags.iter().enumerate() {
        if (n >> i) & 1 == 1 {
            out.push(format!("{}:{}", c, desc));
        }
    }
    out
}