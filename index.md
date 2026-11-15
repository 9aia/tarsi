---
layout: home

hero:
  name: tarsi
  text: The release plane
  tagline: Changelog → semver bump → tag → publish — one ops wire for humans, agents, and campaigns.
  actions:
    - theme: brand
      text: Get started
      link: /tutorials/getting-started
    - theme: alt
      text: Release semantics
      link: /explanation/release-semantics

features:
  - title: Plan, don't mutate
    details: "`release` is dry-run by default — real mutation requires `dry_run: false`, the same `--yes` a human would give."
  - title: Per-unit tags
    details: "`{unit}-v{semver}` tags attribute commits by path and version each release unit independently."
  - title: Profiles as packs
    details: "Org conventions ship as `type: tarsi` tamias packs — layered config, versioned by the store."
  - title: One wire
    details: "ndjson ops over `tarsi-serve` — agents, peregrin campaigns, and humans speak the same protocol."
---

## What it is

Tarsi is the org's release plane: changelog → semver bump → tag →
publish, exposed as `tarsi-serve` ndjson ops. Repo config is
`tarsi.toml`; org conventions ride `type: tarsi` tamias profile
packs; there is deliberately no `tarsi` CLI.

## Why

Every repo was reinventing release plumbing — changelog rendering,
commit→bump inference, tag schemes, publish ordering. Tarsi makes
release *semantics* a shared, deterministic ops surface that
humans, agents, and peregrin campaigns all drive identically, with
dry-run as the default so probing never mutates.

## Boundary

- **Owns:** release semantics — plans, bumps, changelogs, tags,
  publish orchestration.
- **Doesn't own:** orchestration timing/approval
  ([peregrin](https://9aia.com/peregrin/)), distribution
  ([tamias](https://9aia.com/tamias/)), git itself
  ([lasius-vcs](https://github.com/9aia/lasius)).

## Where to go next

| You want to… | Go to |
| --- | --- |
| Cut a real release end to end | [Tutorial](/tutorials/getting-started) |
| Do a task (write tarsi.toml, author a profile, drive ops) | [How-to](/how-to/) |
| Understand the design (ops wire, semantics, boundaries) | [Explanation](/explanation/) |
| Look up an op, config field, or pack layout | [Reference](/reference/) |
