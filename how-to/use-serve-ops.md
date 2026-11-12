# Drive tarsi-serve ops

Every tarsi surface is an op on `tarsi-serve` — no CLI verbs, no
`--json` flag. This page covers both transports.

## One-shot mode

```bash
tarsi-serve <op> --dir <repo> [--set key=value]… [--yes|--dry-run]
```

`hypha-serve-cli` parses args into the op's `params`, dispatches, and
prints the result JSON. Exit 0 on `ok:true`, 1 otherwise.

```bash
tarsi-serve manifest
tarsi-serve profiles
tarsi-serve plan --dir .
tarsi-serve plan --dir . --set profile=9aia-release --set range=HEAD~5..HEAD
tarsi-serve release --dir . --yes        # the only mutating call
```

Flag mapping:

- `--set k=v` → `params[k] = "v"` (strings; repeat for multiple)
- `--yes` → `params.dry_run = false`
- `--dry-run` → `params.dry_run = true` (the default anyway)

## ndjson service mode

```bash
tarsi-serve    # no args — loop on stdio
```

One request per line, one response per line:

```json
{"op":"manifest","params":{}}
{"ok":true,"result":{"name":"tarsi-serve","version":"0.1.0","ops":[…]}}

{"op":"release","params":{"dir":".","dry_run":true}}
{"ok":true,"result":{"dry_run":true,"plan":{…},"publish":[…]}}
```

Errors come back as `{"ok":false,"error":"…"}` — including release
failures, which also carry the partial `result` (plan, steps run,
rollback report) for inspection.

This is the wire peregrin campaigns and agents use; `tarsi-serve` is
a composition root only — orchestration and UI live in the callers.

## Params reference

Common params (all ops accept `dir`, default `.`):

| param | type | meaning |
| --- | --- | --- |
| `dir` | string | repo being released |
| `profile` | string | `type: tarsi` profile name (beats `$TARSI_PROFILE` and config `profile`) |
| `range` | string | explicit rev range e.g. `"HEAD~5..HEAD"` — overrides since-last-tag for **every** unit |
| `dry_run` | bool | `release` only; **default `true`** — mutation needs `false` |

## The safe sequence

```text
manifest → profiles → plan → changelog → notes → release(dry_run:true) → release(dry_run:false)
```

Rules of thumb:

- **Always `plan` before `release`.** The plan is the review gate.
- **`dry_run:false` is the `--yes` equivalent** — get explicit human
  or campaign approval first; never run it against a repo you haven't
  planned.
- **`range` is a debugging tool**, not a release mechanism — it
  applies one range to all units and bypasses per-unit tag ranges.
- On failure, read `rolled_back` (clean undo) vs the manual-cleanup
  note (a publish target succeeded before another failed — the tag
  stays, live releases must not be orphaned).

## Calling from code

`tarsi-serve` is a lib too: `tarsi_serve::dispatch(&GitVcs, &Request)`
takes an injected `lasius_vcs::Vcs`, which is how
`tarsi-testkit`'s conformance suite exercises the ops against fixture
repos without a process.
