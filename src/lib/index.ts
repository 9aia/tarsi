import { readFile } from 'node:fs/promises'
import { simpleGit } from 'simple-git'
import { CONFIG } from '../config'
import semver from 'semver'

const git = simpleGit()

export async function getLastTag(): Promise<string> {
  const tags = (await git.tags()).all
  const sortedVersions = semver.rsort(tags)
  const latestVersion = sortedVersions[0]
  return latestVersion || ''
}

export async function getGitDiffSince(tag: string): Promise<string> {
  // Build exclude patterns for git diff
  const excludePatterns = CONFIG.gitDiffPatterns.flatMap(pattern => ['--', `:${pattern}`])

  console.log(`🔍 Excluding files: ${CONFIG.gitDiffPatterns.join(', ')}`)

  // Run git diff from latest tag to HEAD
  const diff = await git.diff([
    `${tag}..HEAD`,
    '--',
    '.', // include all files
    ...excludePatterns, // exclude patterns
  ])

  return diff
}

export async function getExcludedFiles(tag: string): Promise<string[]> {
  // Get list of files that would be excluded
  const excludeArgs = CONFIG.gitDiffPatterns.flatMap(pattern => ['--', `:(exclude)${pattern}`])
  const allFiles = await git.diff([tag, '--name-only'])
  const excludedFiles = await git.diff([tag, '--name-only', ...excludeArgs])

  // Files that are in allFiles but not in excludedFiles are the ones being excluded
  const allFileList = allFiles.split('\n').filter(Boolean)
  const excludedFileList = excludedFiles.split('\n').filter(Boolean)

  return allFileList.filter(file => !excludedFileList.includes(file))
}

export async function readChangelog(): Promise<string> {
  return await readFile(CONFIG.changelogPath, 'utf-8')
}
