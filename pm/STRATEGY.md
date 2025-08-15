---
description: The key strategies for developing, releasing, and growing the project.
---

- [Development Strategy](#development-strategy)
  - [Core Principles](#core-principles)
  - [Integration Principles](#integration-principles)
  - [Development Style](#development-style)
  - [What We Will Do](#what-we-will-do)
  - [What We Won't Promise](#what-we-wont-promise)
- [Release Strategy](#release-strategy)
  - [Versioning](#versioning)
  - [Release Phases](#release-phases)
    - [Initial Development Releases (`v0.x.x`)](#initial-development-releases-v0xx)
    - [Stable-ish Releases (`v1.x.x`)](#stable-ish-releases-v1xx)
- [Distribution Strategy](#distribution-strategy)
- [Growth Strategy](#growth-strategy)

# Strategy <!-- omit in toc -->

## Development Strategy

### Core Principles

- **Build What We Need**: Primarily developed for Gaia's internal use, shared because it might help others
- **Keep It Simple**: Focus on core functionality rather than feature bloat
- **Learn by Doing**: Experiment with AI-powered tooling and improve as we go
- **Community Welcome**: Open to contributions but no pressure to maintain enterprise-grade support

### Integration Principles

- **Works With What We Use**: Focus on tools and workflows Gaia actually uses
- **Simple Integrations**: Add integration guides when someone asks or we need them
- **No Partnership Pressure**: Collaborate when it makes sense, not for growth metrics

### Development Style

- **Iterative Improvement**: Add features when we actually need them
- **Real-world Testing**: Use it in our own projects to validate functionality
- **Flexible Architecture**: Keep the codebase adaptable for future needs
- **Documentation When Needed**: Write docs when people start asking questions

### What We Will Do

- Maintain functionality that we actively use
- Fix bugs that affect real workflows
- Add features when we actually need them
- Keep the codebase clean and understandable

### What We Won't Promise

- Enterprise-grade support or SLAs
- Feature requests that don't align with our needs
- Compatibility with every possible workflow
- Long-term roadmaps or feature guarantees

## Release Strategy

### Versioning

Follows [Semantic Versioning](https://semver.org/).

### Release Phases

#### Initial Development Releases (`v0.x.x`)

- **Template**: `v0.MINOR.PATCH` (below `v1.0.0`)
- **Example**: `v0.1.0`, `v0.2.3`
- **Distribution**: GitHub Releases, NPM
- **Audience**: Ourselves and anyone curious enough to try it
- **Purpose**:
  - Get basic functionality working
  - Experiment with different approaches
  - Breaking changes are normal and expected
  - Learning how AI changelog generation works in practice
- **Release Frequency**: When we have something worth sharing
- **Documentation**: Basic usage in README, probably some rough edges

#### Stable-ish Releases (`v1.x.x`)

- **Template**: `v1.MINOR.PATCH` (starting from `v1.0.0`)
- **Example**: `v1.0.0`, `v1.1.0`, `v1.0.1`
- **Distribution**: GitHub Releases, NPM
- **Audience**: People who want to use it in their projects
- **Purpose**:
  - Works reliably for our use cases
  - Handles most common scenarios
  - Good enough for daily use
- **Release Frequency**: When we fix bugs or add features we need
- **Documentation**: Clear usage instructions and examples

## Distribution Strategy

* **Primary**: NPM (`tarsi`), global CLI install supported.
* **CI/CD**: `bun run release` → lint, type check, changelog (Tarsi), build, publish. Fail if checks fail.
* **Docs**: README quickstart, CLI help, examples, on-demand API docs.
* **Support**: Best-effort fixes, PRs welcome, no SLAs.

## Growth Strategy

* **Organic**: Use daily, share learnings, welcome contributors.
* **Sustainable**: Maintain while useful; community can take over if it grows.
* **No Monetization**: Remains free/open-source.
* **Fun-First**: If it’s no longer useful/interesting, move on.
