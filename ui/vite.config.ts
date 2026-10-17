import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'

// The Hub's web layer runs inside the system WebView: WebView2 (Chromium) on Windows, WKWebView
// (Safari engine) on macOS. `target` is the floor of what the build may emit; CSS is written against the
// same floor (no color-mix(), no native nesting), matching the other ASTERION WebView surfaces.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  clearScreen: false,
  server: { port: 5173, strictPort: true, host: '127.0.0.1' },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    target: ['chrome110', 'safari15'],
    assetsInlineLimit: 0,
  },
})