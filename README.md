# Tarsi

> Generic release tool for the 9aia fleet — changelog → semver bump
> → tag → publish, as a tamias content type and peregrin campaign.
>
> The org release plane (project ADR-0005): `tarsi-serve` exposes the
> ops — `manifest`/`profiles`/`plan`/`changelog`/`notes`/`check`/
> `schema`/`bump`/`tag`/`publish`/`release`. There is no `tarsi` CLI;
> a repo's `scripts/release.sh` is a thin shim over `tarsi-serve`.

## Distribution

The command ships as a tamias pack — `libraries/tarsi/tamias.yaml`
declares `tarsi-serve` as a `tools/bin/<triple>/` payload bin (T-014).

```bash
cargo xtask pack    # release-build → libraries/tarsi/tools/bin/<triple>/
tamias install ./libraries/tarsi   # deploys `tarsi-serve` onto PATH
```

`tarsi-serve` has two modes: no args → ndjson ops loop on stdio;
`<op> [--dir D] [--set k=v]… [--yes|--dry-run]` → one-shot JSON
(`manifest`/`plan`/`changelog`/`notes`/`release`). A repo's
`tarsi.toml` configures units and publish targets; org release
defaults ride `type: tarsi` profile packs (`libraries/9aia-release`)
distributed by tamias, and peregrin's `libraries/release` bundle
drives `tarsi-serve release --yes`.

Dev shortcut without tamias: `cargo xtask install` puts a release
`tarsi-serve` in `~/.local/bin`.

Fleet rollout is opt-in per repo (ADR-0005): `scripts/fleet-check.sh`
sweeps the sibling clones and reports which carry `tarsi.toml` + a
`tarsi-serve` `release.sh` shim — run it from any checkout to see the
current state.

The legacy TypeScript implementation lives on the `legacy-ts` branch.

## Docs

`docs/` worktree — VitePress + Diátaxis → `9aia.com/tarsi`;
`cargo xtask site` builds it. Repo docs: [AGENTS](AGENTS.md) ·
[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) ·
[LICENSE](LICENSE) (MIT) · [project](project/index.md).
