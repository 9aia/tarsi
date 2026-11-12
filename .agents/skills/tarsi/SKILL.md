---
name: tarsi
description: Drive releases through tarsi-serve — plan/changelog/notes/release ops over ndjson, release profiles via type:tarsi tamias packs, per-unit tags and publishing. Trigger when cutting a release, writing a tarsi.toml, authoring a release profile pack, or debugging bump/tag/publish behavior.
metadata:
  version: "0.1.0"
---

# tarsi — the release plane

tarsi turns a repo's conventional commits into a release: plan →
changelog → version bump → tag → publish. There is **no `tarsi`
CLI** — every surface is an ndjson op on `tarsi-serve` (or the
`tamias-driver-tarsi` content driver for profile packs). Humans,
agents, and peregrin campaigns all speak the same wire.

## The one rule

**`release` is dry-run by default.** Real mutation requires
`{"dry_run": false}` — treat that as the `--yes` equivalent and get
explicit human/campaign approval first. Never run it against a
repo you haven't `plan`ned.

## Driving a release

```bash
tarsi-serve            # ndjson loop on stdio — one {"op","params"}
                       # per line, one {"ok","result"|"error"} back
```

Or one-shot (hypha-serve-cli): `tarsi-serve <op> --dir <repo>
--set key=value [--yes]`.

Ops (all JSON in/out — no `--json` flag exists or is needed):

| op | params | returns |
| -- | ------ | ------- |
| `manifest` | — | op directory |
| `profiles` | — | installed `type: tarsi` release profiles |
| `plan` | `dir`, `profile?`, `range?` | units in topo order: bump, new_version, tag, changelog_markdown, publish_targets |
| `changelog` | same | per-unit `{unit, version, tag, markdown}` for releasing units |
| `notes` | same | concatenated release notes |
| `release` | same + `dry_run` (default **true**) | executes the plan; pushes commit+tags *before* publishing |

`range` overrides the since-last-tag rev (e.g. `"HEAD~5..HEAD"`)
for every unit. `profile` selects a `type: tarsi` pack by name
(beats `$TARSI_PROFILE` and the config's `profile = "…"` key).

Typical flow: `plan` → show the human → on approval `release`
with `dry_run:false`. A failed publish rolls back local+remote
state when nothing published; partial publishes are reported for
manual cleanup — read `rolled_back`/`publish` in the result.

## Repo config — `tarsi.toml`

A plain `tarsi_core::Config` document (TOML or YAML). Key sections:

- `[release]` — `units` (default: cargo workspace members),
  `tag_format` (default `{unit}-v{version}` — the org convention),
  `changelog_path`, `styleguide`, `diff_exclude` (path globs kept
  out of commit attribution), `unit_overrides.<name>`
  (`bump`/`tag_format`/`publish`/`skip`).
- `[publish.crates]` / `[publish.ghreleases]` — `enabled`,
  `token_env` (secret **name**, never a value), `repo`,
  `assets` globs (`{unit}`/`{version}` expand per unit).
- `profile = "name"` — layer a `type: tarsi` pack under this file.

Config layers: repo `tarsi.*` > profile > `~/.config/tarsi/` >
defaults (objects merge key-wise).

## Release profiles — `type: tarsi` packs

Profiles are tamias content packs (`libraries/9aia-release` is the
org one): payload root carries a `tarsi.{yaml,yml,toml}` Config
document — **no `[profile]` table**; `profile` is the selector
key. `tamias-driver-tarsi` validates installs; the tamias store
commit is the placement (profiles resolve versioned by
`name@label`). `cargo xtask pack` stages the driver into
`drivers/bin/<triple>/` so `tamias install --allow-drivers
./libraries/<pack>` self-provisions.

## Bump inference (conventional commits)

`feat`/`perf` → minor, `fix` → patch, `!`/`BREAKING CHANGE` →
major; chore/docs/test/etc. don't release (`Bump::None`). Commits
attribute to units by path prefix; repo-root files land in
`unattributed`. Per-unit bumps can disagree — with
`version.workspace = true` the workspace version is last-writer-
wins (documented ADR-0003 limitation).

## Testing / dogfooding

- `cargo xtask check` + `cargo xtask test` — the gate.
- `tarsi-testkit` holds the golden plan corpus (`golden_cases()`)
  and the serve-op conformance suite (`serve::assert_*`) runnable
  against any `lasius_vcs::Vcs` on `repo::workspace_repo()`
  fixtures.
- Releasing tarsi itself is F-phase dogfooding — do not run real
  releases from an agent session; that's a human/campaign call.
