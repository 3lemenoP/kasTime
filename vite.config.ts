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
  // WASM support configuration
  optimizeDeps: {
    // Don't pre-bundle the WASM module - it needs to be loaded at runtime
    exclude: ['ktcs_wasm'],
  },
  build: {
    target: 'esnext', // Required for top-level await in WASM
  },
  // Enable WebAssembly support
  server: {
    fs: {
      // Allow serving files from the src/wasm directory
      allow: ['..'],
    },
  },
})
