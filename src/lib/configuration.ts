import { existsSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"
import process from "node:process"

const require = createRequire(import.meta.url)

/**
 * Path to the user configuration file
 */
const CONFIG_FILE_PATH = path.resolve(process.cwd(), "./tarsi.config.ts")

/**
 * Supported Gemini AI models for content generation
 */
type GeminiModels
    = | "gemini-2.5-flash"

/**
 * Configuration interface for Tarsi CLI tool
 */
interface TarsiConfig {
  /** AI model to use for content generation (Gemini model name or custom model) */
  model: GeminiModels | (string & {})
  /** File path where generated reviews will be saved */
  reviewPath: string
  /** File path to the changelog file */
  changelogPath: string
  /** Number of entries to slice from the end of the changelog */
  changelogSliceEnd: number
  /** Style guide text for changelog formatting and guidelines */
  changelogStyleguide: string
  /** Array of git diff patterns to exclude from analysis */
  gitDiffPatterns: string[]
}

/**
 * Type definition for configuration module import
 */
interface ConfigModule { CONFIG: Partial<TarsiConfig> }

/**
 * Sets up configuration by merging default config with user-defined config
 * @description Loads user configuration from tarsi.config.ts if it exists, otherwise returns default config
 * @param config - Default configuration object
 * @returns Merged configuration object
 * @throws {Error} If configuration file exists but cannot be required or lacks CONFIG object
 */
function setupConfig(config: TarsiConfig): TarsiConfig {
  if (!existsSync(CONFIG_FILE_PATH)) {
    return config
  }

  const module: ConfigModule = require(CONFIG_FILE_PATH)

  // Validate that the required module has a CONFIG object
  if (!module || typeof module !== "object" || !("CONFIG" in module)) {
    throw new Error(`Configuration file "${CONFIG_FILE_PATH}" must export a CONFIG object`)
  }

  // Validate that CONFIG is an object
  if (module.CONFIG === null || typeof module.CONFIG !== "object") {
    throw new Error(`CONFIG export in "${CONFIG_FILE_PATH}" must be an object`)
  }

  const result: TarsiConfig = { ...config, ...module.CONFIG }

  return result
}

export { setupConfig }
export type { TarsiConfig }
