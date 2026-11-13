# tarsi.toml schema

The config document is `tarsi_core::Config` — TOML or YAML
(`tarsi.yaml`/`tarsi.yml` preferred on discovery ties). Pure serde
types in `crates/tarsi-core/src/config.rs`; loading/layering in
`tarsi-config`. Secret **names** appear (`token_env`); values never
do.

## Top level

```toml
profile = "9aia-release"   # optional — selector, not a table

[release]
…

[publish]
…
```

| key | type | default | meaning |
| --- | --- | --- | --- |
| `profile` | string | — | named `type: tarsi` profile to layer under this file |
| `release` | table | defaults | release planning config |
| `publish` | table | defaults | publish target config |

## `[release]`

| key | type | default | meaning |
| --- | --- | --- | --- |
| `units` | spec | cargo workspace members | explicit units (`UnitsSpec::Explicit { units }`) |
| `tag_format` | string | `{unit}-v{version}` | tag template; `{unit}`, `{version}` expand |
| `changelog_path` | string | `CHANGELOG.md` | changelog file spliced per unit |
| `strategy` | string \| table | `conventional` | release strategy — builtin name or inline rules table (below) |
| `default_bump` | bump | — | fallback bump when inference finds nothing (folded into the strategy's `fallback`) |
| `diff_exclude` | `[string]` | `[]` | path globs stripped from commits before attribution; a commit left with no paths is dropped |
| `unit_overrides` | table | `{}` | per-unit overrides by unit name |
| `sync_refs` | table | `{rules:[]}` | pack/doc version-ref rewrites before the release commit |
| `styleguide` | string | `keep-a-changelog` | **deprecated** — superseded by `strategy.changelog`; still parsed, ignored |

`bump` values: `"major"` | `"minor"` | `"patch"` | `"none"`.

### `[release] strategy`

The release *policy* as data (P-030): bump inference and changelog
sectioning. Either a builtin name or an inline table — absent means
`conventional`, the pre-strategy behavior.

```toml
strategy = "conventional"            # builtin name, OR inline:

[release.strategy.bump]
fallback = "patch"                   # bump when no rule matches

[[release.strategy.bump.rules]]
type = "feat"                        # raw conventional type token; "*"/absent = any
scope = "core-*"                     # optional: exact or trailing-* glob
breaking = true                      # optional: pin the !/BREAKING CHANGE flag
not_type = ["fix"]                   # optional: type tokens that veto the match
bump = "minor"                       # major|minor|patch|none

[release.strategy.changelog]
[[release.strategy.changelog.sections]]
key = "added"                        # stable section id
heading = "Added"                    # rendered as `### Added`
[[release.strategy.changelog.sections.rules]]
type = "feat"                        # same matcher fields, no `bump`
```

Semantics:

- **Bump rules are max-match** — a commit contributes the highest
  `bump` across its matching rules; a unit folds max across its
  commits (semantic-release commit-analyzer's "highest release
  type wins" — rule order can't be gamed). `fallback` applies when
  inference finds nothing; `release.default_bump` fills it when the
  strategy doesn't set one.
- **Changelog sections are first-match** — a commit lands in the
  first section (table order) with a matching rule; table order is
  also render order. Commits matching nothing are dropped.
- Matchers run on the commit's **raw type token** — `deps`,
  `i18n`, and other non-`CommitKind` types are nameable.
- Each absent `strategy.*` sub-table defaults to the `conventional`
  rules, so `[release.strategy.bump]` alone keeps the conventional
  changelog.
- Builtin names: `conventional` (default — byte-identical to the
  pre-P-030 hardcoded policy), `semantic-release-angular` (the
  commit-analyzer angular preset as data), and `keep-a-changelog`
  (the keepachangelog.com section layout — see below).
- The resolved strategy echoes on `plan` results as
  `plan.strategy {name, resolved}` (`name` = the builtin name or
  `"custom"`).

What can't be data stays code: changelog render formats beyond
section/heading/order, non-semver versioning, non-conventional
commit parsing, anything needing execution (P-030 boundary).

#### `keep-a-changelog`

Keep a Changelog (keepachangelog.com) is a *changelog layout*, not
a bump policy — the builtin keeps `conventional`'s bump rules
verbatim and swaps only the section table for the six canonical
sections, in keepachangelog.com order:

| section | commits |
| --- | --- |
| `### Added` | `feat` |
| `### Changed` | `refactor`, plus breaking commits of types with no section of their own (`chore!`, `deps!`, unknown `!` types) |
| `### Deprecated` | `deprecate`, `deprecation` |
| `### Removed` | `revert`, `remove` |
| `### Fixed` | `fix`, `perf` |
| `### Security` | `security` |

Breaking commits of section-owning types stay in their own
section (`fix!`→Fixed, `security!`→Security, …) — Changed's
catch-all vetoes them via `not_type`. Types outside the table
(`chore`, `docs`, `test`, `deps`, `release`, …) still count for
bump inference but are dropped from the changelog. KaC-named
types (`deprecate`, `revert`, `remove`, `security`) are
changelog-only here — they don't move the version unless
`default_bump` fills the fallback; a repo that wants e.g.
`revert`→patch takes the same table inline with one extra bump
rule.

### `[release.unit_overrides.<name>]`

| key | type | meaning |
| --- | --- | --- |
| `bump` | bump | force this unit's bump (beats inference) |
| `tag_format` | string | per-unit tag template |
| `publish` | `[target]` | restrict publish targets (`"crates-io"`, `"github-release"`) |
| `skip` | bool | drop the unit from planning entirely |

### `[release.sync_refs]`

Rewrites stale version refs in repo files *inside* the `bump`/`release`
pipeline — after manifests are bumped, before the release commit — so
the tagged tree is self-consistent. This is the stage that replaced the
per-repo `scripts/release.sh` plan+`sed` pre-step (T-039): pack
`tamias.yaml` manifest versions and `name@ver` README/SKILL install
examples.

```toml
[release.sync_refs]
rules = [
  # pack manifest version line (line anchor needs (?m))
  { files = ["libraries/myco/tamias.yaml"],
    find = "(?m)^version: \"{old}\"",
    replace = "version: \"{new}\"" },
  # doc refs — global, like the old `sed s|name@old|name@new|g`
  { files = ["README.md", ".agents/skills/myco/SKILL.md"],
    find = "myco@{old}",
    replace = "myco@{new}" },
]
```

| key | type | meaning |
| --- | --- | --- |
| `rules` | `[rule]` | rewrite rules applied in order; empty = stage disabled (default) |
| `rule.files` | `[string]` | repo-root-relative globs; non-matching globs are skipped |
| `rule.find` | string | regex matched against file contents — **required** |
| `rule.replace` | string | literal replacement — **required** |

`{old}`/`{new}` expand to the releasing units' shared
`current_version`/`new_version`. Inside `find` they are
regex-escaped (so `.` stays literal); inside `replace` they expand
literally. Patterns are **not** auto-bounded — `app@{old}` matches
the `app@1.2.3` prefix of `app@1.2.30`, exactly like the old `sed`
pre-steps; add `\b` when a boundary is needed (`regex` has no
look-around). Use `(?m)` for line anchors.

Semantics:

- **Single-version only** — sync needs one unambiguous `old → new`
  pair. A plan whose releasing units disagree on `new_version` (or
  `current_version`) reports `status:"skipped"` and the release
  proceeds unsynced — the shims' `len(vs)==1` guard.
- **Idempotent** — rules match the *old* version; once refs say
  `new`, nothing matches and the stage reports zero changes.
- **Dry-run** — `bump`/`release` with `dry_run:true` (the default)
  reports `status:"dry-run"` plus `changed_files` without writing.
- Results surface as `sync_refs{status, reason?, old, new,
  changes[], changed_files?}` on `bump` and `release`; `steps_run`
  gains a `"sync-refs"` entry between `bump-manifests` and
  `commit-bump` when rules are configured. Synced files are staged
  into the release commit itself — no separate pre-commit.

## `[publish]`

| key | type | default | meaning |
| --- | --- | --- | --- |
| `dry_run` | bool | `false` | schema-level flag; the serve op's `dry_run` param is what callers use |

### `[publish.crates]`

| key | type | default | meaning |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | crates.io publishing on/off |
| `retry_429` | bool | `true` | backoff on crates.io rate limits |
| `max_attempts` | int | `5` | publish attempts per unit |

### `[publish.ghreleases]`

| key | type | default | meaning |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | GitHub Releases on/off |
| `token_env` | string | `GITHUB_TOKEN` | env var **name** holding the token |
| `repo` | string | origin remote | `owner/name` override |
| `assets` | `[string]` | `[]` | repo-root globs; `{unit}`/`{version}` expand per unit |

## Layering

With a profile resolved, config loads through
`LayeredFileSource` — objects merge key-wise, later layers fill gaps:

```
repo tarsi.*  >  profile tarsi.*  >  ~/.config/tarsi/  >  defaults
```

Without a profile, discovery is first-match:
`$TARSI_CONFIG` → `./tarsi.*` → `~/.config/tarsi/`. A missing
profile name is a hard error; a present-but-broken `tarsi.*` file
always errors loudly.

## Example

See `tarsi.toml` at the repo root (dogfooding config) and
`libraries/9aia-release/tarsi.toml` (the org profile).
