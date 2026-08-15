use anyhow::Result;
use serde::Serialize;

use crate::report::Report;

#[derive(Debug, Clone)]
pub struct EventFlowConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Serialize)]
struct HostHealthPayload {
    overall_score: u8,
    hostname: String,
    modules: Vec<ModuleEntry>,
}

#[derive(Serialize)]
struct ModuleEntry {
    name: String,
    score: Option<u8>,
    findings: Vec<(String, String)>,
}

pub fn post_to_eventflow(cfg: &EventFlowConfig, report: &Report) -> Result<()> {
    let modules: Vec<ModuleEntry> = report
        .modules
        .iter()
        .map(|(name, mr)| ModuleEntry {
            name: name.clone(),
            score: mr.score,
            findings: mr
                .findings
                .iter()
                .map(|f| (f.0.clone(), f.1.clone()))
                .collect(),
        })
        .collect();

    let payload = HostHealthPayload {
        overall_score: report.overall_score,
        hostname: report.hostname.clone(),
        modules,
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!("{}/api/ingest/host_health", cfg.url.trim_end_matches('/'));

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);
    if let Some(ref key) = cfg.api_key {
        headers.insert("X-API-Key", key.parse()?);
    }
    if let Some(ref tid) = cfg.tenant_id {
        headers.insert("X-Tenant-ID", tid.parse()?);
    }

    let resp = client
        .post(&url)
        .headers(headers)
        .json(&payload)
        .send()?;

    if resp.status().is_success() {
        eprintln!("0verwatch: posted host_health to EventFlow ({})", resp.status());
        Ok(())
    } else {
        anyhow::bail!("EventFlow returned {}: {}", resp.status(), resp.text().unwrap_or_default())
    }
}
