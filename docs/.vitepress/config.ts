import { defineConfig } from "vitepress";

export default defineConfig({
  title: "plzplz",
  description: "A simple cross-platform task runner with helpful defaults",
  themeConfig: {
    nav: [
      { text: "Getting Started", link: "/getting-started" },
      { text: "Reference", link: "/reference" },
    ],
    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Getting Started", link: "/getting-started" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Config & CLI", link: "/reference" },
        ],
      },
    ],
    socialLinks: [
      { icon: "github", link: "https://github.com/k88hudson/plzplz" },
    ],
  },
});
