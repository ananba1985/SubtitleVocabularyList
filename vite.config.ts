import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
    // Tauri watches Rust; Vite must not crawl native builds or private runtime data.
    watch: {
      ignored: [
        '**/src-tauri/**',
        '**/.tools/**',
        '**/.local/**',
        '**/release/**',
        '**/output/**',
        '**/temp/**',
        '**/cache/**',
        '**/data/**',
        '**/user-data/**',
        '**/media/**',
        '**/models/**',
        '**/.playwright*/**',
      ],
    },
  },
  // Avoid discovering HTML entry points inside bundled tool sources and test output.
  optimizeDeps: { entries: ['index.html'] },
  clearScreen: false,
  build: { target: 'es2022' },
});
