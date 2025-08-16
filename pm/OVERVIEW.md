---
description: A high-level summary of the project.
---

# Project Overview

## Description

Tarsi is an AI-powered CLI tool designed to streamline the process of creating user-friendly changelogs for software projects. By analyzing recent git changes and referencing previous changelog entries, Tarsi leverages advanced language models to automatically draft new "Unreleased" sections in a clear, user-focused style. This tool helps teams maintain consistent, high-quality changelogs with minimal manual effort, ensuring that end users are always informed about the latest updates and improvements.

## Goals

* **Automate Changelog Generation**: Eliminate the manual effort required to write changelogs by automatically analyzing git diffs and generating user-focused release notes.
* **Maintain Consistency**: Ensure changelogs follow a consistent style and format across all releases using predefined styleguides and AI-powered content generation.
* **User-Focused Communication**: Transform technical git changes into clear, understandable descriptions that focus on user-facing improvements rather than internal implementation details.
* **Quality Assurance**: Provide review capabilities to validate generated changelogs against actual code changes, ensuring accuracy and completeness.

## Scope

Tarsi focuses on a specific set of changelog automation features to provide a streamlined and effective solution:

* **AI-Powered Generation**: Automatically generate changelog entries by analyzing git diffs using Google's Gemini AI model.
* **Git Integration**: Seamlessly integrate with git repositories to track changes since the last version tag.
* **Intelligent Filtering**: Exclude irrelevant files (documentation, lock files, generated content) from changelog consideration using configurable patterns.
* **Style Consistency**: Follow predefined styleguide rules for changelog formatting and categorization (Added, Changed, Fixed, etc.).
* **Review and Validation**: Check generated changelogs against git diffs to identify missing or incorrect entries.
* **Flexible Configuration**: Support customizable paths, models, and filtering patterns through configuration files.
* **Command-Line Tool**: Provided as a user-friendly CLI, enabling seamless changelog generation and management directly from the terminal.

**The project does not include:**

* **Multi-Language AI Models**: Currently only supports Google's Gemini AI models and does not integrate with other AI providers.
* **Complex Git Workflows**: Does not handle complex branching strategies, merge conflicts, or multi-repository scenarios.
* **Real-time Changelog Updates**: Works with tagged releases and does not provide continuous changelog updates during development.
* **Visual Interface**: Command-line only tool without a graphical user interface or web dashboard.
* **Automated Release Publishing**: Focuses solely on changelog generation and does not handle version bumping or release publishing.

## Requirements

To effectively use Tarsi, users will need:

* **Git Repository**: Must be run within a git repository with proper version tagging using semantic versioning.
* **Google Gemini API Access**: Requires a valid `TARSI_GEMINI_API_KEY` environment variable for AI-powered content generation.
* **Existing Changelog Structure**: Works best with an existing changelog file following the expected format and location (`pm/CHANGELOG.md`).
* **Development Workflow**: Integrates into development workflows where regular version tagging and release processes are established.
