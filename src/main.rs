mod config;
mod eventflow;
mod modules;
mod output;
mod report;

use clap::{Parser, ValueEnum};
use report::{ModuleReport, Report};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "0verwatch",
    version,
    about = "Comprehensive system health assessment"
)]
struct Cli {
    #[arg(
        short = 'm',
        long = "modules",
        help = "Comma-separated module list to run (default: all)"
    )]
    modules: Option<String>,

    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutFmt::Text, help = "Output format")]
    output: OutFmt,

    #[arg(long = "json", help = "Shortcut for --output json")]
    json: bool,

    #[arg(long = "json-pretty", help = "Shortcut for --output json-pretty")]
    json_pretty: bool,

    #[arg(long = "score", help = "Print only the overall health score (0-100)")]
    score: bool,

    #[arg(long = "config", help = "Path to config file")]
    config: Option<String>,

    #[arg(long = "watch", help = "Refresh every N seconds (0 = single run)")]
    watch: Option<u64>,

    #[arg(long = "no-color", help = "Disable ANSI colors")]
    no_color: bool,

    #[arg(
        long = "eventflow-url",
        env = "EVENTFLOW_URL",
        help = "EventFlow URL to post host_health events"
    )]
    eventflow_url: Option<String>,

    #[arg(
        long = "eventflow-api-key",
        env = "EVENTFLOW_API_KEY",
        help = "EventFlow API key"
    )]
    eventflow_api_key: Option<String>,

    #[arg(
        long = "eventflow-tenant-id",
        env = "AUGUR_TENANT_ID",
        help = "Tenant ID for EventFlow/Augur"
    )]
    eventflow_tenant_id: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutFmt {
    Text,
    Json,
    JsonPretty,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let cfg = match config::Config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("0verwatch: config error: {}", e);
            return ExitCode::from(2);
        }
    };

    let ef_cfg = cli
        .eventflow_url
        .as_ref()
        .map(|url| eventflow::EventFlowConfig {
            url: url.clone(),
            api_key: cli.eventflow_api_key.clone(),
            tenant_id: cli.eventflow_tenant_id.clone(),
        });

    let interval = cli.watch.unwrap_or(0);
    let once = interval == 0;

    loop {
        let report = build_report(&cli, &cfg);

        if let Some(ref ef) = ef_cfg {
            if let Err(e) = eventflow::post_to_eventflow(ef, &report) {
                eprintln!("0verwatch: EventFlow post failed: {}", e);
            }
        }

        emit(&cli, &report);

        if once {
            break;
        }

        if let Some(i) = cli.watch {
            std::thread::sleep(std::time::Duration::from_secs(i));
            print!("\x1b[2J\x1b[H");
        }
    }

    let final_report = build_report(&cli, &cfg);
    let score = final_report.overall_score;
    let exit = if score >= 90 {
        0
    } else if score >= 40 {
        1
    } else {
        2
    };
    if cli.score {
        if once {
            println!("{}", score);
        }
        return ExitCode::from(exit);
    }
    ExitCode::from(exit)
}

fn build_report(cli: &Cli, cfg: &config::Config) -> Report {
    let hostname = modules::util::read_first_line("/proc/sys/kernel/hostname")
        .unwrap_or_else(|| std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into()));

    let mut report = Report::new(hostname);

    let enabled: Vec<String> = if let Some(ref m) = cli.modules {
        config::parse_module_list(m).into_iter().collect()
    } else {
        cfg.enabled_modules()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    };

    for name in &enabled {
        let mut mrep = modules::run_module(name, &cfg.thresholds);
        let mut mr = ModuleReport::new(mrep.data.clone());
        mr.findings = mrep.findings.clone();
        mr.score = mrep.score;
        let _ = &mut mrep;
        report.modules.push((name.to_string(), mr));
    }

    report.finalize();
    report
}

fn emit(cli: &Cli, report: &Report) {
    if cli.score {
        return;
    }
    let fmt = if cli.json_pretty {
        output::Format::JsonPretty
    } else if cli.json {
        output::Format::Json
    } else {
        match cli.output {
            OutFmt::Text => output::Format::Text,
            OutFmt::Json => output::Format::Json,
            OutFmt::JsonPretty => output::Format::JsonPretty,
        }
    };
    match fmt {
        output::Format::Text => {
            let _ = cli.no_color;
            print!("{}", output::render_text(report));
        }
        output::Format::Json => println!("{}", output::render_json(report, false)),
        output::Format::JsonPretty => println!("{}", output::render_json(report, true)),
    }
}
