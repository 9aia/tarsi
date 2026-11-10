#![cfg_attr(coverage, feature(coverage_attribute))]
//! xtask — repo chores. `cargo xtask <verb>` via the alias in
//! `.cargo/config.toml`.
//!
//! - `check`    — `cargo fmt --check` + `cargo clippy -D warnings`
//! - `test`     — `cargo nextest run` when installed, else `cargo test`
//! - `coverage` — `cargo llvm-cov` (HTML → target/llvm-cov/html);
//!   extra args pass through, e.g. `--fail-under-lines 97`
//! - `install`  — release-install `tarsi` into ~/.cargo/bin
//! - `package`  — `cargo package --workspace` (verify: builds each packaged crate)
//! - `smoke`    — install `tarsi` from crates.io + run it (post-publish check)
//! - `site`     — build the docs site (docs/ worktree, VitePress)

use std::process::{Command, ExitStatus};

fn run(args: &[&str]) -> ExitStatus {
    let empty: &[&str] = &[];
    let (prog, rest) = args.split_first().unwrap_or((&"true", empty));
    Command::new(prog).args(rest).status().unwrap_or_else(|e| {
        eprintln!("xtask: failed to spawn {prog}: {e}");
        std::process::exit(1)
    })
}

fn have_cargo_sub(sub: &str) -> bool {
    Command::new("cargo")
        .args([sub, "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (verb, rest) = args
        .split_first()
        .map_or(("help", &[][..]), |(v, r)| (v.as_str(), r));
    let rest_refs: Vec<&str> = rest.iter().map(String::as_str).collect();

    let status = match verb {
        "check" => {
            let fmt = run(&["cargo", "fmt", "--all", "--", "--check"]);
            if !fmt.success() {
                fmt
            } else {
                run(&[
                    "cargo",
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ])
            }
        }
        "test" => {
            if have_cargo_sub("nextest") {
                run(&["cargo", "nextest", "run"])
            } else {
                run(&["cargo", "test", "--workspace"])
            }
        }
        "coverage" => {
            if have_cargo_sub("llvm-cov") {
                let mut a = vec!["cargo", "llvm-cov", "--workspace"];
                a.extend(rest_refs.iter().copied());
                run(&a)
            } else {
                eprintln!("xtask: cargo-llvm-cov not installed");
                std::process::exit(1)
            }
        }
        "install" => run(&["cargo", "install", "--path", "crates/tarsi-cli", "--force"]),
        "package" => run(&["cargo", "package", "--workspace", "--allow-dirty"]),
        "smoke" => {
            eprintln!("xtask: smoke not wired yet (needs crates.io publish)");
            std::process::exit(1)
        }
        "site" => {
            eprintln!("xtask: site not wired yet (needs docs/ worktree)");
            std::process::exit(1)
        }
        _ => {
            eprintln!("usage: cargo xtask <check|test|coverage|install|package|smoke|site>");
            std::process::exit(2);
        }
    };
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_manifest_parses() {
        let manifest =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
                .expect("workspace Cargo.toml readable");
        assert!(manifest.contains("[workspace]"));
    }
}
