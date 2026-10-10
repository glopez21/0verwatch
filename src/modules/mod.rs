pub mod cpu;
pub mod daemons;
pub mod disk;
pub mod gpu;
pub mod kernel;
pub mod logs;
pub mod memory;
pub mod network;
pub mod processes;
pub mod security;
pub mod util;

use crate::config::ThresholdConfig;
use crate::report::ModuleReport;

pub fn run_module(name: &str, thresholds: &ThresholdConfig) -> ModuleReport {
    let mut rep = ModuleReport::new(serde_json::Value::Null);
    match name {
        "kernel" => kernel::inspect(&mut rep),
        "cpu" => cpu::inspect(&mut rep, thresholds),
        "memory" => memory::inspect(&mut rep, thresholds),
        "daemons" => daemons::inspect(&mut rep, thresholds),
        "disk" => disk::inspect(&mut rep, thresholds),
        "network" => network::inspect(&mut rep),
        "security" => security::inspect(&mut rep),
        "processes" => processes::inspect(&mut rep),
        "logs" => logs::inspect(&mut rep),
        "gpu" => gpu::inspect(&mut rep),
        _ => {
            rep.info(format!("unknown module: {}", name));
        }
    }
    rep
}
