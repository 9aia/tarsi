# Changelog

## [0.2.0] - 2026-11-13

### Added

- tarsi.toml dogfood config + golden-fixture e2e (B3 partial) (`f06c93e`)
- **serve:** one-shot CLI mode — <op> --dir --set --yes (`e61513b`)
- **core:** composable release strategies — data-driven bump/changelog policy (P-030) (`bfdbc8a`)
- **serve:** [release.sync_refs] — pack/doc version-ref sync inside the release pipeline (T-039) (`a0a1cbe`)
- **serve:** granular release ops — bump/tag/publish + check/schema (PL-014 D1) (`9f8ccec`)
- **ghreleases:** release asset upload — globs, dry-run report, idempotent replace (`6c2f62b`)
- **config:** type:tarsi release profiles — layered config + store discovery + 9aia-release pack (`110a329`)
- C-phase ports + tarsi-serve — manifest, publish, serve surface (`04ebe09`)

### Changed

- **serve:** one-shot via hypha-serve-cli (P-020) (`1cd7de4`)

### Fixed

- **serve:** T-033 cosmetic nits — changelog newline/order, plan publish enabled (`ddd64e6`)
- **serve:** release op — push before publish, error propagation, scoped staging, rollback (`b273aff`)
- **serve:** wire release.changelog_path + plan step order (T-035, T-036) (`78d37ff`)
