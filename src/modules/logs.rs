use crate::modules::util::*;
use crate::report::ModuleReport;
use serde_json::json;
use serde_json::Value;

pub fn inspect(rep: &mut ModuleReport) {
    let mut data = serde_json::Map::new();

    let has_journal = which("journalctl") && exists("/run/systemd/journal");

    if has_journal {
        let crit = run("journalctl", &["-p", "emerg", "--no-pager", "--output=cat", "--since", "-24h"])
            .unwrap_or_default();
        let crit_count = crit.lines().filter(|l| !l.is_empty()).count();
        let crit_samples: Vec<String> = crit.lines().filter(|l| !l.is_empty()).take(20).map(|l| l.chars().take(500).collect()).collect();

        let alert = run("journalctl", &["-p", "alert", "--no-pager", "--output=cat", "--since", "-24h"])
            .unwrap_or_default();
        let alert_count = alert.lines().filter(|l| !l.is_empty()).count();

        let err = run("journalctl", &["-p", "err", "--no-pager", "--output=cat", "--since", "-24h"])
            .unwrap_or_default();
        let err_count = err.lines().filter(|l| !l.is_empty()).count();
        let err_samples: Vec<String> = err.lines().filter(|l| !l.is_empty()).take(20).map(|l| l.chars().take(500).collect()).collect();

        let warn = run("journalctl", &["-p", "warning", "--no-pager", "--output=cat", "--since", "-24h"])
            .unwrap_or_default();
        let warn_count = warn.lines().filter(|l| !l.is_empty()).count();

        data.insert("emerg".into(), json!(crit_count));
        data.insert("alert".into(), json!(alert_count));
        data.insert("errors_24h".into(), json!(err_count));
        data.insert("warnings_24h".into(), json!(warn_count));
        if !crit_samples.is_empty() {
            data.insert("emerg_samples".into(), json!(crit_samples));
        }
        if !err_samples.is_empty() {
            data.insert("error_samples".into(), json!(err_samples));
        }

        if crit_count > 0 {
            rep.critical(format!("{} emergency-level message(s) in last 24h", crit_count));
        }
        if alert_count > 0 {
            rep.critical(format!("{} alert-level message(s) in last 24h", alert_count));
        }
        if err_count > 50 {
            rep.warning(format!("{} error-level message(s) in last 24h", err_count));
        } else if err_count > 0 {
            rep.info(format!("{} error-level message(s) in last 24h", err_count));
        }
    } else {
        let mut err_count = 0;
        let mut warn_count = 0;
        let mut err_samples: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/var/log") {
            for e in entries.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !name.ends_with(".log") && !name.ends_with("log") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines().rev().take(10000) {
                        if line.contains(" ERROR") || line.contains(" error:") {
                            err_count += 1;
                            if err_samples.len() < 20 { err_samples.push(line.chars().take(500).collect()); }
                        } else if line.contains(" WARN") || line.contains(" warning") {
                            warn_count += 1;
                        }
                    }
                }
            }
        }
        data.insert("errors_24h".into(), json!(err_count));
        data.insert("warnings_24h".into(), json!(warn_count));
        if !err_samples.is_empty() {
            data.insert("error_samples".into(), json!(err_samples));
        }
        if err_count > 50 {
            rep.warning(format!("{} error message(s) in /var/log", err_count));
        } else if err_count > 0 {
            rep.info(format!("{} error message(s) in /var/log", err_count));
        }
    }

    rep.data = Value::Object(data);
    rep.compute_score();
}