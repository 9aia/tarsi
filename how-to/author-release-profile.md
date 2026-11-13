# Author a release profile pack

Release profiles are tamias content packs of `type: tarsi` — the org's
conventions (`9aia-release` in `libraries/9aia-release`) ship this way
so every repo can layer them under its own `tarsi.toml`.

## The `type: tarsi` manifest contract

```yaml
# tamias.yaml
name: my-release-profile
version: "0.1.0"
type: tarsi
description: >-
  Human label for the profile — surfaced by the `profiles` op.
```

Payload contract (validated by `tamias-driver-tarsi` at install):

- Payload root MUST carry `tarsi.{yaml,yml,toml}` — a plain
  `tarsi_core::Config` document. **No `[profile]` table**: `profile =
  "…"` is the *selector* key a consumer writes, not something the pack
  carries.
- Extra files may ride along (`styleguide.md`, commit maps, …);
  consumers resolve them relative to the profile dir.
- `description` in `tamias.yaml` is the profile's label; an optional
  top-level `description` in `tarsi.toml` also works for dir-only
  profiles.

## The profile `tarsi.toml`

Keep it to *conventions* — repo-specific overrides belong in the
repo's own `tarsi.toml` (repo keys always win). `9aia-release` is the
reference:

```toml
[release]
strategy = "conventional"   # P-030: builtin name or inline rules table
changelog_path = "CHANGELOG.md"
diff_exclude = ["docs/**", "project/**", "*.md", ".github/**"]
tag_format = "{unit}-v{version}"

[publish.crates]
enabled = false   # org: crates.io is tamias-only (ADR-0004)

[publish.ghreleases]
enabled = false   # org: tag-only releases
```

## How profiles resolve

`tarsi-config` discovery order for a named profile:

1. `--set profile=<name>` on the op (highest)
2. `$TARSI_PROFILE`
3. `profile = "<name>"` in the repo's config

Profile *directories* are found in this order (`find_dir`):

1. `$TARSI_PROFILES_DIR` / `~/.config/tarsi/profiles/<name>/` —
   unversioned dir profiles (dev/pinned)
2. The tamias store — every `<store>/name@label/` carrying a `tarsi.*`,
   newest version label wins (the store commit *is* the placement —
   the driver validates, it does not copy)

Layering with a profile active:

```
repo tarsi.*  >  profile tarsi.*  >  ~/.config/tarsi/  >  defaults
```

## Ship it

`tamias-driver-tarsi` (`crates/tarsi-driver`) is the content driver.
`cargo xtask pack` stages a release build into
`libraries/<pack>/drivers/bin/<triple>/`, and tamias registers
bundled drivers *before* dispatching install — so:

```bash
cargo xtask pack
tamias install --allow-drivers ./libraries/my-release-profile
```

self-provisions the driver and lands the profile in the store,
versioned by label. Verify resolution:

```bash
tarsi-serve profiles                       # lists installed profiles
tarsi-serve plan --dir . --set profile=my-release-profile
```

The driver's `status` verb reports which dir `find_dir` picks for a
name and from which source; `uninstall` removes `profilesDir` copies
(the store entry itself is dropped by tamias).

## Gotchas

- **Dir profiles shadow store versions.** `~/.config/tarsi/profiles/`
  is searched first and carries no version — a stale copy there wins
  over every store version, including ones `tamias remove` dropped.
  The driver only writes there when `profilesDir` is configured
  (opt-in escape hatch).
- **Unsafe names rejected.** Profile names are validated
  (`safe_name`) — stick to lowercase slug names like `9aia-release`.
- **No silent fallback.** Naming a missing profile errors the op;
  there is no "profile not found → defaults" path.
