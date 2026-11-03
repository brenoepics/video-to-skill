/**
 * The whole chart engine: a linear scale and a way to paint its gridlines.
 *
 * There is no charting library here on purpose. A forest plot of effect
 * sizes is a row of intervals on a shared axis, which is a table with one
 * graphical column -- so that is exactly how it is built. Each row's marks
 * live in a small inline SVG that carries no `viewBox`, so its coordinates
 * are CSS pixels and its horizontal positions are percentages of whatever
 * width the column happens to have. Nothing is scaled, so nothing is
 * distorted and no text shrinks on a phone.
 *
 * The shared gridlines and the zero line cannot live in those per-row SVGs
 * without breaking continuity between rows, so they are painted as CSS
 * gradients on the plot column -- one declaration on the table element,
 * aligned to the same percentages the marks use.
 */

export interface Tick {
  value: number;
  /** Position as a percentage of the plot column's width. */
  at: number;
  label: string;
  /** How the label is anchored, so the end labels never overflow. */
  align: 'start' | 'middle' | 'end';
}

export interface Scale {
  min: number;
  max: number;
  ticks: Tick[];
  /** Percentage position of `v`, clamped into the plot area. */
  at(v: number): number;
  /** True when `v` falls outside the domain and was clamped. */
  clipped(v: number): boolean;
  /** CSS `background-image` painting the gridlines and the emphasised zero line. */
  gridImage(): string;
  /** Percentage position of zero, or null when zero is outside the domain. */
  zeroAt: number | null;
}

/** Fraction of the column reserved at each end so marks are never half-cut. */
const INSET = 2.5;

export function linearScale(
  min: number,
  max: number,
  tickValues: number[],
  labelFor: (v: number) => string,
): Scale {
  const span = max - min || 1;
  const at = (v: number) => {
    const t = (clampTo(v, min, max) - min) / span;
    return INSET + t * (100 - 2 * INSET);
  };
  const ticks: Tick[] = tickValues.map((value, i) => ({
    value,
    at: at(value),
    label: labelFor(value),
    align: i === 0 ? 'start' : i === tickValues.length - 1 ? 'end' : 'middle',
  }));
  const zeroInside = min <= 0 && max >= 0;
  return {
    min,
    max,
    ticks,
    at,
    clipped: (v: number) => v < min || v > max,
    zeroAt: zeroInside ? at(0) : null,
    gridImage: () => gridImage(ticks, zeroInside ? at(0) : null),
  };
}

/** Domain for a success rate: always the full 0-100%, so bars are comparable. */
export function rateScale(): Scale {
  return linearScale(0, 1, [0, 0.25, 0.5, 0.75, 1], (v) => `${Math.round(v * 100)}%`);
}

/**
 * Domain for paired differences: symmetric about zero, so a positive and a
 * negative effect of equal size are drawn the same length. Asymmetric axes
 * are the easy way to make a small gain look big.
 */
export function diffScale(values: Array<number | null | undefined>): Scale {
  const finite = values.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
  const widest = finite.reduce((acc, v) => Math.max(acc, Math.abs(v)), 0);
  const bound = niceBound(widest * 1.08);
  const step = niceStep(bound);
  const ticks: number[] = [];
  for (let v = -bound; v <= bound + 1e-9; v += step) ticks.push(roundTo(v, 6));
  return linearScale(-bound, bound, ticks, (v) => {
    const centred = Math.abs(v) < 1e-9;
    if (centred) return '0';
    const sign = v > 0 ? '+' : '−';
    return `${sign}${Math.round(Math.abs(v) * 100)}`;
  });
}

const BOUNDS = [0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.75, 1];

function niceBound(v: number): number {
  for (const b of BOUNDS) if (v <= b + 1e-9) return b;
  return 1;
}

function niceStep(bound: number): number {
  for (const s of [0.05, 0.1, 0.2, 0.25, 0.5]) if (bound / s <= 5 + 1e-9) return s;
  return 0.5;
}

/**
 * One extra rule on the plot column, for a reference value such as the
 * baseline arm's rate. Painted above the gridlines (CSS paints the first
 * background layer on top), so compose it before `scale.gridImage()`.
 */
export function referenceLine(scale: Scale, value: number, color = 'var(--text-secondary)'): string {
  return line(scale.at(value), color);
}

function gridImage(ticks: Tick[], zeroAt: number | null): string {
  const layers: string[] = [];
  if (zeroAt !== null) layers.push(line(zeroAt, 'var(--axis)'));
  for (const t of ticks) {
    if (zeroAt !== null && Math.abs(t.at - zeroAt) < 0.01) continue;
    layers.push(line(t.at, 'var(--grid)'));
  }
  return layers.join(', ');
}

function line(at: number, color: string): string {
  const a = `calc(${at.toFixed(3)}% - 0.5px)`;
  const b = `calc(${at.toFixed(3)}% + 0.5px)`;
  return `linear-gradient(to right, transparent ${a}, ${color} ${a}, ${color} ${b}, transparent ${b})`;
}

function clampTo(v: number, min: number, max: number): number {
  return Math.min(Math.max(v, min), max);
}

function roundTo(v: number, digits: number): number {
  const f = 10 ** digits;
  return Math.round(v * f) / f;
}
