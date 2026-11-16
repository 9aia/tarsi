---
url: /tarsi/reference/ops.md
---
# Ops wire surface

`tarsi-serve` speaks ndjson JSON-lines: one `{"op": …, "params": …}`
per line on stdin, one `{"ok": …, "result"|"error"}` per line on
stdout — the same protocol shape as `sepia serve` / `tamias-driver`
verbs. `tarsi-serve manifest` returns this list authoritatively, and
`schema` returns the same surface as data (params + result shapes).

## Ops

### `manifest`

No params. Returns the service manifest:

```json
{"name":"tarsi-serve","version":"0.1.0","ops":["manifest","profiles","plan","changelog","notes","check","schema","bump","tag","publish","release"]}
```

### `profiles`

No params (ignores `dir`). Returns every installed `type: tarsi`
release profile:

```json
{"profiles":[{"name":"9aia-release","version":"0.1.0","description":"…","source":"/path/to/dir"}]}
```

Discovery order: `$TARSI_PROFILES_DIR` / `~/.config/tarsi/profiles/`
(dir profiles) then the tamias store (`name@label`, newest label
wins). See [Author a release profile pack](/how-to/author-release-profile).

### `plan`

Params: `dir?`, `profile?`, `range?`, `units?`. Pure — computes,
writes nothing.

```json
{
  "specs": [ …resolved ReleaseUnitSpec… ],
  "plan": {
    "units": [
      {
        "spec": { … },
        "bump": "minor",
        "new_version": "0.3.0",
        "tag": "my-unit-v0.3.0",
        "releases": true,
        "changelog_markdown": "## [0.3.0] …",
        "commits": 4,
        "publish_targets": [{"target":"crates-io","enabled":false}]
      }
    ],
    "unattributed": [{"sha":"…","subject":"…"}],
    "steps": ["write-changelogs","bump-manifests","commit-bump","tag","push","publish"],
    "strategy": {"name":"conventional","resolved":{…rules…}}
  }
}
```

`strategy` echoes the release strategy resolved from
`[release] strategy` (P-030) — `name` is the builtin name or
`"custom"` for inline tables; `resolved` is the full rule set that
produced the bumps/changelogs.

### `changelog`

Same params as `plan`. Returns only the releasing units' rendered
notes:

```json
{"changelogs":[{"unit":"my-unit","version":"0.3.0","tag":"my-unit-v0.3.0","markdown":"…"}]}
```

### `notes`

Same params. Concatenated `changelog_markdown` of all releasing units
in plan order:

```json
{"notes":"## [0.3.0] …\n## [1.1.0] …\n"}
```

### `check`

Same params as `plan`. Read-only release validation — the pre-flight
before `release`: per releasing unit it checks the tag is free, the
manifest exists, the changelog isn't already released (drift), and
reports each publish target's effective enabled state.

```json
{"ok":true,"checks":[{"check":"tag-available","unit":"my-unit","ok":true,"detail":"tag my-unit-v0.3.0 is free"},…],"releasing":2,"plan":{…}}
```

`ok:false` when any *hard* check fails (tag taken, manifest missing,
changelog drifted); publish-target rows are informational — a
disabled target still reports `ok:true`.

### `schema`

No params. Returns this document as data — every op, its params,
and its result shape. Node authors introspect it instead of reading
source.

### `bump`

Granular pipeline step 1 of 3 — the mutation half of `release`
without tag/push/publish. Params: `dir?`, `profile?`, `range?`,
`units?`, `dry_run?` (**default `true`**). Writes changelogs, bumps
manifests (`toml_edit`), runs `[release.sync_refs]` rewrites, stages
the touched paths, and creates the `chore(release): bump versions`
commit. Result echoes `units` (the plan-unit JSON) so a caller can
chain `tag`/`publish` via `params.units` without re-planning:

```json
{"ok":true,"dry_run":false,"plan":{…},"bump":{"commit":"<sha>","units":[…],"steps_run":["write-changelogs","bump-manifests","sync-refs","commit-bump"]},"sync_refs":{"status":"applied",…},"units":[…]}
```

Failures leave partial working-tree state and report it (`created_files`,
`steps_run`, a cleanup note) — compensation is the caller's job;
`release` is the atomic path.

### `tag`

Granular step 2 — create the per-unit `{unit}-v{version}` tags.
Params: `dir?`, `profile?`, `range?`, `units?`, `sha?`, `push?`,
`dry_run?` (**default `true`**). `sha` is the tag target (default
HEAD — pass the `bump` commit); `push:true` pushes branch + tags —
**required before `publish`** (GitHub Releases auto-creates missing
tags at remote HEAD otherwise).

```json
{"ok":true,"dry_run":false,"tag":{"sha":"<sha>","tags":[{"tag":"my-unit-v0.3.0","sha":"<sha>"}],"pushed":{"branch":true,"tags":[…]}},"units":[…],"steps_run":["tag","push"]}
```

### `publish`

Granular step 3 — run the publish targets (crates.io, GitHub
Releases) for the units. Params: `dir?`, `profile?`, `range?`,
`units?`, `commitish?`, `dry_run?` (**default `true`**). `commitish`
pins the forge target (default HEAD). Result:

```json
{"ok":true,"dry_run":false,"publish":[{"unit":"my-unit","target":"github-release","ok":true,"detail":"…"}],"targets":[…],"units":[…]}
```

The granular chain `check → bump → tag(push:true) → publish` is what
peregrin campaign nodes compose; `release` runs the same sequence
atomically with rollback. `params.units` carries plan-unit JSON
between steps so each op acts on exactly the units the previous step
reported.

### `release`

Same params as `plan` plus `dry_run` (**default `true`**). Executes the plan.

Dry-run result:

```json
{"ok":true,"dry_run":true,"plan":{…},"steps_run":[],"publish":[{"target":…,"ok":…,"dry_run":true}…]}
```

Real result (`dry_run:false`):

```json
{
  "ok": true,
  "dry_run": false,
  "plan": {…},
  "steps_run": ["write-changelogs","bump-manifests","sync-refs","commit-bump","tag","push"],
  "publish": [ {"target":"github-release","ok":true,…} ],
  "sync_refs": {"status":"applied","old":"0.1.0","new":"0.2.0","changes":[…]}
}
```

`sync_refs` reports the `[release.sync_refs]` stage (pack/doc
version-ref rewrites folded into the bump commit — see
[config](/reference/config#release-sync_refs)): `status` is
`disabled` (no rules), `skipped` (+`reason`, e.g. mixed-version
plan), `dry-run`, or `applied`; `changes[]` lists per-rule
file/replacement counts and `changed_files` the touched paths.
Dry-run results carry the same shape with `status:"dry-run"`. The
`"sync-refs"` `steps_run` entry appears only when rules are
configured.

Failure shapes:

* Plan/build error → op-level `{"ok":false,"error":"…"}`.
* Mutation failure → `ok:false` result with `rolled_back` per-step
  report (remote tag deletes, branch reset, local cleanup).
* Publish failure with nothing published → rollback, same shape.
* Publish failure after a success → `ok:false`, `rolled_back:
  {"attempted":false,"reason":"…manual cleanup…"}` — state left in
  place deliberately.

`steps_run` is the mutation-phase ledger; `publish` reports carry
per-target `ok`, `disabled`, and target-specific fields (e.g. gh
release URL, crates.io warnings).

## Common params

| param | type | default | notes |
| --- | --- | --- | --- |
| `dir` | string | `"."` | repo root being released |
| `profile` | string | — | profile name; precedence: param > `$TARSI_PROFILE` > config `profile` |
| `range` | string | per-unit tag range | `"a..b"` rev range applied to **every** unit |
| `units` | array | releasing units | plan-unit JSON echoed by `plan`/`check`/`bump` — the granular ops (`bump`/`tag`/`publish`) act on exactly these instead of re-planning |
| `dry_run` | bool | `true` | `release`/`bump`/`tag`/`publish` — `false` is the mutation opt-in |
| `sha` | string | HEAD | `tag` only — commit the tags point at |
| `push` | bool | `false` | `tag` only — also push branch + tags (required before `publish`) |
| `commitish` | string | HEAD | `publish` only — forge target ref |

## One-shot mapping

`hypha-serve-cli` translates argv to params:

```bash
tarsi-serve release --dir . --set profile=9aia-release --yes
# ≡ {"op":"release","params":{"dir":".","profile":"9aia-release","dry_run":false}}
```

`--set k=v` values are strings; `--yes`/`--dry-run` set `dry_run`.
