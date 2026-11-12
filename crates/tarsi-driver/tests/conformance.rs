//! Driver conformance gate — runs pavo's wire-contract suite
//! ([`pavo_testkit::conformance::check_driver_with`]) against this
//! crate's own `tamias-driver-tarsi serve` binary.
//!
//! `manifest`/`views` never touch tarsi state; `view`/`query` ops
//! spawn `tarsi-serve` one-shot ops (`$TARSI_SERVE` overrides the
//! binary) and degrade to `{"error": …}` payloads when it can't
//! run — the wire stays `ok:true` either way.
//!
//! The timeout is raised above the 10s default, matching the
//! sibling drivers: the first data op can pay for a `tarsi-serve`
//! spawn + git log on a loaded machine, and a timed-out request
//! leaves a late response that desyncs the rest of the suite.

use std::path::Path;
use std::time::Duration;

#[test]
fn driver_passes_pavo_conformance() {
    let violations = pavo_testkit::conformance::check_driver_with(
        Path::new(env!("CARGO_BIN_EXE_tamias-driver-tarsi")),
        pavo_testkit::CheckOptions {
            timeout: Duration::from_secs(120),
        },
    );
    assert!(
        violations.is_empty(),
        "pavo conformance violations:\n{}",
        violations
            .iter()
            .map(|v| format!("  - {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
