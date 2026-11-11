//! tarsi-serve binary — two modes:
//! - no args: ndjson service loop on stdin/stdout (peregrin/agents)
//! - `<op> [--dir D] [--set k=v]… [--yes|--dry-run]`: one-shot —
//!   prints the op's result JSON, exit 0/1. This is what shell
//!   composition (peregrin release bundle, release.sh replacements)
//!   calls.

use std::path::PathBuf;

use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        std::process::exit(tarsi_serve::serve());
    }
    std::process::exit(one_shot(&args));
}

fn one_shot(args: &[String]) -> i32 {
    let op = args[0].clone();
    let mut params = json!({ "dir": "." });
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                i += 1;
                if let Some(d) = args.get(i) {
                    params["dir"] = json!(d);
                }
            }
            "--set" => {
                i += 1;
                if let Some(kv) = args.get(i).and_then(|s| s.split_once('=')) {
                    let (k, v) = kv;
                    params[k] = parse_value(v);
                }
            }
            "--range" => {
                i += 1;
                if let Some(r) = args.get(i) {
                    params["range"] = json!(r);
                }
            }
            "--yes" | "--execute" => params["dry_run"] = json!(false),
            "--dry-run" => params["dry_run"] = json!(true),
            "-h" | "--help" => {
                eprintln!(
                    "tarsi-serve <op> [--dir D] [--set k=v]… [--yes|--dry-run]\n\
                     ops: manifest plan changelog notes release\n\
                     no args = ndjson service on stdin/stdout"
                );
                return 0;
            }
            other => {
                eprintln!("unknown flag {other:?}");
                return 2;
            }
        }
        i += 1;
    }
    // --set k=v parsing happens above; --yes sets dry_run=false
    // (release only — harmless elsewhere).
    let dir = params["dir"].as_str().unwrap_or(".").to_string();
    params["dir"] = json!(PathBuf::from(&dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&dir)));

    let req = tarsi_serve::Request { op, params };
    let resp = tarsi_serve::dispatch(&lasius_vcs_git::GitVcs, &req);
    println!("{}", serde_json::to_string(&resp).unwrap_or_default());
    if resp.ok {
        0
    } else {
        1
    }
}

/// `--set` values: JSON when parseable (`true`, `123`, `[…]`,
/// `{"…"}`), bare string otherwise.
fn parse_value(v: &str) -> Value {
    serde_json::from_str(v).unwrap_or_else(|_| json!(v))
}
