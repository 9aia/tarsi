# Getting started

tarsi is the org's release plane: it turns conventional commits into a
release — changelog → semver bump → tag → publish. There is **no
`tarsi` CLI**; the surface is `tarsi-serve`, an ndjson ops binary with
a one-shot front door (PL-014, org ledger ADR-0003).

## Install `tarsi-serve`

The command ships as a tamias pack (`libraries/tarsi` — `type: files`
declaring `tarsi-serve` as a `tools/bin/<triple>/` payload bin).

```bash
# From a tarsi checkout:
cargo xtask pack    # release-build → libraries/tarsi/tools/bin/<triple>/
tamias install ./libraries/tarsi   # deploys `tarsi-serve` onto PATH
```

Dev shortcut without tamias:

```bash
cargo xtask install   # release `tarsi-serve` → ~/.local/bin
```

## Check the wire

```bash
tarsi-serve manifest
# → {"name":"tarsi-serve","version":"…","ops":["manifest","profiles","plan","changelog","notes","release"]}
```

Two modes:

- **No args** — ndjson JSON-lines loop on stdio: one
  `{"op": …, "params": …}` per line in, one
  `{"ok": …, "result"|"error"}` per line out.
- **`tarsi-serve <op> [--dir D] [--set k=v]… [--yes|--dry-run]`** —
  one-shot via `hypha-serve-cli`; prints the result JSON, exit 0/1.
  `--yes` maps to `dry_run: false`; `--dry-run` forces the default.

## Plan your first release

Inside a repo with a `tarsi.toml` (or none — units default to cargo
workspace members):

```bash
tarsi-serve plan --dir .
```

The result lists each unit in topo order: inferred bump, new version,
tag, rendered changelog markdown, and publish targets — all computed,
nothing written. The full walkthrough is in
[Your first release](/tutorials/first-release).
