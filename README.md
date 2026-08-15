# 0verwatch

Comprehensive system health assessment tool. Surveys the whole machine — kernel,
daemons, memory, CPU cores, disks, network, security, processes, logs, and GPUs —
and produces a scored health report.

Written in Rust. Console (human-readable) and JSON output. Configurable via TOML.

## What it checks

| Module | What it assesses |
|---|---|
| `kernel` | version, taint flags, dmesg errors/warnings/oops/panics, kptr/dmesg_restrict, panic_on_oops, loaded modules, boot cmdline, uptime |
| `cpu` | per-core utilization & frequency, governor, load average, context switches, temps, throttling |
| `memory` | RAM/swap usage, hugepages, OOM kills, PSI (pressure stall), EDAC memory errors |
| `daemons` | systemd failed/running/enabled units, sockets, timers, system state, zombies (falls back to init.d) |
| `disk` | mount usage, read-only mounts, diskstats I/O, SMART health, dirty fs check |
| `network` | interfaces/link/speed/mtu, rx/tx bytes & error/drop counts, TCP state counts, listening ports, retransmits, conntrack, DNS |
| `security` | SELinux/AppArmor/Smack, kptr/dmesg/perf/yama restrictions, failed logins, ssh root login, listening services |
| `processes` | process count by state, zombies, D-state, top 10 by CPU and memory |
| `logs` | journalctl emerg/alert/err/warning counts (24h) with samples, falls back to /var/log |
| `gpu` | PCI GPU detection (lspci + sysfs), NVIDIA (nvidia-smi), AMD (rocm-smi + sysfs), Intel (sysfs + intel_gpu_top) — utilization, memory, temp, power, clocks, throttling |

Each module computes a 0–100 score; the overall score is the average.

## Build

```bash
cd ~/projects/0verwatch
cargo build --release
```

Binary: `target/release/0verwatch`

## Install

A wrapper at `~/.local/bin/0verwatch` (already created) runs the release build:

```bash
0verwatch                 # full text report
0verwatch --json          # compact JSON
0verwatch --json-pretty   # pretty JSON
0verwatch -m kernel,memory # run selected modules
0verwatch --score         # print only the 0-100 score
0verwatch --watch 2       # refresh every 2s
0verwatch --config path   # use a specific config file
0verwatch --no-color      # disable ANSI
```

Exit code reflects health: 0 = healthy (≥90), 1 = degraded/warning (≥40), 2 = critical (<40).

## Configuration

Defaults are baked in. Override with a TOML file at any of:

- `./0verwatch.toml`
- `./config.toml`
- `~/.config/0verwatch/0verwatch.toml`
- `~/.0verwatch.toml`
- path passed via `--config`

See `config.toml` for all options (enable/disable modules, set warning/critical
thresholds for CPU, memory, disk, load-avg factor, max zombies, max failed services).

## JSON shape

```json
{
  "timestamp": "...",
  "hostname": "...",
  "overall_score": 86,
  "modules": [
    ["kernel", {
      "data": { ... },
      "findings": [["critical", "..."], ["warning", "..."]],
      "score": 79
    }],
    ...
  ]
}
```

## Python

`python/0verwatch.py` is a thin wrapper that locates the binary and forwards
arguments. It also offers a `json_report()` helper for programmatic use.

## Dependencies

Rust crates: `clap`, `serde`, `serde_json`, `anyhow`, `sysinfo`, `toml`, `libc`.
External tools (auto-detected, optional): `dmesg`, `systemctl`, `df`, `smartctl`,
`lspci`, `nvidia-smi`, `rocm-smi`, `intel_gpu_top`, `journalctl`, `ss`.

## License

MIT