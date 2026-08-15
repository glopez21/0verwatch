use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub modules: ModuleConfig,
    #[serde(default)]
    pub thresholds: ThresholdConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    #[serde(default = "default_true")]
    pub kernel: bool,
    #[serde(default = "default_true")]
    pub cpu: bool,
    #[serde(default = "default_true")]
    pub memory: bool,
    #[serde(default = "default_true")]
    pub daemons: bool,
    #[serde(default = "default_true")]
    pub disk: bool,
    #[serde(default = "default_true")]
    pub network: bool,
    #[serde(default = "default_true")]
    pub security: bool,
    #[serde(default = "default_true")]
    pub processes: bool,
    #[serde(default = "default_true")]
    pub logs: bool,
    #[serde(default = "default_true")]
    pub gpu: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    #[serde(default = "default_cpu_crit")]
    pub cpu_critical: f64,
    #[serde(default = "default_cpu_warn")]
    pub cpu_warning: f64,
    #[serde(default = "default_mem_crit")]
    pub mem_critical: f64,
    #[serde(default = "default_mem_warn")]
    pub mem_warning: f64,
    #[serde(default = "default_disk_crit")]
    pub disk_critical: f64,
    #[serde(default = "default_disk_warn")]
    pub disk_warning: f64,
    #[serde(default = "default_load_factor")]
    pub load_avg_factor: f64,
    #[serde(default = "default_max_zombies")]
    pub max_zombies: usize,
    #[serde(default = "default_max_failed")]
    pub max_failed_services: usize,
}

fn default_true() -> bool { true }
fn default_cpu_crit() -> f64 { 90.0 }
fn default_cpu_warn() -> f64 { 75.0 }
fn default_mem_crit() -> f64 { 90.0 }
fn default_mem_warn() -> f64 { 75.0 }
fn default_disk_crit() -> f64 { 90.0 }
fn default_disk_warn() -> f64 { 75.0 }
fn default_load_factor() -> f64 { 0.7 }
fn default_max_zombies() -> usize { 0 }
fn default_max_failed() -> usize { 0 }

impl Default for Config {
    fn default() -> Self {
        Config {
            modules: ModuleConfig::default(),
            thresholds: ThresholdConfig::default(),
        }
    }
}
impl Default for ModuleConfig {
    fn default() -> Self {
        ModuleConfig {
            kernel: true, cpu: true, memory: true, daemons: true,
            disk: true, network: true, security: true, processes: true,
            logs: true, gpu: true,
        }
    }
}
impl Default for ThresholdConfig {
    fn default() -> Self {
        ThresholdConfig {
            cpu_critical: 90.0, cpu_warning: 75.0,
            mem_critical: 90.0, mem_warning: 75.0,
            disk_critical: 90.0, disk_warning: 75.0,
            load_avg_factor: 0.7, max_zombies: 0, max_failed_services: 0,
        }
    }
}

impl Config {
    pub fn load(path: Option<&str>) -> anyhow::Result<Self> {
        if let Some(p) = path {
            let content = std::fs::read_to_string(p)?;
            return Ok(toml::from_str(&content)?);
        }
        let mut candidates: Vec<String> = vec![
            "./0verwatch.toml".into(),
            "./config.toml".into(),
        ];
        if let Ok(h) = std::env::var("HOME") {
            candidates.push(format!("{}/.config/0verwatch/0verwatch.toml", h));
            candidates.push(format!("{}/.0verwatch.toml", h));
        }
        for c in &candidates {
            if let Ok(content) = std::fs::read_to_string(c) {
                if let Ok(cfg) = toml::from_str::<Config>(&content) {
                    return Ok(cfg);
                }
            }
        }
        Ok(Config::default())
    }

    pub fn enabled_modules(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        macro_rules! push_if {
            ($e:expr, $n:expr) => {
                if $e { out.push($n); }
            };
        }
        push_if!(self.modules.kernel, "kernel");
        push_if!(self.modules.cpu, "cpu");
        push_if!(self.modules.memory, "memory");
        push_if!(self.modules.daemons, "daemons");
        push_if!(self.modules.disk, "disk");
        push_if!(self.modules.network, "network");
        push_if!(self.modules.security, "security");
        push_if!(self.modules.processes, "processes");
        push_if!(self.modules.logs, "logs");
        push_if!(self.modules.gpu, "gpu");
        out
    }
}

pub fn parse_module_list(s: &str) -> HashSet<String> {
    s.split(',')
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}