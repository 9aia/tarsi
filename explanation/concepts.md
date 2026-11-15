# Concepts

The domain vocabulary, each term defined once. Field-level
contracts live in [Reference](../reference/).

## Release unit

One independently versioned thing — a cargo workspace member or an
explicitly declared unit. Units resolve their own commit range
(`{latest-tag-for-that-unit}..HEAD`) and release only when their
paths changed.

## Op

The unit of the `tarsi-serve` wire: `manifest`, `profiles`, `plan`,
`changelog`, `notes`, `check`, `schema`, `bump`, `tag`, `publish`,
`release`. One JSON object in, one result out — identical for
humans, agents, and campaigns.

## Plan

The computed, unwritten release: per-unit bump, `new_version`,
tag, rendered changelog, publish targets, `steps` — plus
`unattributed` commits. `plan` is the review gate; `release`
executes it.

## Bump strategy

The data-driven rule table mapping commit types to semver bumps —
`[release] strategy` in `tarsi.toml`, defaulting to `conventional`.
`semantic-release-angular` ships the angular preset;
`keep-a-changelog` keeps the same bumps with KaC section names.
`spec.bump` and `release.default_bump` override inference.

## Tag format

`{unit}-v{version}` per unit (`tamias-core-v0.4.2`) — not a
repo-level `v{semver}`.

## Profile pack

A `type: tarsi` tamias package carrying org release defaults —
layered config resolved by `tarsi-config` through scarab-config.
Shipped via `libraries/9aia-release`.

## Publish target

A forge destination — `crates` (crates.io), `ghreleases`
(GitHub Releases), `dryrun`. Org repos keep both off: releases are
tag-only and crates.io publishing is tamias-only.

## Dry-run

The default for every `release`-family op: full plan + dry-run
publish, zero writes. Mutation requires `dry_run: false` (`--yes`).

## Rollback

Ordered mutation (changelogs → manifests → scoped add → bump
commit → tags → push → publish) with exact undo: remote tags
deleted, branch reset, created files removed. Publish failures
roll back only if *nothing* published.
