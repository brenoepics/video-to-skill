import type { APIRoute } from 'astro';

import { loadReport } from '../lib/report';

/**
 * The bundle this site was built from, served beside the pages.
 *
 * Auditing a results page should not require trusting the page. This is the
 * exact JSON the figures were rendered from, as the loader handed it to the
 * templates -- so a reader can recompute every number on the site with
 * `v2sbench verify-results`, or by hand.
 *
 * It is the sanitised bundle: for a `no-results` build the loader has already
 * stripped the measurement fields, and what is served here is what was
 * rendered, not what was on disk. The provenance block names the source file.
 */
export const GET: APIRoute = () => {
  const { bundle } = loadReport();
  return new Response(`${JSON.stringify(bundle, null, 2)}\n`, {
    headers: { 'content-type': 'application/json; charset=utf-8' },
  });
};
