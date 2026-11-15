# Decisions

ADR-lite: the load-bearing decisions, each with the alternative it
beat and the consequence it carries. The contracts are pinned by
org-ledger ADRs — ADR-0003 (release semantics), ADR-0004 (tag-only
+ tamias publish), ADR-0005 (tarsi as the release plane), P-030
(strategy tables).

## Ops surface, not a CLI

**Decision:** there is no `tarsi` binary — `tarsi-serve` exposes
ndjson ops plus a one-shot argv front door.

- *Alternative:* a conventional clap CLI like every other engine.
- *Consequence:* humans, agents, and peregrin campaigns speak one
  wire; the surface is generatable (ops manifest → MCP/CLI tools).
  Cost: every interaction is a JSON round-trip, and `release.sh`
shims exist per repo to keep the muscle memory.

## Dry-run is the default

**Decision:** `release`-family ops mutate only with
`dry_run: false` (`--yes`).

- *Alternative:* mutate by default, flag to preview.
- *Consequence:* agents and campaigns can probe any repo safely;
  the explicit opt-in mirrors "service surfaces never mutate
unless asked" org-wide.

## Per-unit tags and ranges

**Decision:** `{unit}-v{version}` tags; each unit's range is
latest-tag-for-that-unit..HEAD.

- *Alternative:* one repo-level `v{semver}`.
- *Consequence:* units in a workspace release independently and
  only when their paths changed — the common case for polyrepos.

## Strategy as data, not code

**Decision:** bump inference and changelog sectioning are
data-driven rule tables (`[release] strategy`), not a plugin API.

- *Alternative:* executable plugins / per-repo code.
- *Consequence:* presets (`semantic-release-angular`,
`keep-a-changelog`) ship as config; arbitrary types like `deps`
are nameable. The documented P-030 boundary: non-semver
versioning or plugin execution would need a different mechanism.

## No `tarsi-llm`

**Decision:** changelog polish composes with sepia sessions at
the *caller*; tarsi itself never calls a model.

- *Alternative:* an in-repo LLM path for nicer notes.
- *Consequence:* release semantics stay deterministic and
auditable; the cuora/secrets boundary is untouched.

## Push before publish

**Decision:** mutation order ends `push branch+tags → publish`.

- *Alternative:* publish, then push.
- *Consequence:* GitHub Releases auto-creates missing tags at
  remote HEAD — pushing first keeps the tag pinned to the bumped
  commit. Load-bearing, pinned by ADR-0003.
