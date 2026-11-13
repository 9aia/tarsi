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
| `styleguide` | string | `keep-a-changelog` | rendered-section layout |
| `default_bump` | bump | — | fallback when inference finds nothing |
| `diff_exclude` | `[string]` | `[]` | path globs excluded from commit attribution |
| `unit_overrides` | table | `{}` | per-unit overrides by unit name |
| `sync_refs` | table | `{rules:[]}` | pack/doc version-ref rewrites before the release commit |

`bump` values: `"major"` | `"minor"` | `"patch"` | `"none"`.

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
