/**
 * Link building. Every internal href goes through here, so the deployment
 * base path exists in exactly one place (site.config.mjs -> astro `base` ->
 * import.meta.env.BASE_URL) and no link in the site hardcodes a prefix.
 */

const BASE = import.meta.env.BASE_URL || '/';

export function href(path = '/'): string {
  const base = BASE.endsWith('/') ? BASE.slice(0, -1) : BASE;
  const rest = path.startsWith('/') ? path : `/${path}`;
  const joined = base + rest;
  return joined === '' ? '/' : joined;
}

/** True when `path` is the page currently being rendered. */
export function isCurrent(currentPath: string, path: string): boolean {
  return normalize(currentPath) === normalize(href(path));
}

function normalize(p: string): string {
  const stripped = p.length > 1 && p.endsWith('/') ? p.slice(0, -1) : p;
  return stripped === '' ? '/' : stripped;
}
