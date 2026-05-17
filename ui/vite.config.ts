import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    port: 5188,
    strictPort: true,
  },
  // Tauri expects a fixed port for the dev server
  clearScreen: false,
});
