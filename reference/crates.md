---
url: /tarsi/reference/crates.md
---
# Crates

Rust workspace, ports-and-adapters — the same anatomy as
tamias/bupha/peregrin. `tarsi-core` is pure domain (no I/O);
`tarsi-serve` is the composition root and the only shipped binary.

| crate | layer | purpose |
|---|---|---|
| `tarsi-core` | domain | Pure release-domain model — plans, units, semver bumps, commit classification, changelog. No I/O. |
| `tarsi-publish` | port | Publish port — per-forge Publisher impls (crates.io, GitHub Releases, dry-run). |
| `tarsi-config` | adapter | tarsi.toml/tarsi.yml loading — scarab-config file discovery over tarsi_core::Config, plus type:tarsi release-profile resolution. |
| `tarsi-manifest` | adapter | Cargo manifest scanning — workspace members → tarsi_core::ReleaseUnitSpec, version reads/writes. |
| `tarsi-publish-crates` | adapter | crates.io Publisher — dep-ordered cargo publish with 429 retry (idempotent re-runs). |
| `tarsi-publish-dryrun` | adapter | Dry-run Publisher — validates and reports every target without remote writes. |
| `tarsi-publish-ghreleases` | adapter | GitHub Releases publisher — release create + notes + asset upload via ureq (blocking, bupha-github convention). |
| `tarsi-serve` | engine | tarsi service surface — ndjson ops (plan/changelog/notes/check/schema + granular bump/tag/publish + release) over tarsi-core + lasius-vcs. |
| `tarsi-driver` | driver | tamias-driver-tarsi — tamias content driver for type:tarsi release-profile packs |
| `tarsi-testkit` | testkit | Conformance fixtures + golden corpus for tarsi ports/impls. |
| `tarsi-xtask` | tooling | Repo chores: check / test / coverage / install / package / smoke / site |

## Key external deps

* `lasius-vcs` + `lasius-vcs-git` — all git history/tag/push (P-019);
  `tarsi-core` sees only `Commit` values.
* `scarab-config` + `scarab-config-file` — config discovery and
  layering (P-017); no `tarsi-config-*` format crates.
* `hypha-serve-cli` — the shared one-shot argv→params shim.
* `semver`, `toml_edit`, `serde`/`serde_json` — versions, manifest
  mutation, the wire.

API docs: `bun run docs:api` renders `cargo doc` into
`public/api/` (linked from the built site under `/tarsi/api/`).
