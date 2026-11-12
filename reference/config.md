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

`bump` values: `"major"` | `"minor"` | `"patch"` | `"none"`.

### `[release.unit_overrides.<name>]`

| key | type | meaning |
| --- | --- | --- |
| `bump` | bump | force this unit's bump (beats inference) |
| `tag_format` | string | per-unit tag template |
| `publish` | `[target]` | restrict publish targets (`"crates-io"`, `"github-release"`) |
| `skip` | bool | drop the unit from planning entirely |

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
