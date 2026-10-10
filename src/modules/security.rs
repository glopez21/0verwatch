use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport) {
    let mut data = serde_json::Map::new();

    let selinux = read_first_line("/sys/fs/selinux/enforce").unwrap_or_default();
    let selinux_enabled = exists("/sys/fs/selinux");
    let apparmor_enabled = exists("/sys/kernel/security/apparmor");
    let smack = exists("/sys/smack");

    data.insert("selinux_enabled".into(), json!(selinux_enabled));
    data.insert("selinux_enforcing".into(), json!(selinux == "1"));
    data.insert("apparmor_enabled".into(), json!(apparmor_enabled));
    data.insert("smack_enabled".into(), json!(smack));

    if !selinux_enabled && !apparmor_enabled && !smack {
        rep.warning("No MAC system active (SELinux/AppArmor/Smack)".to_string());
    } else if selinux_enabled && selinux != "1" {
        rep.warning("SELinux is permissive/disabled".to_string());
    } else {
        rep.ok("MAC framework active".to_string());
    }

    let kptr = read_first_line("/proc/sys/kernel/kptr_restrict").unwrap_or_default();
    let dmesg = read_first_line("/proc/sys/kernel/dmesg_restrict").unwrap_or_default();
    let perf_paranoid = read_first_line("/proc/sys/kernel/perf_event_paranoid").unwrap_or_default();
    let yama = read_first_line("/proc/sys/kernel/yama/ptrace_scope").unwrap_or_default();
    let modprobe = read_first_line("/proc/sys/kernel/modules_disabled").unwrap_or_default();

    data.insert("kptr_restrict".into(), json!(kptr));
    data.insert("dmesg_restrict".into(), json!(dmesg));
    data.insert("perf_event_paranoid".into(), json!(perf_paranoid));
    data.insert("ptrace_scope".into(), json!(yama));
    data.insert("modules_disabled".into(), json!(modprobe));

    if kptr != "2" {
        rep.warning("kptr_restrict < 2".to_string());
    }
    if dmesg != "1" {
        rep.info("dmesg readable by users (dmesg_restrict=0)".to_string());
    }
    if yama != "2" && yama != "3" {
        rep.info("ptrace_scope < 2 allows process inspection".to_string());
    }
    if perf_paranoid != "2" && perf_paranoid != "3" {
        rep.info("perf_event_paranoid < 2".to_string());
    }

    let failed_logins = count_failed_logins();
    data.insert("failed_logins_journal".into(), json!(failed_logins));
    if failed_logins > 100 {
        rep.warning(format!(
            "{} failed login attempts since boot",
            failed_logins
        ));
    }

    let listening_services = listen_services();
    data.insert("listening_services".into(), json!(listening_services.len()));
    data.insert(
        "listening_local".into(),
        json!(listening_services
            .iter()
            .filter(|l| l
                .get("bind")
                .map(|b| b == "0.0.0.0" || b == "::" || b == "*")
                .unwrap_or(false))
            .count()),
    );
    data.insert(
        "listening_list".into(),
        json!(listening_services
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()),
    );

    let ssh_root = check_ssh_root();
    data.insert("ssh_root_login".into(), json!(ssh_root));
    if ssh_root == "yes" {
        rep.warning("SSH root login permitted".to_string());
    }

    let sudo_users = read_first_line("/etc/sudoers")
        .map(|_| run("getent", &["group", "sudo"]).unwrap_or_default())
        .unwrap_or_default();
    let _ = sudo_users;

    let uptodate =
        read_first_line("/var/lib/update-notifier/updates-available").unwrap_or_default();
    data.insert("pending_updates".into(), json!(uptodate));

    rep.data = Value::Object(data);
    rep.compute_score();
}

fn count_failed_logins() -> u64 {
    if which("journalctl") {
        run(
            "journalctl",
            &[
                "-q",
                "--facility=authpriv",
                "--grep=Failed",
                "--no-pager",
                "--output=cat",
            ],
        )
        .map(|s| s.lines().count() as u64)
        .unwrap_or(0)
    } else {
        0
    }
}

fn listen_services() -> Vec<Value> {
    let mut out = Vec::new();
    if !which("ss") {
        return out;
    }
    let lines = run_lines("ss", &["-tulnH"]);
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let proto = parts[0];
        let local = parts[4];
        let (bind, port) = match local.rsplit_once(':') {
            Some((b, p)) => (b, p),
            None => ("?", local),
        };
        out.push(json!({"proto": proto, "bind": bind, "port": port}));
    }
    out
}

fn check_ssh_root() -> String {
    if let Ok(s) = std::fs::read_to_string("/etc/ssh/sshd_config") {
        for line in s.lines() {
            let l = line.trim();
            if l.starts_with("PermitRootLogin") && !l.starts_with("PermitRootLogin#") {
                let val = l.split_whitespace().nth(1).unwrap_or("yes");
                return if val == "yes" {
                    "yes".into()
                } else {
                    "no".into()
                };
            }
        }
    }
    "unknown".into()
}
