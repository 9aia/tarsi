---
url: /tarsi/explanation/architecture.md
---
# Architecture

Tarsi is the org's **release plane**: one pure domain core, a
config loader, an ops wire, a publish port — and nothing else. It
answers questions about releases; it never decides *when* to
release or *who* approves.

```
 caller (human, agent, peregrin campaign — `tarsi-serve release --dir .`)
        │  ndjson {"op": …, "params": …}  or  one-shot argv
        ▼
   tarsi-serve ────────── hypha-serve-cli (shared arg→params shim)
        │                       │
        │  plans · bumps ·      │ tarsi.toml / type: tarsi profiles
        │  changelogs · tags    │ (scarab-config FileSource/Layered)
        ▼                       ▼
   tarsi-core (pure)      tarsi-manifest (Cargo.toml scan/patch)
        │
        ▼
   lasius-vcs ── git history, tags, push (tarsi-core never touches git)
        │
        ▼
   tarsi-publish port ── crates / ghreleases / dryrun impls
```

## The pieces

* **`tarsi-core`** — pure release domain: plans, units, semver
  bumps, commit classification, changelog rendering. No I/O.
* **`tarsi-manifest`** — Cargo manifest scanning: workspace members
  → `ReleaseUnitSpec`, version reads/writes via `toml_edit`.
* **`tarsi-config`** — `tarsi.toml`/`.yml` loading (scarab-config)
  plus `type: tarsi` profile-pack resolution.
* **`tarsi-serve`** — the ops surface: `manifest` / `profiles` /
  `plan` / `changelog` / `notes` / `check` / `schema` / `bump` /
  `tag` / `publish` / `release`. No `tarsi` CLI exists.
* **`tarsi-publish`** + `-crates` / `-ghreleases` / `-dryrun` — the
  publish port and per-forge impls.
* **`tarsi-driver`** — `tamias-driver-tarsi`: the `type: tarsi`
  pack driver, plus a read-only pavo view.
* **`tarsi-testkit`** — conformance fixtures + golden corpus.

## The ops wire

Two modes over the same ops: no args = ndjson JSON-lines on stdio;
`<op> [--dir D] [--set k=v] [--yes|--dry-run]` = one-shot argv via
the shared `hypha-serve-cli` shim. `release`-family ops are
**dry-run by default** — mutation requires `dry_run: false`
(`--yes`). Humans, agents, and peregrin campaigns speak identical
wire; there is deliberately no `tarsi-llm`.

## Profiles are packs

Org release conventions ship as `type: tarsi` tamias packs
(`libraries/9aia-release`) — layered config, versioned by the
tamias store. `tamias-driver-tarsi` validates and installs them;
`tarsi-config` resolves the layers through scarab-config's
`FileSource`/`LayeredFileSource`.

## Where it sits

In the org's layering — **delivery** (tamias distribution, scarab
config/templates, **tarsi** releases, bupha CI, hypha entity sync) →
**memory** (myco records, kuda code maps) → **process** (sturn
coordination, cuora secrets, mimus proofs, cicada schedules, lasius
workspace, sepia sessions, peregrin campaigns) → **interface** (pavo
views, MCP fronts).

Tarsi sits at the **release plane in the delivery layer**: every
repos run `tarsi-serve release --dir .` directly,
org conventions ride tamias profile packs, and peregrin's
`libraries/release` campaign drives the same ops — tarsi computes
plans and mutations; the process layer decides when to run them.

## Deeper reads

* [Release semantics](release-semantics.md) — the load-bearing
  rules (dry-run default, per-unit tags, ordering, rollback).
* [Boundaries](boundaries.md) — owns / doesn't-own, neighbors named.
* [Concepts](concepts.md) — units, strategies, profiles, the wire.
* [Decisions](decisions.md) — ADR-lite.
