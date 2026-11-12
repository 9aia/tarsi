---
layout: home

hero:
  name: tarsi
  text: The release plane
  tagline: Changelog → semver bump → tag → publish — one ops wire for humans, agents, and campaigns.
  actions:
    - theme: brand
      text: Get started
      link: /getting-started
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
