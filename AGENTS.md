# Tarsi

The org **release plane** (project ADR-0005): changelog → semver
bump → tag → publish, exposed as `tarsi-serve` ndjson ops
(`manifest`/`profiles`/`plan`/`changelog`/`notes`/`check`/`schema`/
`bump`/`tag`/`publish`/`release`) — there is no `tarsi` CLI.
Repo config is `tarsi.toml`; org release defaults ride `type: tarsi`
profile packs (`libraries/9aia-release`) distributed by tamias;
peregrin's `libraries/release` campaign drives the same ops.
Releases run `tarsi-serve release --dir .` directly — the
per-repo `scripts/release.sh` shims were removed org-wide
(PL-014 E1/F); the release *model* stays tag-only + `tamias publish`
per project ADR-0004 (crates.io is tamias-only). The canonical
agent surface is `.agents/skills/tarsi/SKILL.md` — read it before
driving a release; `release`-family ops are dry-run by default.

## Layout

| Crate | Role |
| --- | --- |
| `crates/tarsi-core` | pure release domain — plans, units, semver bumps, commit classification, changelog — no I/O |
| `crates/tarsi-manifest` | Cargo manifest scanning — workspace members → `ReleaseUnitSpec`, version reads/writes |
| `crates/tarsi-config` | `tarsi.toml`/`tarsi.yml` loading (scarab-config) + `type: tarsi` profile resolution |
| `crates/tarsi-serve` | `tarsi-serve` ops surface — ndjson ops + one-shot argv over tarsi-core + lasius-vcs |
| `crates/tarsi-publish` + `-crates`/`-ghreleases`/`-dryrun` | publish port + per-forge impls (crates.io, GitHub Releases, dry-run) |
| `crates/tarsi-driver` | `tamias-driver-tarsi` — `type: tarsi` pack driver; also a read-only pavo view driver (`manifest`/`serve`) |
| `crates/tarsi-testkit` | conformance fixtures + golden corpus |
| `xtask` | check / test / coverage / install / pack / package (+ `smoke`/`site` stubs) |

`scripts/fleet-check.sh`
sweeps sibling clones for `tarsi.toml` coverage (and flags any
leftover `scripts/release.sh`). `libraries/` holds
the tamias packs (`tarsi` command pack, `9aia-release` profile
pack).

## Commands

```bash
cargo xtask check          # fmt + clippy -D warnings
cargo xtask test           # whole suite (nextest if present)
cargo xtask coverage       # llvm-cov report; extra args pass through
cargo xtask install        # release tarsi-serve → ~/.local/bin
cargo xtask pack           # stage libraries/tarsi payload
                           # (release tarsi-serve → tools/bin/<triple>/)
cargo xtask package        # cargo package --workspace (verify)
cargo xtask smoke          # stub — not wired yet (needs crates.io publish)
cargo xtask site           # stub — not wired yet (needs docs/ worktree)
```

Git hooks live in `.hooks/` (`core.hooksPath` = `.hooks`).

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->

<!-- sturn:agent-awareness -->

## Sturn awareness

- **No PRs.** The pushed branch/commit is the deliverable; review
  streams continuously (sturn WAL), merge is a human/sturn call.
- sturn-wired repo (`sturn.toml` present): `sturn claim "<glob>"
  --ws <ws>` *before* edits — claims surface conflicts in ms.
- Ops via `sturn write|rename|delete`; `sturn snap` folds the ws
  into a real commit; `sturn push` publishes heads + refs/sturn/*.
- `sturn leave` on exit — drops the actor's claims.

<!-- /sturn:agent-awareness -->

## Specs

`specs/<name>.toml` is this repo's mimus-standard conformance spec —
approval-gated. Any change requires a live org proposal
(`proposal-NNNN`, status open|done) referenced in the spec header or
commit range, and no coverage regression vs merge-base. Guard:
workspace `../../scripts/spec-guard.py`; sweep:
`../../scripts/standard.sh`; regenerate coverage only via
`../../scripts/gen-repo-specs.py`.
