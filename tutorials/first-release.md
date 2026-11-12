# Your first release

This tutorial cuts a real release of a small repo using only
`tarsi-serve`. It assumes `tarsi-serve` is on your PATH (see
[Getting started](/getting-started)) and the repo uses conventional
commits.

## 1. Prepare the repo

The repo needs a `tarsi.toml`. The minimal useful one:

```toml
# tarsi.toml
[publish.crates]
enabled = false

[publish.ghreleases]
enabled = false
```

Tag-only releases are the org convention (ADR-0004); without the
`enabled = false` lines both publish targets default to on. Units
default to cargo workspace members — for a non-cargo repo, declare
them explicitly (see [Write a tarsi.toml](/how-to/write-tarsi-toml)).

Make sure the history you want released is conventional:

```bash
git log --oneline
# feat(api): add retries        → minor bump
# fix(core): handle empty body  → patch bump
# chore: tidy imports           → no release on its own
```

## 2. Plan

```bash
tarsi-serve plan --dir .
```

Read the result. Each unit in `plan.units` carries:

- `spec` — resolved unit (name, path, tag format, publish targets)
- `bump` — `major` / `minor` / `patch` / `none`
- `new_version` — the version the bump would write
- `tag` — e.g. `my-unit-v1.2.0`
- `releases` — whether this unit actually releases
- `changelog_markdown` — the rendered changelog section
- `commits` — how many commits were attributed

`plan.unattributed` lists commits that matched no unit's paths —
repo-root files land there. `plan.steps` is the mutation sequence
`release` would run.

Nothing has been written. This is the review gate — always `plan`
before `release`.

## 3. Preview the notes

```bash
tarsi-serve changelog --dir .   # per-unit {unit, version, tag, markdown}
tarsi-serve notes --dir .       # concatenated release notes
```

## 4. Dry-run the release

```bash
tarsi-serve release --dir .
```

`release` is **dry-run by default** — this executes the full plan and
runs every publish target in dry-run mode, reporting what *would*
happen, and writes nothing. Equivalently over ndjson:

```json
{"op":"release","params":{"dir":".","dry_run":true}}
```

## 5. Release for real

Only after the plan looks right and a human approves:

```bash
tarsi-serve release --dir . --yes
```

`--yes` is `dry_run:false`. The mutation phase, in order:

1. Write `CHANGELOG.md` sections per releasing unit (created files
   are tracked for rollback).
2. Bump versions in manifests via `toml_edit` (comments preserved).
3. Stage only the touched files — `CHANGELOG.md`/`Cargo.toml` per unit
   plus root manifests — never `git add -A`.
4. Commit `chore(release): bump versions`.
5. Tag each releasing unit (`{unit}-v{version}`).
6. **Push branch + tags before publishing** — GitHub Releases
   auto-creates tags at remote HEAD otherwise.
7. Run publish targets (crates.io, GitHub Releases) against the
   pushed commit.

The result reports `steps_run`, per-target `publish` reports, and —
on failure — `rolled_back` or a manual-cleanup note for partial
publishes.

## 6. Verify

```bash
git tag -l '*-v*' | tail
git log --oneline -3
tarsi-serve plan --dir .   # now: nothing to release
```

The latest-tag-per-unit range means a second `plan` finds no new
commits since the tags you just pushed.

## Next steps

- Layer org conventions with a profile —
  [Author a release profile pack](/how-to/author-release-profile)
- Tune attribution and overrides —
  [Write a tarsi.toml](/how-to/write-tarsi-toml)
- Script it from an agent — [Drive tarsi-serve ops](/how-to/use-serve-ops)
