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
| `plan` | `dir`, `profile?`, `range?` | units in topo order: bump, new_version, tag, changelog_markdown, publish_targets; `strategy{name,resolved}` echoes the release strategy |
| `changelog` | same | per-unit `{unit, version, tag, markdown}` for releasing units |
| `notes` | same | concatenated release notes |
| `check` | same | read-only validation: `checks[]` (tag-available, manifest-present, changelog-not-drifted, publish-*) + plan; `ok:false` on drift |
| `schema` | — | the op surface as data (params + result shapes) — introspect before driving |
| `bump` | same + `dry_run` (default **true**) | step 1: changelogs + manifests + release commit → `bump{commit, units[]}` |
| `tag` | same + `sha?`, `push?`, `dry_run` | step 2: tags at `sha` (default HEAD); `push:true` pushes branch+tags — **required before `publish`** |
| `publish` | same + `commitish?`, `dry_run` | step 3: forge publishes; `commitish` pins the target (default HEAD) |
| `release` | same + `dry_run` (default **true**) | composed pipeline: plan → bump → tag → push → publish, with rollback |

`range` overrides the since-last-tag rev (e.g. `"HEAD~5..HEAD"`)
for every unit. `profile` selects a `type: tarsi` pack by name
(beats `$TARSI_PROFILE` and the config's `profile = "…"` key).

Typical flow: `check` → `plan` → show the human → on approval
either `release` with `dry_run:false` (atomic, rolls back on
failure) **or** the granular chain `bump` → `tag{push:true}` →
`publish`, each with `dry_run:false`. The granular ops are the
peregrin-campaign shape: every mutating op echoes `units` (the
plan-unit JSON it acted on) — pass it back as `params.units` to
the next op instead of re-planning (re-planning mid-chain sees
the bump commit and the new tags). Granular ops do **not** roll
back — `bump`/`tag`/`publish` report partial state for the
caller to compensate; `release` is the all-or-nothing path. Read
`rolled_back`/`publish`/`note` in results. A failed `release`
rolls back local+remote state when nothing published; partial
publishes are reported for manual cleanup.

## Repo config — `tarsi.toml`

A plain `tarsi_core::Config` document (TOML or YAML). Key sections:

- `[release]` — `units` (default: cargo workspace members),
  `tag_format` (default `{unit}-v{version}` — the org convention),
  `changelog_path`, `strategy` (release policy — see below),
  `default_bump` (fallback bump when inference finds nothing),
  `diff_exclude` (path globs stripped from commits before
  attribution — a commit left with no paths is dropped),
  `unit_overrides.<name>` (`bump`/`tag_format`/`publish`/`skip`),
  `sync_refs` (pack/doc version-ref rewrites — see below).
  `styleguide` is **deprecated** (superseded by
  `strategy.changelog`) — still parsed, ignored.
- `[release] strategy` — builtin name (`"conventional"`,
  `"semantic-release-angular"`, `"keep-a-changelog"`) or an inline
  table. Absent = `conventional`. `keep-a-changelog` keeps the
  conventional bump rules and swaps the changelog for the six
  canonical keepachangelog.com sections (Added / Changed /
  Deprecated / Removed / Fixed / Security): `feat`→Added,
  `refactor`+breaking-silent-kinds→Changed, `deprecate`/
  `deprecation`→Deprecated, `revert`/`remove`→Removed,
  `fix`/`perf`→Fixed, `security`→Security. Inline form:

  ```toml
  [release.strategy.bump]
  fallback = "patch"            # bump when no rule matches
  [[release.strategy.bump.rules]]
  type = "feat"                 # raw conventional type token ("*" = any)
  scope = "core-*"              # optional: exact or trailing-* glob
  breaking = true               # optional: pin the !/BREAKING flag
  not_type = ["fix"]            # optional: type tokens that veto
  bump = "minor"                # major|minor|patch|none — max-match fold

  [release.strategy.changelog]
  [[release.strategy.changelog.sections]]
  key = "added"                 # stable id
  heading = "Added"             # ### text
  [[release.strategy.changelog.sections.rules]]
  type = "feat"                 # first matching section wins;
                                # table order = render order
  ```

  Rules match the commit's **raw type token** (`deps`, `i18n`,
  … are nameable — not just the closed `CommitKind` set). Each
  absent `strategy.*` sub-table defaults to the `conventional`
  rules. The resolved strategy echoes on `plan` results as
  `plan.strategy {name, resolved}`.
- `[release.sync_refs]` — `rules[]` of `{files, find, replace}`:
  `files` are repo-root globs, `find` a regex, `replace` literal;
  `{old}`/`{new}` expand to the releasing units' shared
  current/new version. Runs inside `bump`/`release` before the
  release commit so synced refs (pack `tamias.yaml` versions,
  `name@ver` README/SKILL refs) land in the tagged tree — the
  stage that replaced every repo's `release.sh` plan+sed pre-step.
  Skips on mixed-version plans, idempotent, dry-run reports
  `status:"dry-run"` + `changed_files`. Empty `rules` = off.
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

Default strategy `conventional`: `feat`→minor, `fix`/`perf`→patch,
`!`/`BREAKING CHANGE`→major; chore/docs/test/etc. don't release
(`Bump::None`). Breaking commits of otherwise-silent kinds land in
changelog `Changed`. Other strategies are data — see
`[release] strategy` above (`semantic-release-angular` ships the
commit-analyzer angular preset; `keep-a-changelog` ships the
keepachangelog.com section layout over the same bump rules).
Commits attribute to units by
path prefix; repo-root files land in `unattributed`. Per-unit
bumps can disagree — with `version.workspace = true` the
workspace version is last-writer-wins (documented ADR-0003
limitation).

## Testing / dogfooding

- `cargo xtask check` + `cargo xtask test` — the gate.
- `tarsi-testkit` holds the golden plan corpus (`golden_cases()`)
  and the serve-op conformance suite (`serve::assert_*`) runnable
  against any `lasius_vcs::Vcs` on `repo::workspace_repo()`
  fixtures.
- Releasing tarsi itself is F-phase dogfooding — do not run real
  releases from an agent session; that's a human/campaign call.
