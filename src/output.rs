use crate::report::Report;
use serde_json::Value;

pub enum Format {
    Text,
    Json,
    JsonPretty,
}

pub fn render_text(report: &Report) -> String {
    let mut out = String::new();
    let width = 80;
    let sep = "─".repeat(width);

    out.push_str(&format!("\n{}\n", "━".repeat(width)));
    out.push_str(&format!(
        "  0VERWATCH  ::  SYSTEM HEALTH ASSESSMENT\n  Generated {}  ::  {}\n",
        report.timestamp, report.hostname
    ));
    out.push_str(&format!("{}\n\n", "━".repeat(width)));

    out.push_str(&render_score_bar("OVERALL", report.overall_score));
    out.push('\n');

    for (name, module) in &report.modules {
        let pretty = pretty_name(name);
        out.push_str(&format!("{}\n", sep));
        out.push_str(&format!("  {}  ", pretty.to_uppercase()));
        if let Some(score) = module.score {
            out.push_str(&format!("[{}]", badge(score)));
        }
        out.push_str(&format!("\n{}\n\n", sep));
        out.push_str(&render_kv(&module.data));
        if !module.findings.is_empty() {
            out.push('\n');
            for (level, msg) in &module.findings {
                out.push_str(&format!("    {} {}\n", level_glyph(level), msg));
            }
        }
        out.push('\n');
    }

    out.push_str(&format!("{}\n", "━".repeat(width)));
    out
}

fn pretty_name(n: &str) -> &'static str {
    match n {
        "kernel" => "Kernel Health",
        "cpu" => "CPU",
        "memory" => "Memory",
        "daemons" => "Daemons / Services",
        "disk" => "Disks / Storage",
        "network" => "Network",
        "security" => "Security",
        "processes" => "Processes",
        "logs" => "System Logs",
        "gpu" => "GPU",
        _ => "Unknown",
    }
}

fn render_score_bar(label: &str, score: u8) -> String {
    let filled = (score / 10).min(10) as usize;
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(10 - filled))
        .collect();
    format!("  {:<14} [{}] {:>3}/100  {}\n", label, bar, score, badge(score))
}

fn badge(score: u8) -> &'static str {
match score {
        90..=u8::MAX => "● HEALTHY",
        70..=89 => "◐ WARNING",
        40..=69 => "◑ DEGRADED",
        0..=39 => "○ CRITICAL",
    }
}

fn level_glyph(level: &str) -> &'static str {
    match level {
        "critical" => "✗",
        "warning" => "!",
        "info" => "•",
        "ok" => "✓",
        _ => "•",
    }
}

fn render_kv(data: &Value) -> String {
    let mut out = String::new();
    match data {
        Value::Object(map) => {
            let mut rows: Vec<(String, Value)> = map.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in rows {
                render_entry(&mut out, &k, v, 1);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                render_entry(&mut out, &format!("[{}]", i), v.clone(), 1);
            }
        }
        _ => out.push_str(&format!("    {}\n", data)),
    }
    out
}

fn render_entry(out: &mut String, key: &str, val: Value, depth: usize) {
    let indent = "    ".repeat(depth);
    match val {
        Value::Object(map) => {
            out.push_str(&format!("{}{}:\n", indent, humanize_key(key)));
            let mut rows: Vec<(String, Value)> = map.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in rows {
                render_entry(out, &k, v, depth + 1);
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                out.push_str(&format!("{}{}: (none)\n", indent, humanize_key(key)));
            } else if arr.iter().all(|v| !v.is_object()) {
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                out.push_str(&format!("{}{}: {}\n", indent, humanize_key(key), items.join(", ")));
            } else {
                out.push_str(&format!("{}{}:\n", indent, humanize_key(key)));
                for (i, v) in arr.iter().enumerate() {
                    render_entry(out, &format!("#{}", i + 1), v.clone(), depth + 1);
                }
            }
        }
        Value::Null => out.push_str(&format!("{}{}: —\n", indent, humanize_key(key))),
        Value::Bool(b) => out.push_str(&format!("{}{}: {}\n", indent, humanize_key(key), if b { "yes" } else { "no" })),
        Value::Number(n) => out.push_str(&format!("{}{}: {}\n", indent, humanize_key(key), n)),
        Value::String(s) => out.push_str(&format!("{}{}: {}\n", indent, humanize_key(key), s)),
    }
}

fn humanize_key(k: &str) -> String {
    if let Some(rest) = k.strip_prefix('#') {
        return format!("#{}", rest);
    }
    k.replace('_', " ")
}

pub fn render_json(report: &Report, pretty: bool) -> String {
    let val = serde_json::to_value(report).unwrap_or(Value::Null);
    if pretty {
        serde_json::to_string_pretty(&val).unwrap_or_default()
    } else {
        serde_json::to_string(&val).unwrap_or_default()
    }
}