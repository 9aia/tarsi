# Pack layout

tarsi ships to the org as two tamias packs under `libraries/`.

## `libraries/tarsi` — the command pack

`type: files` — ships the `tarsi-serve` binary. There is no `tarsi`
CLI and no `shim: cli` bin by design (PL-014 surface split:
release-expertise libs + `tarsi-serve` only).

```yaml
# tamias.yaml (abridged)
name: tarsi
version: "0.1.0"
type: files
description: >-
  Generic release tool — changelog → semver bump → tag → publish.
bins:
  - name: tarsi-serve
    payload: tools/bin/x86_64-unknown-linux-gnu/tarsi-serve
```

Layout:

```
libraries/tarsi/
├── tamias.yaml          # the manifest above
└── tools/bin/<triple>/  # gitignored — `cargo xtask pack` stages
    └── tarsi-serve      #   release builds here per host triple
```

```bash
cargo xtask pack                          # build payload
tamias install ./libraries/tarsi          # deploy tarsi-serve → PATH
# or, once published: tamias install tarsi@<ver>
```

Dev shortcut: `cargo xtask install` puts a release `tarsi-serve` in
`~/.local/bin` without the pack flow.

## `libraries/9aia-release` — the org profile pack

`type: tarsi` — the org's release conventions as a layered config
profile (PL-014 D2).

```
libraries/9aia-release/
├── tamias.yaml    # name/version/type: tarsi/description
├── tarsi.toml     # THE profile — a plain Config document
└── drivers/bin/<triple>/   # gitignored — `cargo xtask pack` stages
    └── tamias-driver-tarsi #   the content driver here
```

The `type: tarsi` contract:

- payload root MUST carry `tarsi.{yaml,yml,toml}` — no `[profile]`
  table (`profile` is the consumer-side selector key)
- `tamias.yaml`'s `description` is the profile's human label
- optional extras (`styleguide.md`, commit maps) resolve relative to
  the profile dir

Install self-provisions the driver — tamias registers bundled drivers
before dispatching `install`:

```bash
cargo xtask pack
tamias install --allow-drivers ./libraries/9aia-release
```

The store commit is the placement: the profile resolves by name,
versioned by label, the moment tamias commits it. See
[Author a release profile pack](/how-to/author-release-profile).

## xtask verbs

| command | does |
| --- | --- |
| `cargo xtask check` | fmt + clippy `-D warnings` |
| `cargo xtask test` | whole suite (nextest if present) |
| `cargo xtask pack` | release-build `tarsi-serve` → `libraries/tarsi/tools/bin/<triple>/` **and** `tamias-driver-tarsi` → `libraries/9aia-release/drivers/bin/<triple>/` |
| `cargo xtask install` | release `tarsi-serve` → `~/.local/bin` (dev shortcut) |

Both payload dirs are gitignored build artifacts — the manifests are
the source of truth.
