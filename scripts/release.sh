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
# Preserved repo step: the `tarsi` pack manifest version is synced
# to the new version *before* the release commit lands, so the
# tagged tree carries consistent versions. The pack publish itself
# stays manual (`cargo xtask pack` stages the driver payload, then
# `tamias publish libraries/tarsi`).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# worktree-local env (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }

ws_version() { grep -m1 '^version = "' Cargo.toml | cut -d'"' -f2; }

# Learn the version tarsi will bump to (releasing units share one
# workspace version — empty output means "don't sync").
new="$(tarsi-serve plan --dir . | python3 -c '
import json,sys
d=json.load(sys.stdin)
if not d.get("ok"): sys.exit(0)
units=[u for u in d["result"]["plan"]["units"] if u.get("releases")]
vs={u["new_version"] for u in units}
print(vs.pop() if len(vs)==1 else "")
')"
old="$(ws_version)"
if [ -n "$new" ] && [ "$new" != "$old" ]; then
    sed -i "s/^version: \"${old}\"/version: \"${new}\"/" libraries/tarsi/tamias.yaml
    git add libraries/tarsi/tamias.yaml
    git commit -qm "chore(release): sync tarsi@${new} version ref ahead of tarsi bump"
fi

tarsi-serve release --dir . --yes

echo "released: tarsi ${new:-$old} (per-unit tags tarsi-*-v<ver>; the pack ships tarsi-serve)"
echo "next: cargo xtask pack && tamias publish libraries/tarsi  # ship tarsi-serve@${new:-$old} to the org registry"
