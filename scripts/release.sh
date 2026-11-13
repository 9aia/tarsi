#!/usr/bin/env bash
# tarsi release — thin shim over tarsi-serve (dogfooding: this repo's
# own release rides the pipeline it ships; config: ./tarsi.toml).
# One call runs plan → changelog → bump → tag → push; publish
# targets are off (ADR-0004: crates.io is tamias-only, GitHub
# Releases unused — `tarsi-serve` itself ships as the tamias pack
# in `libraries/tarsi`, `type: files`).
# Operator-run on main — `tarsi-serve release` is dry-run by
# default; --yes is the explicit opt-in. Releasing tarsi is a
# human/campaign decision (PL-014 F), never an agent's.
#
# The `tarsi` pack manifest version sync rides inside the pipeline
# now (`[release.sync_refs]` in tarsi.toml — T-039): refs land in
# the bump commit the tags point at. The pack publish itself stays
# manual (`cargo xtask pack` stages the driver payload, then
# `tamias publish libraries/tarsi`).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# worktree-local env (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }

tarsi-serve release --dir . --yes

echo "released: tarsi (per-unit tags tarsi-*-v<ver>; the pack manifest version synced into the bump commit)"
echo "next: cargo xtask pack && tamias publish libraries/tarsi  # ship tarsi-serve to the org registry"
