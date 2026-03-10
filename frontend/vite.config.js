import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    host: '0.0.0.0',
    port: 5173,
    strictPort: true,
    watch: {
      usePolling: true,
      interval: 300
    },
    hmr: {
      protocol: 'ws',
      host: 'localhost',
      clientPort: 5173,
      port: 5173
    }
  }
});
