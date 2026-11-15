# Boundary with tamias and peregrin

tarsi owns **release semantics** — changelog → bump → tag → publish.
Its neighbors own the layers around it:

| neighbor | owns | tarsi's relation |
| --- | --- | --- |
| **tamias** | content distribution — fetch, store, cache, dispense; `type:` content drivers | release *profiles* are `type: tarsi` packs; `tamias-driver-tarsi` validates/installs them; the `tarsi-serve` binary itself ships as a `type: files` pack (`libraries/tarsi`) |
| **peregrin** | orchestration — campaigns, multi-machine workflows | `libraries/release` bundle drives `tarsi-serve release --yes`; campaigns call the same ndjson ops humans and agents use |
| **lasius** | git & workspace expertise — `lasius-vcs` port | all history/tag/push goes through `lasius-vcs` + `lasius-vcs-git`; `tarsi-core` never touches git |
| **sepia** | session lifecycle | optional changelog polish — the caller may prompt a sepia session with rendered notes; no in-repo LLM path |
| **cuora** | secrets broker | `token_env` names resolve through env/cuora; values never enter config, manifests, or logs |
| **scarab** | config/template expertise | `tarsi.toml` discovery/layering is `scarab-config`'s `FileSource`/`LayeredFileSource`; tarsi-config is only the wiring |
| **hypha** | distributed substrate | `hypha-serve-cli` is the shared one-shot arg→params shim every `*-serve` bin uses |

## What tarsi is not

- **Not a CLI.** No `tarsi` binary, no verb tree — the surface is
  ops. (The legacy TypeScript changelog CLI under the README's
  bottom half is pre-rewrite reference only.)
- **Not an orchestrator.** `tarsi-serve` composes ports and answers
  ops; scheduling, approval flows, and UI live in peregrin/pavo.
- **Not a publisher of record for the org.** Org repos release
  tag-only; crates.io publishing is tamias-only and GitHub Releases
  are unused (ADR-0004). The `tarsi-publish-*` crates exist for
  external consumers who opt back in per-repo.
- **Not a registry.** It reads git history and writes tags/manifests;
  it does not host anything.

## The dogfooding loop

`tarsi`'s own `tarsi.toml` configures its release — units resolve
from the cargo workspace, `tarsi-xtask` is skipped, both publish
targets are off. Releasing tarsi itself is plan F of PL-014: a human
or campaign decision, never an agent's.
