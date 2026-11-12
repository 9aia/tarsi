//! `tamias-driver-tarsi` — dual-surface driver: the tamias content
//! driver for `type: tarsi` release-profile packs (PL-014 D3) AND
//! the pavo read-only view driver over tarsi release state. One
//! binary, two consumers, disjoint argv — see `tarsi_driver`'s
//! module docs for both contracts.
//!
//! pavo surface (verbs take no `--dir`/`--name`):
//!
//! - `manifest` — print the `pavo_protocol::DriverManifest` JSON
//!   (discovery probe; never touches tarsi state).
//! - `capabilities` — merged JSON advertising BOTH surfaces: pavo's
//!   `{"capabilities": ["pavo",…], "verbs": […"serve"…]}` shape plus
//!   tamias's `"types": ["tarsi"]` and the content verbs in
//!   `verbs` — one payload satisfies both discoverers.
//! - `serve` — ndjson JSON-RPC on stdio (`manifest`/`views`/`view`/
//!   `query`); queries ride `tarsi-serve` one-shot ops (plan
//!   preview, changelog preview, profile list). `$TARSI_SERVE`
//!   overrides the `tarsi-serve` binary (default: PATH).
//!
//! ```text
//! tamias-driver-tarsi manifest|capabilities|serve
//! tamias-driver-tarsi <verb> --dir <d> --name <n> [--config <f>]
//! ```
//!
//! `verb ∈ {config-schema, install, deploy, uninstall, status,
//! health, logs, hooks}`. tamias dispatches the lifecycle verbs
//! with `--dir`/`--name` always present; the probe verbs take no
//! flags. Every verb answers a JSON report on stdout; failures go
//! to stderr with exit 1.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use tarsi_driver as drv;

const USAGE: &str = "usage: tamias-driver-tarsi manifest|capabilities|serve\n       tamias-driver-tarsi <verb> --dir <d> --name <n> [--config <f>]\n       verbs: capabilities config-schema install deploy uninstall status health logs hooks";

/// The flags tamias appends to every dispatched verb.
#[derive(Debug, Default)]
struct Args {
    dir: Option<PathBuf>,
    name: Option<String>,
    config: Option<PathBuf>,
}

/// Parse the trailing argv after the verb: `--dir`/`--name`/
/// `--config` in `--flag v` or `--flag=v` form. Unknown flags and
/// stray positionals are errors — the wire is exact.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a, None),
        };
        let take = |i: &mut usize| -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("tamias-driver-tarsi: {flag} needs a value"))
        };
        match flag {
            "--dir" => out.dir = Some(PathBuf::from(take(&mut i)?)),
            "--name" => out.name = Some(take(&mut i)?),
            "--config" => out.config = Some(PathBuf::from(take(&mut i)?)),
            "--help" | "-h" => return Err("help".to_string()),
            other => return Err(format!("tamias-driver-tarsi: unknown flag '{other}'")),
        }
        i += 1;
    }
    Ok(out)
}

fn env() -> impl Fn(&str) -> Option<String> {
    |k| std::env::var(k).ok()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(verb) = args.first().map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match verb {
        // pavo probes — bare argv, no --dir/--name. `capabilities`
        // answers the merged pavo+tamias payload.
        "manifest" => {
            drv::emit(&drv::manifest_value());
            ExitCode::SUCCESS
        }
        "capabilities" => {
            drv::emit(&drv::capabilities_value());
            ExitCode::SUCCESS
        }
        "serve" => ExitCode::from(drv::serve(&env()) as u8),
        "config-schema" => {
            drv::emit(&drv::config_schema());
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        "install" | "deploy" | "uninstall" | "status" | "health" | "logs" | "hooks" => {
            let parsed = match parse_args(&args[1..]) {
                Ok(a) => a,
                Err(e) if e == "help" => {
                    println!("{USAGE}");
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("{e}\n{USAGE}");
                    return ExitCode::from(2);
                }
            };
            let (Some(dir), Some(name)) = (parsed.dir, parsed.name) else {
                eprintln!("tamias-driver-tarsi: --dir and --name are required\n{USAGE}");
                return ExitCode::from(2);
            };
            let inv = drv::Invocation {
                dir,
                name,
                config: parsed.config,
            };
            match drv::run_verb(verb, &inv, &env()) {
                Ok(report) => {
                    drv::emit(&report);
                    if report["ok"] == true {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        other => {
            eprintln!("tamias-driver-tarsi: unknown verb '{other}'\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
