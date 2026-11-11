//! tarsi-serve binary — ndjson service loop on stdin/stdout.

fn main() {
    std::process::exit(tarsi_serve::serve());
}
