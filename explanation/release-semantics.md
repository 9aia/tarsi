# Release semantics

The semantics below are load-bearing — they are pinned by
**ADR-0003** in the org ledger (`9aia/project`,
`project/adrs/0003-tarsi-release-semantics-plan-dry-run-bump-tag-manifest.md`).
This page is the readable summary; the ADR is the contract.

## Dry-run is the default

`tarsi-serve release` mutates only with `dry_run: false` — the
`--yes` equivalent. Orchestration callers (peregrin campaigns,
agents) opt in explicitly; anything probing a repo gets plans,
never writes. This mirrors the org rule that service surfaces never
mutate unless asked.

## Bump inference

Conventional commits, attributed to units by path prefix. The
inference itself is a **strategy** (P-030) — data-driven rules in
`tarsi.toml`'s `[release] strategy`, defaulting to `conventional`:

| commit | bump |
| --- | --- |
| `feat` | minor |
| `fix` / `perf` | patch |
| `!` / `BREAKING CHANGE:` | major |
| `chore` / `docs` / `test` / … | none — no release |

Bump rules fold **max-match** (a commit contributes the highest
matching `bump`; the unit folds max across commits) against the
commit's **raw type token** — arbitrary types like `deps` are
nameable. Changelog sectioning is the same mechanism: ordered
`sections[]`, first match wins, table order = render order.
`semantic-release-angular` ships semantic-release's commit-analyzer
angular preset as data; custom tables cover anything else short of
non-semver versioning or plugin execution (the documented P-030
boundary).

`spec.bump` (unit override or explicit unit spec) wins over
inference; the strategy's `fallback` — filled by
`release.default_bump` when unset — is the repo-wide fallback when
inference finds nothing. Commits with empty path lists match every
unit — conservative attribution. `release.diff_exclude` strips
matching paths before attribution (a commit left with no paths is
dropped).

## Per-unit tags, per-unit ranges

Tag format is `{unit}-v{version}` per unit (`tamias-core-v0.4.2`),
not repo-level `v{semver}`. Each unit's commit range is
`{latest-tag-for-that-unit}..HEAD` — units release independently and
only when their own paths changed. `params.range` overrides with one
range for all units.

## Manifest mutation

Versions are written via `toml_edit` (`tarsi-manifest`) — formatting
and comments survive. Units with `version.workspace = true` bump
`[workspace.package].version` instead: **all releasing units in such
a workspace share one bump** (last-writer-wins today; per-unit
workspace versions are a known limitation).

## Ordering and rollback

The mutation phase is strictly ordered — changelogs → manifests →
scoped `git add` (never `-A`) → `chore(release): bump versions`
commit → per-unit tags → **push branch and tags** → publish. The
push-before-publish order is load-bearing: GitHub Releases
auto-creates `refs/tags/<tag>` at remote HEAD when the tag doesn't
exist yet, which would pin the release to the pre-bump commit.

Failure handling:

- Any mutation failure → rollback undoes exactly what ran (remote
  tags deleted, branch reset, local commit/tags removed, created
  changelog files deleted) and the op reports `rolled_back`.
- Publish failures roll back **only if nothing published** — once a
  target succeeded, deleting the tag would orphan a live release, so
  state is left and the report asks for manual cleanup.

## The LLM boundary

There is deliberately no `tarsi-llm`. Changelog polish composes with
**sepia**: the *caller* (peregrin node, agent) may prompt a sepia
session with the rendered changelog. Conventional-commit output is
the only in-repo path — deterministic by construction (ADR-0003,
decision 6).

## Secrets

Config carries env-var **names** (`token_env`, `CARGO_REGISTRY_TOKEN`
via cuora secrets records) — never values. The cuora boundary holds
end to end: names in manifests, values in the secrets broker.

## VCS ownership

Git history, tags, and push go through `lasius-vcs` +
`lasius-vcs-git` (org plan P-019). `tarsi-core` is pure — it sees
only `Commit` values; every VCS call lives in `tarsi-serve` (and
`tarsi-testkit` fixtures). New publish targets are a new
`tarsi-publish-*` crate plus a `PublishTarget` variant; serve
dispatch stays a match arm.
