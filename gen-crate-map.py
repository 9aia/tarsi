#!/usr/bin/env python3
"""Generate the crate-map table inside reference/crates.md
(via the shared `cypera` tool — never hand-edit the marked region).

Thin wrapper over `cypera gen crate-map` — the per-repo python
implementation was retired in favor of the org-wide generator
(sibling clone `../cypera`, the shared successor to the per-repo
`docs/gen-*.py` scripts). The LAYER table below is the only
per-repo input: it rides `--layer` overrides so the crate map keeps
its layer vocabulary and ordering (cypera's canonical rank reproduces
every repo's ORDER table).

cypera resolution order:
  1. `cargo run -q --manifest-path <repo>/../cypera/Cargo.toml -p cypera`
  2. `cypera` on PATH

TARSI_ROOT overrides the `..` worktree layout (detached CI checkouts),
the same convention the retired python script used.
"""
import os, pathlib, shutil, subprocess, sys

DOCS = pathlib.Path(__file__).parent
ROOT = pathlib.Path(os.environ.get("TARSI_ROOT") or DOCS.parent)
OUT = DOCS / "reference/crates.md"

LAYER = {
    "tarsi-core": "domain",
    "tarsi-publish": "port",
    "tarsi-config": "adapter",
    "tarsi-manifest": "adapter",
    "tarsi-publish-crates": "adapter",
    "tarsi-publish-dryrun": "adapter",
    "tarsi-publish-ghreleases": "adapter",
    "tarsi-serve": "engine",
    "tarsi-driver": "driver",
    "tarsi-testkit": "testkit",
    "tarsi-xtask": "tooling",
}

def cypera():
    manifest = ROOT.parent / "cypera" / "Cargo.toml"
    if manifest.exists():
        return ["cargo", "run", "-q", "--manifest-path",
                str(manifest), "-p", "cypera", "--"]
    exe = shutil.which("cypera")
    if exe:
        return [exe]
    sys.exit("cypera not found — expected a ../cypera sibling clone "
             "or `cypera` on PATH")

args = (cypera() + ["gen", "crate-map", "--repo", str(ROOT),
                    "--out", str(OUT)])
for name, layer in LAYER.items():
    args += ["--layer", f"{name}={layer}"]
subprocess.run(args, check=True)
