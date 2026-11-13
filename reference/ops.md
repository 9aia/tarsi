# Ops wire surface

`tarsi-serve` speaks ndjson JSON-lines: one `{"op": …, "params": …}`
per line on stdin, one `{"ok": …, "result"|"error"}` per line on
stdout — the same protocol shape as `sepia serve` / `tamias-driver`
verbs. `tarsi-serve manifest` returns this list authoritatively.

> Stage-wise note: granular per-stage ops may land as the surface
> grows. The six ops below are the current stable contract; check
> `manifest` for the live list before scripting against anything
> newer.

## Ops

### `manifest`

No params. Returns the service manifest:

```json
{"name":"tarsi-serve","version":"0.1.0","ops":["manifest","profiles","plan","changelog","notes","release"]}
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

Params: `dir?`, `profile?`, `range?`. Pure — computes, writes nothing.

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
    "steps": ["write-changelogs","bump-manifests","commit-bump","tag","push","publish"]
  }
}
```

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

### `release`

Same params plus `dry_run` (**default `true`**). Executes the plan.

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

- Plan/build error → op-level `{"ok":false,"error":"…"}`.
- Mutation failure → `ok:false` result with `rolled_back` per-step
  report (remote tag deletes, branch reset, local cleanup).
- Publish failure with nothing published → rollback, same shape.
- Publish failure after a success → `ok:false`, `rolled_back:
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
| `dry_run` | bool | `true` | `release` only — `false` is the mutation opt-in |

## One-shot mapping

`hypha-serve-cli` translates argv to params:

```bash
tarsi-serve release --dir . --set profile=9aia-release --yes
# ≡ {"op":"release","params":{"dir":".","profile":"9aia-release","dry_run":false}}
```

`--set k=v` values are strings; `--yes`/`--dry-run` set `dry_run`.
