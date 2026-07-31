import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  build: {
    target: 'esnext', // Required for top-level await in WASM
  },
  server: {
    fs: {
      // Restrict the dev server to the project root. Do NOT use '..' — that
      // would expose the entire parent repository (Rust crates, docs, etc.)
      // over the dev server. The WASM assets live under src/, inside root.
      allow: ['.'],
    },
  },
})
