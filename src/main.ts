import process from "node:process"
import check from "./commands/check"
import generate from "./commands/generate"

// TODO: add genesis generation
// TODO: add security command for security analysis

async function main() {
  const args = process.argv.slice(2)
  const command = args[0]

  if (!command) {
    console.log("🐵 Tarsi")
    console.log("")
    console.log("Usage: tarsi <command>")
    console.log("")
    console.log("Commands:")
    console.log("  generate     📦 Generate changelog from git diff")
    console.log("  check        🔍 Review changelog against git diff")
    console.log("")
    process.exit(1)
  }

  switch (command) {
    case "generate":
      await generate()
      break
    case "check":
      await check()
      break
    default:
      console.error(`❌ Unknown command: ${command}`)
      console.log("")
      console.log("Available commands:")
      console.log("  generate     📦 Generate changelog from git diff")
      console.log("  check        🔍 Review changelog against git diff")
      console.log("")
      process.exit(1)
  }
}

main().catch(console.error)
