/**
 * The only arithmetic this site performs.
 *
 * The report bundle is supposed to arrive with its intervals already
 * computed by `v2sbench report`. This module recomputes them from the raw
 * counts anyway, for two reasons:
 *
 *   1. A rate must never be shown as a bare point estimate, so a bundle
 *      that omits `wilson_lo`/`wilson_hi` still gets an interval rather
 *      than being rendered without one.
 *   2. When the bundle *does* carry them, the recomputation is compared
 *      against it and a visible mismatch marker is rendered if they
 *      disagree. That is the same check `v2sbench verify-results` makes,
 *      run again in the browser's own build.
 *
 * Mirrors harness/v2sbench/stats/wilson.py exactly, including the
 * hard-coded z and the vacuous interval at n == 0.
 */

/** Two-sided 95% normal quantile, written to ten digits as the harness does. */
export const Z_95 = 1.959963985;

/** Largest absolute difference between a published and a recomputed bound we accept. */
export const BOUND_TOLERANCE = 5e-4;

export interface Interval {
  /** Point estimate, or null when there are no trials. */
  rate: number | null;
  lo: number;
  hi: number;
  n: number;
  successes: number;
  /** True when the bundle carried no bounds and these were recomputed here. */
  recomputed: boolean;
  /** True when the bundle's bounds disagree with the recomputation. */
  mismatch: boolean;
}

/** Wilson score interval. n === 0 returns the vacuous [0, 1], as the harness does. */
export function wilsonInterval(successes: number, n: number, z = Z_95): [number, number] {
  if (n <= 0) return [0, 1];
  const k = successes;
  const z2 = z * z;
  const denom = n + z2;
  const centre = (k + z2 / 2) / denom;
  const half = (z / denom) * Math.sqrt((k * (n - k)) / n + z2 / 4);
  return [clamp01(centre - half), clamp01(centre + half)];
}

export function pointRate(successes: number, n: number): number | null {
  return n <= 0 ? null : successes / n;
}

/**
 * Build the interval a table cell renders, preferring the published bounds
 * and flagging either their absence or their disagreement with ours.
 */
export function intervalFor(input: {
  n: number;
  successes: number;
  rate?: number;
  wilson_lo?: number;
  wilson_hi?: number;
}): Interval {
  const n = Math.max(0, Math.trunc(input.n ?? 0));
  const successes = Math.min(Math.max(0, Math.trunc(input.successes ?? 0)), n);
  const [lo, hi] = wilsonInterval(successes, n);
  const published = typeof input.wilson_lo === 'number' && typeof input.wilson_hi === 'number';
  const mismatch =
    published &&
    (Math.abs(input.wilson_lo! - lo) > BOUND_TOLERANCE ||
      Math.abs(input.wilson_hi! - hi) > BOUND_TOLERANCE);
  return {
    rate: pointRate(successes, n),
    lo: published && !mismatch ? input.wilson_lo! : lo,
    hi: published && !mismatch ? input.wilson_hi! : hi,
    n,
    successes,
    recomputed: !published,
    mismatch,
  };
}

export interface DerivedInterval {
  rate: number;
  lo: number;
  hi: number;
  /** Count the rate implies over `n`. */
  successes: number;
  n: number;
}

/**
 * An interval for a secondary rate the bundle publishes without its counts
 * (error rate, policy-violation rate, key-action hit rate, skill-cited rate).
 *
 * The schema carries these as a bare fraction, and a bare point estimate is
 * not something this site shows. So the count is recovered from the rate and
 * the cell's own n, and the interval is marked as derived wherever it is
 * rendered. If the rate is not a whole number of trials out of n -- which
 * means the denominator is something other than n and the recovery would be
 * a guess -- this returns null and the figure is shown without an interval
 * and with a note saying why, rather than with an invented one.
 */
export function derivedInterval(
  rate: number | null | undefined,
  n: number,
): DerivedInterval | null {
  if (rate === null || rate === undefined || !Number.isFinite(rate)) return null;
  if (!Number.isFinite(n) || n <= 0) return null;
  const implied = rate * n;
  const rounded = Math.round(implied);
  if (Math.abs(implied - rounded) > 0.01) return null;
  if (rounded < 0 || rounded > n) return null;
  const [lo, hi] = wilsonInterval(rounded, n);
  return { rate, lo, hi, successes: rounded, n };
}

function clamp01(v: number): number {
  return Math.min(Math.max(v, 0), 1);
}
