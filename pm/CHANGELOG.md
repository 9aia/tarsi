---
description: A detailed, chronological record of all changes made to a project, often including technical information.
---

# Changelog

## [Unreleased]

### Added

- Integrated Google Gemini AI (gemini-2.5-flash) to automatically generate user-focused changelogs from git changes
- Added `tarsi generate` command to analyze git diff from latest tag to HEAD, processes through AI with style guide, and saves to `CHANGELOG.tmp.md`
- Added `tarsi check` command to review existing changelog against git diff, identifies discrepancies, and saves analysis to `pm/CHANGELOG.review.md`
- Implemented automatic exclusion of irrelevant files (generated code, docs, PM files) from diff analysis
- Designed a comprehensive style guide with 12 categorized change types and human-focused writing principles
- Integrated semantic version-aware tag detection, smart diff generation, and repository analysis
- Created an intuitive CLI interface with validation, error handling, progress indicators, and detailed execution summaries
- Enabled secure API authentication using the `TARSI_GEMINI_API_KEY` environment variable
