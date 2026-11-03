/**
 * Deployment and data-source configuration. One place, no duplicates.
 *
 * The site is served from GitHub Pages under a base path that differs
 * between the two planned homes, so the base path is a single value and
 * every link is built from `import.meta.env.BASE_URL` (see src/lib/url.ts).
 * Nothing in src/ hardcodes a path prefix.
 *
 *   v2s.dev/bench     ->  V2S_BENCH_BASE=/bench   V2S_BENCH_ORIGIN=https://v2s.dev
 *   bench.v2s.dev     ->  V2S_BENCH_BASE=/        V2S_BENCH_ORIGIN=https://bench.v2s.dev
 *
 * The report bundle is resolved relative to this file, so the site can be
 * built from any working directory and can be pointed at an alternative
 * bundle without editing code:
 *
 *   V2S_BENCH_REPORT=../results/report.json          (default)
 *   V2S_BENCH_REPORT=src/data/report.mock.json       (layout exercise)
 */

/** Base path the built site is served from. Leading slash, no trailing slash (except "/"). */
export const BASE_PATH = normalizeBase(process.env.V2S_BENCH_BASE ?? '/bench');

/** Absolute origin, used only for canonical URLs and the sitemap. */
export const SITE_ORIGIN = process.env.V2S_BENCH_ORIGIN ?? 'https://v2s.dev';

/** Report bundle, relative to this file (or absolute). */
export const REPORT_PATH = process.env.V2S_BENCH_REPORT ?? '../results/report.json';

/** Committed fallback: a valid bundle whose status.kind is "no-results". */
export const REPORT_FALLBACK = 'src/data/report.empty.json';

/** Where the sources behind this page live, for the "audit it yourself" links. */
export const REPO_URL = 'https://github.com/brenoepics/video-to-skill';
export const REPO_BENCH_PATH = 'v2s-bench';

function normalizeBase(value) {
  if (!value || value === '/') return '/';
  const withLeading = value.startsWith('/') ? value : `/${value}`;
  return withLeading.endsWith('/') ? withLeading.slice(0, -1) : withLeading;
}
