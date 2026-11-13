#!/usr/bin/env bash
# fleet-check — PL-014 F3 rollout verification.
#
# Sweeps the sibling clones next to this repo (the org `repos/`
# workspace) and reports each repo's tarsi rollout state:
#
#   cfg=yes/no   — carries a tarsi.toml (release config landed)
#   shim=shim    — scripts/release.sh delegates to `tarsi-serve`
#   shim=legacy  — scripts/release.sh exists but predates the shim
#   shim=missing — no scripts/release.sh at all
#
# A repo "counts" as rolled out when cfg=yes AND shim=shim. Repos
# without either are listed as `pending` — expected for non-Rust /
# non-releasing repos (themes, site, ledger, private WIP).
#
# Usage: scripts/fleet-check.sh [workspace-root]
#   default workspace-root = parent dir of this repo's toplevel
#   (i.e. the org `repos/` dir when running inside a sibling clone).
#
# Exit: 0 when every repo with a release.sh is shimmed (pending
# repos don't fail the run — rollout is opt-in per ADR-0005);
# 1 on drift (legacy/missing shim where a release.sh exists is
# fine — only `legacy` fails, since it means a repo kept the old
# pipeline).
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
ws_root="${1:-$(dirname "$repo_root")}"

rolled=0 pending=0 drift=0
printf '%-24s %-7s %s\n' "repo" "cfg" "release.sh"
for d in "$ws_root"/*/; do
  name="$(basename "$d")"
  [ -d "$d/.git" ] || [ -f "$d/.git" ] || continue   # skip non-repos
  cfg=no; shim=missing
  [ -f "$d/tarsi.toml" ] && cfg=yes
  if [ -f "$d/scripts/release.sh" ]; then
    if grep -q 'tarsi-serve' "$d/scripts/release.sh"; then shim=shim; else shim=legacy; fi
  fi
  case "$cfg/$shim" in
    yes/shim)   state=rolled-out; rolled=$((rolled+1)) ;;
    */legacy)   state=DRIFT;       drift=$((drift+1)) ;;
    *)          state=pending;     pending=$((pending+1)) ;;
  esac
  printf '%-24s %-7s %-14s %s\n' "$name" "$cfg" "$shim" "$state"
done
echo "---"
echo "rolled-out: $rolled · pending: $pending · drift: $drift"
[ "$drift" -eq 0 ]
