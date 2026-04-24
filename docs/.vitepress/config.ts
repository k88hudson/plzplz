import { defineConfig } from "vitepress";

export default defineConfig({
  title: "plzplz",
  description: "A simple cross-platform task runner with helpful defaults",
  themeConfig: {
    outline: [2, 3],
    nav: [
      { text: "Getting Started", link: "/getting-started" },
      { text: "Reference", link: "/reference" },
    ],
    sidebar: [
      {
        text: "Guide",
        items: [
          {
            text: "Getting Started",
            link: "/getting-started",
            items: [
              { text: "Installation", link: "/getting-started#installation" },
              { text: "GitHub Actions", link: "/getting-started#github-actions" },
              { text: "Initialize a project", link: "/getting-started#initialize-a-project" },
              { text: "Run a task", link: "/getting-started#run-a-task" },
              { text: "Add tasks", link: "/getting-started#add-tasks" },
              { text: "Set up defaults", link: "/getting-started#set-up-defaults" },
              { text: "Git hooks", link: "/getting-started#git-hooks" },
              { text: "Variables", link: "/getting-started#variables" },
              { text: "Passing arguments", link: "/getting-started#passing-arguments" },
            ],
          },
        ],
      },
      {
        text: "Reference",
        items: [
          {
            text: "Config & CLI",
            link: "/reference",
            items: [
              { text: "CLI Commands", link: "/reference#cli-commands" },
              { text: "TOML Configuration", link: "/reference#toml-configuration" },
              { text: "Settings", link: "/reference#settings" },
              { text: "Healthcheck", link: "/reference#healthcheck" },
            ],
          },
        ],
      },
    ],
    socialLinks: [
      { icon: "github", link: "https://github.com/k88hudson/plzplz" },
    ],
  },
});
