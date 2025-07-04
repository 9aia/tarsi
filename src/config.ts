export const CONFIG = {
  model: 'gemini-2.5-flash',
  reviewPath: 'pm/CHANGELOG.review.md',
  changelogPath: 'pm/CHANGELOG.md',
  changelogSliceEnd: 2,
  changelogStyleguide: `
    # Guiding Principles

    - Changelogs are for humans, not machines.
    - There should be an entry for every single version.
    - The same types of changes should be grouped.
    - Versions and sections should be linkable.
    - The latest version comes first.
    - The release date of each version is displayed.
    - Mention whether you follow Semantic Versioning.

    # Types of changes

    - **Added** for new features.
    - **Changed** for changes in existing functionality.
    - **Enhanced** for enhancements and improvements of existing functionality.
    - **Deprecated** for soon-to-be removed features.
    - **Beautified** for changes that are just aesthetic changes, such as colors, fonts, spacing, animations, etc.
    - **Removed** for now removed features.
    - **Fixed** for any bug fixes.
    - **Security** in case of vulnerabilities.
    - **Performance** for performance improvements, such as faster loading times, better memory usage, etc.
    - **Chore** for updating build tasks, package manager files, etc.
    - **Project Management** for tasks, backlogs, issues, etc.
    - **Test** for adding missing tests.
    - **Other** for other changes that don't fit into the other categories.
  `,
  gitDiffPatterns: [
    // Generated files
    '!worker-configuration.d.ts',
    '!pnpm-lock.yaml',

    '!db-client/*',
    '!api-client/*',
    '!psitta/*',
    '!locales/script.ts',
    '!psitta.config.ts',
    '!pm/*',
    '!co-authors*',
    '!codeshare/*',
    '!tarsi/*',

    '!docs/*',
    '!README.md',
    '!LICENSE',
    '!SECURITY.md',
  ],
}
