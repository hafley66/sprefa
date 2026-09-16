import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'
import { buildFrames } from './bin/build-frames.mjs'

const SRC = fileURLToPath(new URL('./src', import.meta.url))

// Author in src/frames.md; this plugin compiles it to src/frames.json (+ renders
// inline d2 graphs) on start and on every save, so the live reload just works.
function framesMd() {
  return {
    name: 'frames-md',
    buildStart() {
      try { buildFrames() } catch (e) { this.warn(String(e)) }
    },
    configureServer(server) {
      server.watcher.add(SRC)
      const rebuild = (file) => {
        const f = file.replace(/\\/g, '/')
        if (!/src\/(frames\.md|kit\.d2|deck\/.*\.md)$/.test(f)) return
        const r = buildFrames()
        server.config.logger.info(`  deck -> ${r.frames} frames, ${r.graphs} graphs`)
      }
      server.watcher.on('change', rebuild)
      server.watcher.on('add', rebuild)
      server.watcher.on('unlink', rebuild)
    },
  }
}

export default defineConfig({
  plugins: [framesMd(), react()],
  base: './',
  build: {
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('./index.html', import.meta.url)),
        explorer: fileURLToPath(new URL('./explorer.html', import.meta.url)),
      },
    },
  },
})
