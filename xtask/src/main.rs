#![cfg_attr(coverage, feature(coverage_attribute))]
//! xtask — repo chores. `cargo xtask <verb>` via the alias in
//! `.cargo/config.toml`.
//!
//! - `check`    — `cargo fmt --check` + `cargo clippy -D warnings`
//! - `test`     — `cargo nextest run` when installed, else `cargo test`
//! - `coverage` — `cargo llvm-cov` (HTML → target/llvm-cov/html);
//!   extra args pass through, e.g. `--fail-under-lines 97`
//! - `install`  — release-build `tarsi-serve` → ~/.local/bin
//! - `pack`     — stage the tamias pack payloads (`libraries/tarsi`
//!   + the `9aia-release` bundled driver)
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

/// Captured stdout of a command — for probes like `rustc -vV`.
fn run_out(args: &[&str]) -> Option<String> {
    let (prog, rest) = args.split_first()?;
    let out = Command::new(prog).args(rest).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The host target triple — the `tools/bin/<triple>/` selector
/// tamias uses for compiled payload bins.
fn host_triple() -> String {
    run_out(&["rustc", "-vV"])
        .and_then(|v| {
            v.lines()
                .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        })
        .unwrap_or_else(|| "x86_64-unknown-linux-gnu".to_string())
}

/// Release-build `bin` and stage it at `dest` (creating parents,
/// marking it executable on unix).
fn stage_release_bin(bin: &str, dest: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::copy(format!("target/release/{bin}"), dest)
        .map_err(|e| format!("copy {bin} → {}: {e}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// `cargo xtask pack` — stage the tamias pack payloads (T-014 +
/// PL-014 D3): release-build `tarsi-serve` into
/// `libraries/tarsi/tools/bin/<host-triple>/` (the payload-bin dir
/// `tamias install` links onto PATH) and `tamias-driver-tarsi`
/// into `libraries/9aia-release/drivers/bin/<host-triple>/` (the
/// bundled-driver dir tamias registers before dispatching the
/// `type: tarsi` install — the pack self-provisions its driver).
/// Gitignored on purpose: binaries are build artifacts; the
/// committed packs are just manifests + this staging verb.
fn pack() -> Result<(), String> {
    for pkg in ["tarsi-serve", "tarsi-driver"] {
        let status = Command::new("cargo")
            .args(["build", "--release", "-p", pkg])
            .status()
            .map_err(|e| format!("cargo build -p {pkg}: {e}"))?;
        if !status.success() {
            return Err(format!("cargo build --release -p {pkg} failed"));
        }
    }
    let triple = host_triple();
    let serve_dest = std::path::Path::new("libraries/tarsi/tools/bin")
        .join(&triple)
        .join("tarsi-serve");
    stage_release_bin("tarsi-serve", &serve_dest)?;
    let driver_dest = std::path::Path::new("libraries/9aia-release/drivers/bin")
        .join(&triple)
        .join("tamias-driver-tarsi");
    stage_release_bin("tamias-driver-tarsi", &driver_dest)?;
    println!("pack staged: {}", serve_dest.display());
    println!("pack staged: {}", driver_dest.display());
    println!();
    println!("install the tarsi-serve command (payload bin):");
    println!("  tamias install ./libraries/tarsi");
    println!("  # or a pinned tarball: tamias bundle pack libraries/tarsi");
    println!("install the org release profile (bundles its driver):");
    println!("  tamias install --allow-drivers ./libraries/9aia-release");
    Ok(())
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
        "install" => {
            // There is no `tarsi` cargo bin — the command ships as
            // the tamias pack (`cargo xtask pack` + `tamias install
            // ./libraries/tarsi`). This is the dev shortcut: release-
            // build `tarsi-serve` into ~/.local/bin.
            let status = Command::new("cargo")
                .args(["build", "--release", "-p", "tarsi-serve"])
                .status()
                .map_err(|e| format!("cargo build: {e}"));
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("xtask: {e}");
                    std::process::exit(1);
                }
            }
            let home = std::env::var_os("HOME")
                .map_or_else(|| std::path::PathBuf::from("/"), std::path::PathBuf::from);
            let out = home.join(".local/bin");
            if let Err(e) = stage_release_bin("tarsi-serve", &out.join("tarsi-serve")) {
                eprintln!("xtask: {e}");
                std::process::exit(1);
            }
            println!("installed tarsi-serve to {}", out.display());
            std::process::exit(0)
        }
        "pack" => match pack() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("xtask: {e}");
                std::process::exit(1);
            }
        },
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
            eprintln!("usage: cargo xtask <check|test|coverage|install|pack|package|smoke|site>");
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
