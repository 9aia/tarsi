# Tarsi

> [!WARNING]
> We are in the early stages of development and this is a work in progress. Please be aware of possible bugs, incomplete or not yet implemented features. We welcome feedback and contributions.

Tarsi is an AI-powered CLI that automatically generates clear, user-focused changelogs from recent git changes, minimizing manual effort and keeping users informed.

## Installing

To install the CLI, use NPM:

```bash
npm install -g tarsi
```

Or with other package managers like PNPM, Yarn, or Bun!

## Usage

To generate a changelog from the git diff since the latest tag, run:

```bash
tarsi generate
```

To check the changelog against the git diff, run:

```bash
tarsi check
```

---

[Contribute](./CONTRIBUTING.md) • [Security](./SECURITY.md) • [License](./LICENSE)
