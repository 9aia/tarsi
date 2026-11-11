//! tarsi-serve binary — two modes:
//! - no args: ndjson service loop on stdin/stdout (peregrin/agents)
//! - `<op> [--dir D] [--set k=v]… [--yes|--dry-run]`: one-shot via
//!   `hypha-serve-cli` (P-020 — the shared shim every *-serve bin
//!   uses). Prints the op's result JSON, exit 0/1.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        std::process::exit(tarsi_serve::serve());
    }
    std::process::exit(hypha_serve_cli::one_shot(&args, |op, params| {
        let req = tarsi_serve::Request {
            op: op.to_string(),
            params: params.clone(),
        };
        let resp = tarsi_serve::dispatch(&lasius_vcs_git::GitVcs, &req);
        Ok(serde_json::to_value(&resp).unwrap_or_default())
    }));
}
