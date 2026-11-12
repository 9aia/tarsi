# Tarsi

The org **release plane** (project ADR-0005): changelog → semver
bump → tag → publish, exposed as `tarsi-serve` ndjson ops
(`manifest`/`profiles`/`plan`/`changelog`/`notes`/`check`/`schema`/
`bump`/`tag`/`publish`/`release`) — there is no `tarsi` CLI.
Repo config is `tarsi.toml`; org release defaults ride `type: tarsi`
profile packs (`libraries/9aia-release`) distributed by tamias;
peregrin's `libraries/release` campaign drives the same ops. Each
repo's `scripts/release.sh` is a thin shim over `tarsi-serve`
(PL-014 E1); the release *model* stays tag-only + `tamias publish`
per project ADR-0004 (crates.io is tamias-only). The canonical
agent surface is `.agents/skills/tarsi/SKILL.md` — read it before
driving a release; `release`-family ops are dry-run by default.

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->

<!-- sturn:agent-awareness -->

## Sturn awareness

- **No PRs.** The pushed branch/commit is the deliverable; review
  streams continuously (sturn WAL), merge is a human/sturn call.
- sturn-wired repo (`sturn.toml` present): `sturn claim "<glob>"
  --ws <ws>` *before* edits — claims surface conflicts in ms.
- Ops via `sturn write|rename|delete`; `sturn snap` folds the ws
  into a real commit; `sturn push` publishes heads + refs/sturn/*.
- `sturn leave` on exit — drops the actor's claims.

<!-- /sturn:agent-awareness -->
