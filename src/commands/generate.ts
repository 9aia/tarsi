import { writeFileSync } from 'node:fs'
import dedent from 'dedent'
import { CONFIG } from '../config'
import { getGitDiffSince, getLastTag, readChangelog } from '../lib'
import { generateContent } from '../lib/gemini'

async function generate() {
  console.log('🐵 Tarsi')
  console.log('================================')

  // Get last tag
  console.log('\n📋 Step 1: Getting last version tag...')
  const tag = await getLastTag()
  console.log(`   Last version tag: ${tag || 'No tags found'}`)

  if (!tag) {
    console.log('   ⚠️  No tags found. Using all commits since repository creation.')
  }

  // Get git diff
  console.log('\n📊 Step 2: Generating git diff...')
  const diff = await getGitDiffSince(tag)
  const diffLines = diff.split('\n').length
  console.log(`   Diff contains ${diffLines} lines`)

  if (diffLines === 0) {
    console.log('   ℹ️  No changes found since last tag')
    return
  }

  // Read changelog
  console.log('\n📖 Step 3: Reading existing changelog...')
  const rawChangelog = await readChangelog()
  console.log(`   Changelog file: ${CONFIG.changelogPath}`)
  console.log(`   Raw changelog size: ${rawChangelog.length} characters`)

  // Clean frontmatter
  console.log('\n🧹 Step 4: Processing changelog content...')
  let content = rawChangelog.replace(/^---[\s\S]*?---/, '').trim()

  // Clean h1
  content = content.replace(/^# Changelog\s*/m, '').trim()

  // Find all h2 (version headers)
  const allVersions = content.split(/^## /gm).slice(1)
  const versions = allVersions.slice(1)
  console.log(`   Found ${versions.length} version sections`)

  const relevantVersions = versions.slice(0, CONFIG.changelogSliceEnd)
  const versionsHeadMd = `## ${relevantVersions.join('## ')}`
  console.log(`   Using ${relevantVersions.length} recent versions for context`)

  // Generate AI prompt
  console.log('\n🤖 Step 5: Preparing AI prompt...')
  const unreleasedPrompt = dedent`
    You're an AI changelog writer. Given the previous changelog format and this git diff below, generate the new 'Unreleased' section based on this info, following the style of the previous changelog and the styleguide.

    You must generate the changelog for a user perspective, not a developer perspective. This changelog shouldn't include any technical details, only the user-facing changes.

    --- Changelog styleguide ---
    ${CONFIG.changelogStyleguide}

    --- Changelog Head ---
    ${versionsHeadMd}

    --- Git diff from ${tag} to HEAD ---
    ${diff}
  `

  console.log(`   Prompt size: ${unreleasedPrompt.length} characters`)
  console.log(`   Using model: ${CONFIG.model}`)

  // Generate content with AI
  console.log('\n🚀 Step 6: Generating changelog with AI...')
  console.log('   This may take a moment...')

  const unreleasedSection = await generateContent({
    prompt: unreleasedPrompt,
    model: CONFIG.model,
  })

  console.log(`   Generated content size: ${unreleasedSection?.length || 0} characters`)

  // Save to file
  console.log('\n💾 Step 7: Saving generated changelog...')
  const outputFile = 'CHANGELOG.tmp.md'
  writeFileSync(outputFile, unreleasedSection ?? '')
  console.log(`   Saved to: ${outputFile}`)

  console.log('\n🎉 Generation complete!')
  console.log('================================')
  console.log(`📄 Generated changelog saved to: ${outputFile}`)
  console.log(`📊 Diff lines processed: ${diffLines}`)
  console.log(`🤖 AI model used: ${CONFIG.model}`)
}

export default generate
