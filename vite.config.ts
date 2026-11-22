/// <reference types="vitest/config" />
import { createReadStream, cpSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { extname, resolve } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const PDFJS_ASSET_DIRS = ['cmaps', 'standard_fonts', 'wasm'] as const;
const PDFJS_MIME: Record<string, string> = {
  '.bcmap': 'application/octet-stream',
  '.wasm': 'application/wasm',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.pfb': 'application/octet-stream',
};

/** PDF.js が日本語フォントやスキャン画像の描画に使う静的アセット（cmaps / standard_fonts / wasm）を配信・出力する */
function pdfjsAssets(): Plugin {
  const pkgDir = resolve('node_modules/pdfjs-dist');
  let outDir = resolve('dist');

  return {
    name: 'mark-shell-pdfjs-assets',
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const rawUrl = req.url?.split('?')[0] ?? '';
        if (!rawUrl.startsWith('/pdfjs/')) return next();
        const rel = rawUrl.slice('/pdfjs/'.length);
        const top = rel.split('/')[0];
        if (!PDFJS_ASSET_DIRS.includes(top as (typeof PDFJS_ASSET_DIRS)[number])) return next();
        const filePath = resolve(pkgDir, rel);
        if (!filePath.startsWith(pkgDir) || !existsSync(filePath) || !statSync(filePath).isFile()) return next();
        res.setHeader('Content-Type', PDFJS_MIME[extname(filePath).toLowerCase()] ?? 'application/octet-stream');
        createReadStream(filePath).pipe(res);
      });
    },
    writeBundle() {
      for (const dir of PDFJS_ASSET_DIRS) {
        const src = resolve(pkgDir, dir);
        const dst = resolve(outDir, 'pdfjs', dir);
        if (!existsSync(src)) continue;
        mkdirSync(dst, { recursive: true });
        cpSync(src, dst, { recursive: true });
      }
    },
  };
}

export default defineConfig({
  plugins: [svelte(), pdfjsAssets()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/docs/**'] },
  },
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 5000,
  },
});
