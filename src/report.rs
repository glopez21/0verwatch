use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub timestamp: String,
    pub hostname: String,
    pub overall_score: u8,
    pub modules: Vec<(String, ModuleReport)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleReport {
    pub data: Value,
    pub findings: Vec<(String, String)>,
    pub score: Option<u8>,
}

impl ModuleReport {
    pub fn new(data: Value) -> Self {
        ModuleReport {
            data,
            findings: Vec::new(),
            score: None,
        }
    }

    pub fn critical(&mut self, msg: impl Into<String>) {
        self.findings.push(("critical".into(), msg.into()));
    }
    pub fn warning(&mut self, msg: impl Into<String>) {
        self.findings.push(("warning".into(), msg.into()));
    }
    pub fn info(&mut self, msg: impl Into<String>) {
        self.findings.push(("info".into(), msg.into()));
    }
    pub fn ok(&mut self, msg: impl Into<String>) {
        self.findings.push(("ok".into(), msg.into()));
    }

    #[allow(dead_code)]
    pub fn score(mut self, s: u8) -> Self {
        self.score = Some(s);
        self
    }

    pub fn compute_score(&mut self) -> u8 {
        let mut penalty: i32 = 0;
        for (level, _) in &self.findings {
            match level.as_str() {
                "critical" => penalty += 20,
                "warning" => penalty += 8,
                "info" => penalty += 1,
                _ => {}
            }
        }
        let s = (100 - penalty).clamp(0, 100) as u8;
        self.score = Some(s);
        s
    }
}

impl Report {
    pub fn new(hostname: String) -> Self {
        Report {
            timestamp: chrono_now(),
            hostname,
            overall_score: 100,
            modules: Vec::new(),
        }
    }

    pub fn finalize(&mut self) {
        let mut total: i32 = 0;
        let mut count = 0i32;
        for (_, m) in &self.modules {
            if let Some(s) = m.score {
                total += s as i32;
                count += 1;
            }
        }
        self.overall_score = if count > 0 {
            (total / count).clamp(0, 100) as u8
        } else {
            100
        };
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", y, mo, d, h, m, s)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}
