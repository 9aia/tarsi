import { defineConfig } from "vitepress";
import llmstxt from "vitepress-plugin-llms";

export default defineConfig({
  vite: {
    plugins: [llmstxt()],
  },
  title: "tarsi",
  description:
    "the org release plane — changelog → semver bump → tag → publish, as ndjson ops and tamias release profiles",
  base: "/tarsi/",
  ignoreDeadLinks: [/\/api\//],
  themeConfig: {
    nav: [
      { text: "9aia", link: "https://9aia.com", target: "_blank" },
      { text: "Tutorials", link: "/tutorials/getting-started" },
      { text: "How-to", link: "/how-to/" },
      { text: "Explanation", link: "/explanation/" },
      { text: "Reference", link: "/reference/ops" },
      {
        text: "LLMs",
        items: [
          { text: "llms.txt", link: "https://9aia.com/tarsi/llms.txt", target: "_blank" },
          { text: "llms-full.txt", link: "https://9aia.com/tarsi/llms-full.txt", target: "_blank" },
        ],
      },
    ],
    sidebar: [
      {
        text: "Tutorials",
        items: [{ text: "Getting started", link: "/tutorials/getting-started" }],
      },
      {
        text: "How-to",
        items: [
          { text: "Overview", link: "/how-to/" },
          { text: "Write a tarsi.toml", link: "/how-to/write-tarsi-toml" },
          { text: "Author a release profile pack", link: "/how-to/author-release-profile" },
          { text: "Drive tarsi-serve ops", link: "/how-to/use-serve-ops" },
        ],
      },
      {
        text: "Explanation",
        items: [
          { text: "Overview", link: "/explanation/" },
          { text: "Architecture", link: "/explanation/architecture" },
          { text: "Release semantics", link: "/explanation/release-semantics" },
          { text: "Concepts", link: "/explanation/concepts" },
          { text: "Boundaries", link: "/explanation/boundaries" },
          { text: "Decisions", link: "/explanation/decisions" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Overview", link: "/reference/" },
          { text: "Ops wire surface", link: "/reference/ops" },
          { text: "tarsi.toml schema", link: "/reference/config" },
          { text: "Pack layout", link: "/reference/pack-layout" },
          { text: "Crates", link: "/reference/crates" },
        ],
      },
    ],
    socialLinks: [{ icon: "github", link: "https://github.com/9aia/tarsi" }],
    footer: {
      message: "MIT licensed — part of the 9aia GAIA stack.",
      copyright: "9aia",
    },
  },
});
