// @ts-check
import { defineConfig } from 'astro/config';
import { BASE_PATH, SITE_ORIGIN } from './site.config.mjs';

export default defineConfig({
  output: 'static',
  site: SITE_ORIGIN,
  base: BASE_PATH,
  trailingSlash: 'ignore',
  build: { format: 'directory', inlineStylesheets: 'auto' },
  devToolbar: { enabled: false },
  // No integrations, no client framework, no CDN: the built site makes zero
  // third-party requests and ships no JavaScript except the small inline
  // progressive-enhancement script on /tasks.
});
