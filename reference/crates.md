# Crates

Rust workspace, ports-and-adapters — the same anatomy as
tamias/bupha/peregrin. `tarsi-core` is pure domain (no I/O);
`tarsi-serve` is the composition root and the only shipped binary.

| crate | role |
| --- | --- |
| `tarsi-core` | pure domain — `Config`/`ReleaseConfig`/`PublishConfig` serde model, `plan_release` (bump inference + attribution + changelog render), `Bump`, `Commit`, `PublishTarget`, `ReleaseUnitSpec`/`UnitsSpec`. No I/O. |
| `tarsi-config` | config wiring — scarab-config `FileSource`/`LayeredFileSource` discovery, `$TARSI_CONFIG`, profile resolution (`profile.rs`: dir profiles + tamias-store profiles, `find_dir`/`list_profiles`). |
| `tarsi-manifest` | unit resolution + version writes — cargo-workspace member discovery, `apply_config` overrides, `toml_edit`-based `write_unit_version` (workspace-version aware). |
| `tarsi-publish` | publish port — `Publisher` trait + `PublishJob`; dispatch over target impls. |
| `tarsi-publish-crates` | crates.io publish impl (`publish`/`yank` via cargo, 429 retry). |
| `tarsi-publish-ghreleases` | GitHub Releases impl — release + asset upload, `target_commitish` pinning. |
| `tarsi-publish-dryrun` | dry-run publisher — reports what would publish, touches nothing. |
| `tarsi-driver` | `tamias-driver-tarsi` — the `type: tarsi` content driver (capabilities/install/deploy/uninstall/status/health/logs/hooks); validates profile packs, optional `profilesDir` materialization. |
| `tarsi-serve` | **the binary** — ndjson ops loop + `hypha-serve-cli` one-shot; composes config/manifest/lasius-vcs/publish into the six ops. Lib (`dispatch`/`serve`) + thin `main.rs`. |
| `tarsi-testkit` | golden plan corpus (`golden_cases()`) + serve-op conformance suite (`serve::assert_*`) over `lasius_vcs::Vcs` fixtures. |
| `xtask` | check / test / pack / install |

## Key external deps

- `lasius-vcs` + `lasius-vcs-git` — all git history/tag/push (P-019);
  `tarsi-core` sees only `Commit` values.
- `scarab-config` + `scarab-config-file` — config discovery and
  layering (P-017); no `tarsi-config-*` format crates.
- `hypha-serve-cli` — the shared one-shot argv→params shim.
- `semver`, `toml_edit`, `serde`/`serde_json` — versions, manifest
  mutation, the wire.

API docs: `bun run docs:api` renders `cargo doc` into
`public/api/` (linked from the built site under `/tarsi/api/`).
