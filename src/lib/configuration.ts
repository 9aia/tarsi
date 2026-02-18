import { existsSync } from "fs";
import path from "path";

const CONFIG_FILE_PATH = path.resolve(process.cwd(), "./tarsi.config.ts")

type GeminiModels =
    | "gemini-2.5-flash"

export interface TarsiConfig {
    model: GeminiModels | (string & {}),
    reviewPath: string,
    changelogPath: string,
    changelogSliceEnd: number,
    changelogStyleguide: string,
    gitDiffPatterns: string[],
}

type ConfigModule = { CONFIG: Partial<TarsiConfig> }

async function setupConfig(config: TarsiConfig) {
    if (!existsSync(CONFIG_FILE_PATH)) {
        return config
    }

    const module: ConfigModule = await import(CONFIG_FILE_PATH)
    const result: TarsiConfig = { ...config, ...module.CONFIG }

    return result
}

export { setupConfig }