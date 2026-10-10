use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub fn read_file(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

#[allow(dead_code)]
pub fn read_file_lossy(path: &str) -> Option<String> {
    fs::read(path)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_string())
}

pub fn read_lines(path: &str) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(s) => s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn read_first_line(path: &str) -> Option<String> {
    read_file(path).and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
}

pub fn run(cmd: &str, args: &[&str]) -> Option<String> {
    run_timeout(cmd, args, Duration::from_secs(5))
}

pub fn run_timeout(cmd: &str, args: &[&str], _timeout: Duration) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

pub fn run_lines(cmd: &str, args: &[&str]) -> Vec<String> {
    run(cmd, args)
        .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
        .unwrap_or_default()
}

pub fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn which(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if Path::new(&format!("{}/{}", dir, cmd)).exists() {
                return true;
            }
        }
    }
    false
}

#[allow(dead_code)]
pub fn try_parse<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().parse().ok()
}

#[allow(dead_code)]
pub fn pid_exists(pid: &str) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}

pub fn uptime_secs() -> f64 {
    read_file("/proc/uptime")
        .and_then(|s| s.split_whitespace().next().map(|x| x.to_string()))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

pub fn format_bytes(b: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut size = b;
    let mut i = 0;
    while size >= 1024.0 && i < units.len() - 1 {
        size /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{:.0} {}", b, units[0])
    } else {
        format!("{:.2} {}", size, units[i])
    }
}

pub fn format_duration(secs: f64) -> String {
    let d = secs as u64;
    let days = d / 86400;
    let h = (d % 86400) / 3600;
    let m = (d % 3600) / 60;
    let s = d % 60;
    if days > 0 {
        format!("{}d {}h {}m", days, h, m)
    } else if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}
