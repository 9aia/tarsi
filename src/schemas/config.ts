import * as v from "valibot"

export const configSchema = v.object({
  model: v.pipe(
    v.string(),
    v.minLength(1, "model must be a non-empty string"),
    v.title("Model"),
    v.description("AI model to use for content generation (Gemini model name or custom model)"),
  ),
  reviewPath: v.pipe(
    v.string(),
    v.minLength(1, "reviewPath must be a non-empty string"),
    v.title("Review Path"),
    v.description("File path where generated reviews will be saved"),
  ),
  changelogPath: v.pipe(
    v.string(),
    v.minLength(1, "changelogPath must be a non-empty string"),
    v.title("Changelog Path"),
    v.description("File path to the changelog file"),
  ),
  changelogSliceEnd: v.pipe(
    v.number(),
    v.integer("changelogSliceEnd must be an integer"),
    v.minValue(0, "changelogSliceEnd must be a non-negative number"),
    v.title("Changelog Slice End"),
    v.description("Number of entries to slice from the end of the changelog"),
  ),
  changelogStyleguide: v.pipe(
    v.string(),
    v.minLength(1, "changelogStyleguide must be a non-empty string"),
    v.title("Changelog Styleguide"),
    v.description("Style guide text for changelog formatting and guidelines"),
  ),
  gitDiffPatterns: v.pipe(
    v.array(v.string()),
    v.minLength(0, "gitDiffPatterns must be an array"),
    v.title("Git Diff Patterns"),
    v.description("Array of git diff patterns to exclude from analysis"),
  ),
})
export type Config = v.InferOutput<typeof configSchema>
