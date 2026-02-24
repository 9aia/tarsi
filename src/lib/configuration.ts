import type { Config } from "../schemas/config"
import { existsSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"
import process from "node:process"
import * as v from "valibot"
import { configSchema } from "../schemas/config"

const require = createRequire(import.meta.url)

/**
 * Type definition for configuration module import
 */
interface ConfigModule { CONFIG: unknown }

/**
 * Sets up configuration by merging default config with user-defined config
 * @description Loads user configuration from tarsi.config.ts if it exists, otherwise returns default config
 * @param defaultConfig - Default configuration object
 * @returns Merged configuration object
 * @throws {Error} If configuration file exists but cannot be required, lacks CONFIG object, or validation fails
 */
function setupConfig(defaultConfig: Config): Config {
  const validatedDefaultConfig = v.parse(configSchema, defaultConfig)

  // Resolve config path dynamically to support testing with different working directories
  // Try .ts first, then .cjs as fallback for compatibility (since package.json has "type": "module")
  const tsConfigPath = path.resolve(process.cwd(), "./tarsi.config.ts")
  const cjsConfigPath = path.resolve(process.cwd(), "./tarsi.config.cjs")
  const configFilePath = existsSync(tsConfigPath) ? tsConfigPath : cjsConfigPath

  if (!existsSync(configFilePath)) {
    return validatedDefaultConfig
  }

  const module: ConfigModule = require(configFilePath)

  const validatedModuleConfig = v.parse(v.partial(configSchema), module.CONFIG)
  const mergedConfig = { ...defaultConfig, ...validatedModuleConfig }

  const finalResult = v.parse(configSchema, mergedConfig)
  return finalResult
}

export { setupConfig }
