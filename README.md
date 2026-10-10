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
0verwatch --eventflow-url https://eventflow.example.com   # push telemetry
```

Exit code reflects health: 0 = healthy (≥90), 1 = degraded/warning (≥40), 2 = critical (<40).

## Telemetry (EventFlow / Augur)

When given an EventFlow base URL, 0verwatch `POST`s a `host_health` event after
each survey to `{EVENTFLOW_URL}/api/ingest/host_health`:

```bash
0verwatch --eventflow-url https://eventflow.example.com \
          --eventflow-api-key "$EVENTFLOW_API_KEY" \
          --eventflow-tenant-id "$AUGUR_TENANT_ID"
```

Flags (each also reads an environment variable):

| Flag | Environment | Purpose |
|---|---|---|
| `--eventflow-url` | `EVENTFLOW_URL` | EventFlow base URL (enables telemetry) |
| `--eventflow-api-key` | `EVENTFLOW_API_KEY` | sent as `X-API-Key` header |
| `--eventflow-tenant-id` | `AUGUR_TENANT_ID` | sent as `X-Tenant-ID` header |

Payload:

```json
{
  "overall_score": 86,
  "hostname": "node-1",
  "modules": [
    { "name": "kernel", "score": 79, "findings": [["warning", "..."]] }
  ]
}
```

Push failures are logged to stderr and never abort the local report.

## Deploy as a systemd timer

`deploy/install.sh` builds the release binary, installs it to
`/usr/local/bin/0verwatch`, and installs a hardened oneshot service plus a timer
that runs a survey every 5 minutes (posting to EventFlow if configured).

```bash
sudo ./deploy/install.sh                    # build + install + enable timer
sudo INTERVAL=1min ./deploy/install.sh      # run every minute
sudo NO_TIMER=1 ./deploy/install.sh         # install binary only
sudo UNINSTALL=1 ./deploy/install.sh        # remove units + binary
```

Configure telemetry by editing `/etc/0verwatch/eventflow.env` (installed from
`deploy/eventflow.env.example`) and uncommenting `EVENTFLOW_URL` etc.

```bash
sudo systemctl status 0verwatch.timer
sudo systemctl start 0verwatch.service      # run one survey now
journalctl -u 0verwatch.service -f          # watch telemetry runs
```

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

Rust crates: `clap`, `serde`, `serde_json`, `anyhow`, `sysinfo`, `toml`, `libc`,
`reqwest`.
External tools (auto-detected, optional): `dmesg`, `systemctl`, `df`, `smartctl`,
`lspci`, `nvidia-smi`, `rocm-smi`, `intel_gpu_top`, `journalctl`, `ss`.

## License

MIT