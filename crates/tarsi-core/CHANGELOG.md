# Changelog

## [0.4.0] - 2026-11-13

### Added

- **publish:** cuora:// token_ref for publish tokens (T-041, T-042) (`9cd6643`)
- **core:** keep-a-changelog release strategy (P-030) (`71352a4`)

## [0.3.0] - 2026-11-13

### Added

- **publish:** cuora:// token_ref for publish tokens (T-041, T-042) (`9cd6643`)
- **core:** keep-a-changelog release strategy (P-030) (`71352a4`)

## [0.2.0] - 2026-11-13

### Added

- **core:** composable release strategies — data-driven bump/changelog policy (P-030) (`bfdbc8a`)
- tarsi-core — pure release domain (PL-014 B1/B2) (`a31a688`)
- **serve:** [release.sync_refs] — pack/doc version-ref sync inside the release pipeline (T-039) (`a0a1cbe`)
- **ghreleases:** release asset upload — globs, dry-run report, idempotent replace (`6c2f62b`)
- **config:** type:tarsi release profiles — layered config + store discovery + 9aia-release pack (`110a329`)

### Changed

- drop tarsi-llm — changelog polish composes with sepia (`a2c195a`)

### Fixed

- **serve:** wire release.changelog_path + plan step order (T-035, T-036) (`78d37ff`)
