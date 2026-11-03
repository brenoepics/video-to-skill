/** Number and interval formatting. One definition each, used everywhere. */

export const DASH = '—'; // em dash: "not available", never 0
const MINUS = '−'; // true minus sign
const ENDASH = '–';

export function pct(value: number | null | undefined, digits = 0): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  return `${(value * 100).toFixed(digits)}%`;
}

/** A rate interval, e.g. "41-82%" with an en dash. */
export function pctRange(lo: number, hi: number, digits = 0): string {
  return `${(lo * 100).toFixed(digits)}${ENDASH}${(hi * 100).toFixed(digits)}%`;
}

/** A signed difference in percentage points, e.g. "+12.0 pp". */
export function pp(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  const scaled = value * 100;
  const sign = scaled > 0 ? '+' : scaled < 0 ? MINUS : '';
  return `${sign}${Math.abs(scaled).toFixed(digits)} pp`;
}

/** A signed CI on a difference, e.g. "-2.0 to +26.0 pp". */
export function ppRange(
  lo: number | null | undefined,
  hi: number | null | undefined,
  digits = 1,
): string {
  if (lo === null || lo === undefined || hi === null || hi === undefined) return DASH;
  return `${signed(lo, digits)} to ${signed(hi, digits)} pp`;
}

export function signed(value: number, digits = 1): string {
  const scaled = value * 100;
  const sign = scaled > 0 ? '+' : scaled < 0 ? MINUS : '';
  return `${sign}${Math.abs(scaled).toFixed(digits)}`;
}

export function num(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  return value.toFixed(digits);
}

export function int(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  return Math.round(value).toLocaleString('en-US');
}

export function usd(value: number | null | undefined, digits = 2): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  return `$${value.toFixed(digits)}`;
}

export function pvalue(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return DASH;
  if (value < 0.001) return '< 0.001';
  return value.toFixed(3);
}

export function isoDate(value: string | null | undefined): string {
  if (!value) return DASH;
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  const iso = d.toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 16)} UTC`;
}

export function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
