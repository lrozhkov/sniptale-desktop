import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: { port: 5173, strictPort: true, fs: { allow: [fileURLToPath(new URL('.', import.meta.url)), fileURLToPath(new URL('../resources/locales', import.meta.url))] } },
  clearScreen: false,
  build: { target: 'es2022' },
});
