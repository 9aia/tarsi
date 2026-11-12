# Write a tarsi.toml

`tarsi.toml` (or `tarsi.yaml`/`tarsi.yml` — YAML wins on ties) is the
repo's release config. It is a plain [`tarsi_core::Config`](https://docs.rs)
document — the full schema is in [reference/config](/reference/config);
this page is the practical guide.

## Discovery

`tarsi-config` resolves config through scarab-config discovery:

1. `$TARSI_CONFIG` — explicit file, the whole config (no layering)
2. `./tarsi.{yaml,yml,toml}` in the repo being released
3. `~/.config/tarsi/`

When a [release profile](/how-to/author-release-profile) is active,
loading becomes layered instead of first-match:

```
repo tarsi.*  >  profile tarsi.*  >  ~/.config/tarsi/  >  defaults
```

Objects merge key-wise; repo keys win.

## The minimal file

```toml
[release]
styleguide = "keep-a-changelog"
changelog_path = "CHANGELOG.md"
diff_exclude = ["docs/**", "project/**", "*.md", ".github/**"]

[publish.crates]
enabled = false

[publish.ghreleases]
enabled = false
```

## `[release]` — what gets released

| key | default | meaning |
| --- | --- | --- |
| `units` | cargo workspace members | explicit unit list, or a units spec |
| `tag_format` | `{unit}-v{version}` | tag template; `{unit}`/`{version}` expand |
| `changelog_path` | `CHANGELOG.md` | changelog file spliced per unit |
| `styleguide` | `keep-a-changelog` | section layout of rendered markdown |
| `default_bump` | none | fallback bump when inference finds nothing |
| `diff_exclude` | `[]` | path globs kept out of commit→unit attribution |
| `unit_overrides` | `{}` | per-unit overrides, keyed by unit name |

### Units

Omit `units` entirely for cargo workspaces — every workspace member
becomes a release unit resolved from its manifest. To declare units
explicitly (non-cargo repos, filtered members), use the `units` spec —
see `tarsi_core::unit::UnitsSpec` for the shape (`Explicit { units }`
with per-unit `name`/`path`/`tag_format`/`publish`/`bump` fields).

### Overrides

```toml
[release.unit_overrides.tarsi-xtask]
skip = true

[release.unit_overrides.my-lib]
bump = "patch"          # force a bump regardless of inference
tag_format = "lib-{version}"
publish = ["crates-io"]  # restrict targets for this unit
```

`skip = true` drops the unit from planning entirely (e.g. a lib that
versions with its parent). Override `bump`/`tag_format`/`publish` merge
onto the resolved spec; per-unit values always beat repo-level defaults.

`diff_exclude` matters more than it looks: docs and config churn would
otherwise attribute commits to units and trigger spurious releases. The
org profile excludes `docs/**`, `project/**`, `*.md`, `.github/**`.

## `[publish.*]` — where releases go

Both targets default to **enabled** — set `enabled = false` for
tag-only repos (the org convention).

```toml
[publish.crates]
enabled = true
retry_429 = true        # backoff on crates.io rate limits
max_attempts = 5

[publish.ghreleases]
enabled = true
token_env = "GITHUB_TOKEN"   # env var NAME — never a value
repo = "owner/name"          # optional; defaults to origin remote
assets = ["dist/{unit}-{version}-*"]
```

- `token_env` is a secret **name** resolved via the environment/cuora
  at publish time; values never appear in config or logs.
- `assets` are repo-root-relative globs; `{unit}`/`{version}` expand
  per releasing unit. Magic patterns matching nothing are skipped;
  literal paths must exist.

`[publish] dry_run = true` exists in the schema but the serve surface
already dry-runs `release` by default — prefer `dry_run:false` at the
op level.

## Selecting a profile

```toml
profile = "9aia-release"
```

…or per-invocation, which beats both the config key and
`$TARSI_PROFILE`:

```bash
tarsi-serve plan --dir . --set profile=9aia-release
TARSI_PROFILE=9aia-release tarsi-serve plan --dir .
```

A named-but-missing profile is a hard error — releasing under silent
defaults would surprise.

## Workspace-version caveat

Units whose manifests use `version.workspace = true` share one version
file: the bump is last-writer-wins across releasing units. Keep
workspaces in lockstep or give units explicit versions until per-unit
workspace versions exist (org ledger ADR-0003, decision 4).
