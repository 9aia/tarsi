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
      { text: "Getting Started", link: "/getting-started" },
      { text: "Tutorial", link: "/tutorials/first-release" },
      { text: "How-to", link: "/how-to/write-tarsi-toml" },
      { text: "Explanation", link: "/explanation/release-semantics" },
      { text: "Reference", link: "/reference/ops" },
      {
        text: "LLMs",
        items: [
          { text: "llms.txt", link: "https://9aia.com/tarsi/llms.txt", target: "_blank" },
          { text: "llms-full.txt", link: "https://9aia.com/tarsi/llms-full.txt", target: "_blank" },
        ],
      },
      { text: "9aia", link: "https://9aia.com" },
    ],
    sidebar: [
      {
        text: "Tutorials",
        items: [{ text: "Your first release", link: "/tutorials/first-release" }],
      },
      {
        text: "How-to",
        items: [
          { text: "Write a tarsi.toml", link: "/how-to/write-tarsi-toml" },
          { text: "Author a release profile pack", link: "/how-to/author-release-profile" },
          { text: "Drive tarsi-serve ops", link: "/how-to/use-serve-ops" },
        ],
      },
      {
        text: "Explanation",
        items: [
          { text: "Release semantics", link: "/explanation/release-semantics" },
          { text: "Boundary with tamias and peregrin", link: "/explanation/boundary" },
        ],
      },
      {
        text: "Reference",
        items: [
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
