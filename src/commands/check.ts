import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import dedent from 'dedent'
import { CONFIG } from '../config'
import { getGitDiffSince, getLastTag, readChangelog } from '../lib'
import { generateContent } from '../lib/gemini'

async function check() {
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

  // Check if generated changelog exists
  console.log('\n📖 Step 3: Checking for the current changelog...')
  const changelogFile = CONFIG.changelogPath
  if (!existsSync(changelogFile)) {
    console.log(`   ❌ Changelog not found: ${changelogFile}`)
    console.log('   💡 Run "tarsi generate" first to create a changelog')
    return
  }
  console.log(`   ✅ Found changelog: ${changelogFile}`)

  // Read existing changelog
  console.log('\n📚 Step 4: Reading existing changelog...')
  const rawChangelog = await readChangelog()
  console.log(`   Existing changelog file: ${CONFIG.changelogPath}`)
  console.log(`   Existing changelog size: ${rawChangelog.length} characters`)

  // Preprocess changelog
  console.log('\n🧹 Step 5: Preprocessing changelog...')
  let content = rawChangelog.replace(/^---[\s\S]*?---/, '').trim()
  // Clean h1
  content = content.replace(/^# Changelog\s*/m, '').trim()
  // Find all h2 (version headers)
  const allVersions = content.split(/^## /gm).slice(1)
  const versions = allVersions.slice(1)
  console.log(`   Found ${versions.length} version sections`)
  const unreleased = allVersions[0]
  const unreleasedSection = `## ${unreleased}`
  console.log(`   Unreleased section size: ${unreleasedSection.length} characters`)

  // Prepare AI check prompt
  console.log('\n🤖 Step 6: Preparing AI check prompt...')
  const checkPrompt = dedent`
    You are a changelog reviewer. Analyze the generated changelog against the git diff and provide feedback.

    Please provide a detailed review with the following structure:

    2. **Missing Items**: List any changes from the git diff that should be in the changelog but are missing
    3. **Incorrect Items**: List any items in the changelog that don't match the git diff

    Be specific and reference specific files/changes when possible.

    --- Changelog styleguide ---
    ${CONFIG.changelogStyleguide}

    --- Existing changelog ---
    ${versions.join('\n')}

    --- Unreleased section to review ---
    ${unreleasedSection}

    --- Git diff from ${tag} to HEAD ---

  `

  console.log(`   Check prompt size: ${checkPrompt.length} characters`)
  console.log(`   Using model: ${CONFIG.model}`)

  // Generate review with AI
  console.log('\n🔍 Step 7: Generating changelog review...')
  console.log('   This may take a moment...')

  const review = await generateContent({
    prompt: checkPrompt,
    model: CONFIG.model,
  })

  console.log(`   Review size: ${review?.length || 0} characters`)

  // Save review to file
  console.log('\n💾 Step 8: Saving review...')
  const reviewFile = CONFIG.reviewPath
  const reviewContent = dedent`
    # Changelog Review
    
    Generated on: ${new Date().toISOString()}
    Last tag: ${tag}
    Diff lines: ${diffLines}
    Model: ${CONFIG.model}
    
    ---
    
    ${review}
  `

  writeFileSync(reviewFile, reviewContent)
  console.log(`   Review saved to: ${reviewFile}`)

  // Display review summary
  console.log('\n📋 Review Summary:')
  console.log('================================')
  console.log(review)

  console.log('\n🎉 Check complete!')
  console.log('================================')
  console.log(`📄 Review saved to: ${reviewFile}`)
  console.log(`📊 Diff lines analyzed: ${diffLines}`)
  console.log(`🤖 AI model used: ${CONFIG.model}`)
}

export default check
