import mdx from "@mdx-js/rollup"
import { tanstackRouter } from "@tanstack/router-plugin/vite"
import react from "@vitejs/plugin-react"
import { recmaCodeHike, remarkCodeHike, type CodeHikeConfig } from "codehike/mdx"
import { defineConfig, loadEnv } from "vite"

const codeHike: CodeHikeConfig = {
  components: { code: "CodePanel" },
  syntaxHighlighting: { theme: "github-dark" },
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "")
  return {
    base: env.SLIDES_BASE_PATH || "/",
    optimizeDeps: { entries: ["index.html"] },
    plugins: [
      tanstackRouter({ target: "react", routesDirectory: "src/pages" }),
      mdx({
        remarkPlugins: [[remarkCodeHike, codeHike]],
        recmaPlugins: [[recmaCodeHike, codeHike]],
      }),
      react(),
    ],
  }
})
