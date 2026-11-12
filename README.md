# Tarsi

> Generic release tool for the 9aia fleet — changelog → semver bump
> → tag → publish, as a tamias content type and peregrin campaign.
>
> **Status: being rewritten in Rust** (ports-and-adapters, same
> anatomy as tamias/bupha/peregrin). The TypeScript implementation
> below is the legacy changelog-only version, kept for reference
> until the rewrite lands — see the org ledger proposal P-013 /
> plan PL-014.

## Distribution (Rust rewrite)

The command ships as a tamias pack — `libraries/tarsi/tamias.yaml`
declares `tarsi-serve` as a `tools/bin/<triple>/` payload bin (T-014;
there is no `tarsi` CLI — the surface is `tarsi-serve`'s ops).

```bash
cargo xtask pack    # release-build → libraries/tarsi/tools/bin/<triple>/
tamias install ./libraries/tarsi   # deploys `tarsi-serve` onto PATH
```

`tarsi-serve` has two modes: no args → ndjson ops loop on stdio;
`<op> [--dir D] [--set k=v]… [--yes|--dry-run]` → one-shot JSON
(`manifest`/`plan`/`changelog`/`notes`/`release`). A repo's
`tarsi.toml` configures units and publish targets; peregrin's
`libraries/release` bundle drives `tarsi-serve release --yes`.

Dev shortcut without tamias: `cargo xtask install` puts a release
`tarsi-serve` in `~/.local/bin`.

## Legacy: AI changelog CLI (TypeScript)

Tarsi is an AI-powered CLI that automatically generates clear, user-focused changelogs from recent git changes, minimizing manual effort and keeping users informed.

## Installing

To install the CLI, use NPM:

```bash
npm install -g tarsi
```

Or with other package managers like PNPM, Yarn, or Bun!

### Adding the Script to your `package.json` (Recommended)

Add the `changelog` script to your `package.json`:

```jsonc
// package.json
{
  // ...
  "scripts": {
    "changelog": "tarsi"
  }
  // ...
}
```

## Usage

To generate a changelog from the git diff since the latest tag, run:

```bash
tarsi generate
```

To check the changelog against the git diff, run:

```bash
tarsi check
```

## Configuration

You can customize Tarsi's behavior by creating a `tarsi.config.ts` file in your project root. This file should export a `CONFIG` object with your custom settings.

### Example Configuration

```typescript
// tarsi.config.ts
export const CONFIG = {
  model: "gemini-2.5-flash",
  reviewPath: "custom-review.md",
  changelogPath: "CHANGELOG.md",
  changelogSliceEnd: 2,
  changelogStyleguide: `
    # Custom Style Guide
    - Use present tense for new features
    - Group changes by type
    - Include relevant PR numbers
  `,
  gitDiffPatterns: [
    "!dist/*",
    "!build/*",
    "!node_modules/*",
  ],
}
```

### Configuration Options

- **model**: AI model to use for content generation (default: "gemini-2.5-flash")
- **reviewPath**: File path where generated reviews will be saved
- **changelogPath**: File path to the changelog file
- **changelogSliceEnd**: Number of entries to slice from the end of the changelog
- **changelogStyleguide**: Style guide text for changelog formatting
- **gitDiffPatterns**: Array of git diff patterns to exclude from analysis

If no configuration file is found, Tarsi will use sensible defaults.

---

[Contribute](./CONTRIBUTING.md) • [Security](./SECURITY.md) • [License](./LICENSE)
