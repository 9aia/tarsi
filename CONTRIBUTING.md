# Contributing to Tarsi <!-- omit in toc -->

Thank you for your interest in contributing to Tarsi! This document provides guidelines and information for contributors.

- [Managing the Project](#managing-the-project)
- [Preparing the Environment](#preparing-the-environment)
  - [Preparing the Basic Environment](#preparing-the-basic-environment)
  - [Preparing the Local Repository](#preparing-the-local-repository)
  - [Preparing the Full Environment Locally](#preparing-the-full-environment-locally)
- [Contributing](#contributing)
  - [Committing](#committing)
  - [Pushing to the Repositories](#pushing-to-the-repositories)
  - [Creating Pull Requests](#creating-pull-requests)
  - [Submitting a Pull Request](#submitting-a-pull-request)
- [Developing](#developing)
  - [Running in Development Mode](#running-in-development-mode)
  - [Writing Code](#writing-code)
  - [Checking Code Quality](#checking-code-quality)
  - [Previewing Builds](#previewing-builds)
  - [Getting AI Assistance](#getting-ai-assistance)
  - [Delegating Tasks to AI Software Engineers](#delegating-tasks-to-ai-software-engineers)
- [Managing the Content](#managing-the-content)
- [Streamlining Operations](#streamlining-operations)
  - [Releasing](#releasing)
- [Testing and Assuring Quality](#testing-and-assuring-quality)
- [Getting Help](#getting-help)
  - [Getting Asynchronous Help](#getting-asynchronous-help)
  - [Getting Real-Time Help](#getting-real-time-help)
- [License](#license)

## Managing the Project

> [!TIP]
> You can edit the project management documents directly in the `./pm` folder via GitHub website, but it's recommended to edit the documents locally.

We approach the project management in a git-based way. We foster an intuitive, transparent and proactivity-driven process. You can find all the project management documents in the `./pm` folder.

Here's a brief overview of the documents:

- **[Project Overview](/pm/OVERVIEW.md)**: A summary of Tarsi's mission, vision, and progress.
- **[Strategy](/pm/STRATEGY.md)**: The key strategies for developing, releasing, and growing the project.
- **[Backlog](/pm/BACKLOG.md)**: A collection of tasks and ideas pending implementation.
- **[Todo](/pm/TODO.md)**: The current tasks we’re working on.
- **[Suggestions](/pm/SUGGESTIONS.md)**: Concepts and ideas under review for potential inclusion.
- **[Changelog](/pm/CHANGELOG.md)**: A detailed chronological record of updates, changes, and improvements.

## Preparing the Environment

### Preparing the Basic Environment

Install the following tools:

- [Git](https://git-scm.com/downloads)
- [Bun 1.2.13 or higher](https://bun.sh/docs/installation)
- ([Vscode](https://code.visualstudio.com/download) or any Vscode compatible editor) (recommended)

### Preparing the Local Repository

> [!IMPORTANT]
> Make sure you have installed the basic environment locally. See [Preparing the Basic Environment](#preparing-the-basic-environment).

1. **Clone the repository**

   Clone the repository to your local machine:

   ```sh
   git clone https://github.com/9aia/tarsi.git
   cd tarsi/
   ```

### Preparing the Full Environment Locally

> [!IMPORTANT]
> Make sure you have installed the basic environment locally. See [Preparing the Basic Environment Locally](#preparing-the-basic-environment-locally).

1. **Install the main dependencies**
   - Ensure you have the following prerequisites installed on your system:
      - [Docker](https://docs.docker.com/)
      - [Docker Compose](https://docs.docker.com/compose)

2. **Configure the IDE (recommended)**
  - For Visual Studio Code, consider installing the following extensions:
    - [Eslint](https://marketplace.visualstudio.com/items?itemName=dbaeumer.vscode-eslint)
    - [Markdown All in One](https://marketplace.visualstudio.com/items?itemName=yzhang.markdown-all-in-one)
    - [GitLens](https://marketplace.visualstudio.com/items?itemName=eamodio.gitlens) (optional)
    - [Copilot](https://marketplace.visualstudio.com/items?itemName=GitHub.copilot) (optional if you are using Cursor or another AI code assistant)

3. **Clone the repository**. See [Preparing the Local Repository](#preparing-the-local-repository) for more details.

4. **Install the dependencies**

   Make sure to install the dependencies:

   ```bash
   bun install
   ```

## Contributing

### Committing

1. **Generate a commit message**
   - Use the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) format, in particular the [Angular Commit Convention](https://github.com/angular/angular/blob/main/CONTRIBUTING.md#commit).
   - It's recommended to generate a commit message using AI of the IDE you are using. For example, in VSCode, you can use the `Copilot` extension to generate a commit message.
   - Check for warnings of already included authors in the commit message.

2. **Commit**
   - Run `git commit` to commit your changes or use the IDE's commit button.

### Pushing to the Repositories

You can use the following command to push your changes to the repository:

```bash
git push --set-upstream origin main # or `git push` if you have already set up upstream
```

### Creating Pull Requests

- Provide a clear description of the changes
- Include any relevant issue numbers
- Ensure all tests pass and code quality checks are satisfied
- Update documentation if necessary

### Submitting a Pull Request

1. **Create a branch**:
   ```bash
   git checkout -b feat/your-feature-name # or fix/your-fix-name, chore/your-chore-name, docs/your-docs-name, etc.
   ```

2. **Make your changes** following the guidelines

3. **Test your changes** by running the application. See [Previewing Builds](#previewing-builds) for more information.

4. **Check code quality (if applicable)**:
   ```bash
   # Format code
   bun run lint

   # Run type checker
   bun run typecheck
   ```

   See [Checking Code Quality](#checking-code-quality) for more information.

5. **Commit your changes**. See [Committing](#committing) for more information.

6. **Push and create a pull request**. See [Pushing to the Repositories](#pushing-to-the-repositories) and [Creating Pull Requests](#creating-pull-requests) for more information.

## Developing

### Running in Development Mode

1. **Run the CLI application:**
     ```bash
     bun start
     bun dev # watch mode
     ```

### Writing Code

- Use meaningful variable and function names
- Avoid redudant comments with the code itself
- Keep functions small and focused
- Use types

### Checking Code Quality

For keeping the code organized, we use the following tools:

- [Eslint](https://eslint.org/): Used to help us with the code formatting and linting. It automatically fix the linting issues when you save the files. You can also run it manually with `bun run lint` to check the issues or `bun run lint:fix` to fix the issues.
- [Typescript](https://www.typescriptlang.org/): Used to help us with the type safety. It's automatically run before the release. You can also run it manually with `bun run typecheck`.

### Previewing Builds

Preview it locally, run:

```bash
bun run build && ./dist/tarsi
```

### Getting AI Assistance

We can use AI code tools to get assistance for generating code, refactoring, debugging, testing, and documentation.

Examples: [Copilot](https://copilot.github.com/) (IDE extension), [Cursor](https://www.cursor.com/) (IDE fork), [Gemini CLI](https://blog.google/technology/developers/introducing-gemini-cli-open-source-ai-agent/) (terminal tool)

### Delegating Tasks to AI Software Engineers

We can use AI software engineers to delegate tasks across the project to an autonomous agent in the cloud. These tools can handle complex, multi-step tasks and integrate with your workflow.

Examples: [Devin](https://devin.ai/), [Codex](https://openai.com/codex/)

## Managing the Content

// TODO: Add managing the content documentation

## Streamlining Operations

### Releasing

> [!WARNING]
> Before releasing, it's recommended to preview locally the application and to test it to ensure everything works as expected.

1. **Update the Changelog**
   - Manually update the changelog sections following the standard described here: [Keep a Changelog](https://www.npmjs.com/package/@release-it/keep-a-changelog).
   - Do **not** specify the next version in the changelog. Leave it as `## [Unreleased]`, which will be automatically updated with the version and release date. This also applies to the README file.
   - For more details, refer to the [Conventional Changelog documentation](https://github.com/release-it/conventional-changelog).

2. **Run the Release Command**
   After following the steps above, run the following command to release the update:

   ```bash
   bun run release
   ```

3. **Select the Next Version**
   Choose the next version based on [semantic versioning](https://semver.org/). Make sure to determine whether it is a major, minor, or patch release based on the changes since the last release.

## Testing and Assuring Quality

// TODO: Add testing and assuring quality documentation

## Getting Help

> [!WARNING]
> For human-human communication, please prioritize asynchronous communication over real-time communication when possible.

### Getting Asynchronous Help

If you have questions or need help, we recommend you to follow these steps in order:

1. Read through this contributing guide.
2. Check the existing documentation in the `docs/` folder.
3. Refer to the official documentation for any relevant technologies.
4. Examine existing code, issues, and discussions for similar problems.
5. If you still need help, open a new issue for bugs or feature requests, or use a discussion for general questions.

### Getting Real-Time Help

For immediate assistance, we encourage you to use both community support and AI.

* **Community**: We have a [Discord server](https://discord.gg/RsYaUn3zQa) where you can get real-time help from our community members and core team if they are available.
* **AI Assistants:** We encourage you to leverage general-purpose AI Assistant, such as [Gemini](https://gemini.google.com/) or [ChatGPT](https://chatgpt.com/). They provide high-level, conversational help, such as explaining complex concepts, brainstorming solutions, and more.

## License

By contributing to this project, you agree that your contributions will be licensed under the same license as the project.

---

Thank you for contributing to Tarsi!
