#!/usr/bin/env bash
# fleet-check — PL-014 F3 rollout verification.
#
# Sweeps the sibling clones next to this repo (the org `repos/`
# workspace) and reports each repo's tarsi rollout state:
#
#   cfg=yes/no   — carries a tarsi.toml (release config landed)
#   shim=none    — no scripts/release.sh (the org standard: releases
#                  run `tarsi-serve release --dir .` directly)
#   shim=leftover— a scripts/release.sh still exists (drift — the
#                  per-repo shims were removed org-wide)
#
# A repo "counts" as rolled out when cfg=yes AND shim=none. Repos
# without either are listed as `pending` — expected for non-Rust /
# non-releasing repos (themes, site, ledger, private WIP).
#
# Usage: scripts/fleet-check.sh [workspace-root]
#   default workspace-root = parent dir of this repo's toplevel
#   (i.e. the org `repos/` dir when running inside a sibling clone).
#
# Exit: 0 when no repo carries a leftover scripts/release.sh
# (pending repos don't fail the run — rollout is opt-in per
# ADR-0005); 1 on drift (a leftover shim means the repo kept the
# old wrapper).
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
ws_root="${1:-$(dirname "$repo_root")}"

rolled=0 pending=0 drift=0
printf '%-24s %-7s %s\n' "repo" "cfg" "release.sh"
for d in "$ws_root"/*/; do
  name="$(basename "$d")"
  [ -d "$d/.git" ] || [ -f "$d/.git" ] || continue   # skip non-repos
  cfg=no; shim=none
  [ -f "$d/tarsi.toml" ] && cfg=yes
  [ -f "$d/scripts/release.sh" ] && shim=leftover
  case "$cfg/$shim" in
    yes/none)     state=rolled-out; rolled=$((rolled+1)) ;;
    */leftover)   state=DRIFT;      drift=$((drift+1)) ;;
    *)            state=pending;    pending=$((pending+1)) ;;
  esac
  printf '%-24s %-7s %-14s %s\n' "$name" "$cfg" "$shim" "$state"
done
echo "---"
echo "rolled-out: $rolled · pending: $pending · drift: $drift"
[ "$drift" -eq 0 ]
