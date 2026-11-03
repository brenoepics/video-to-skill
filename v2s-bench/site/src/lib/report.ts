/**
 * The site's single data source.
 *
 * Reads one report bundle, validates its schema version, and -- this is the
 * load-bearing part -- *sanitises* it before any page can see it.
 *
 * The honesty rules are enforced here rather than in the templates, because
 * a template is one `{cond && ...}` away from leaking a number it should not
 * show. After `loadReport()` returns, a `no-results` bundle physically does
 * not contain an effect size, a rate, a verdict or a cell, so no page can
 * render one by accident and no screenshot of this site can show a figure
 * that was never measured.
 *
 * Escalation, never relaxation: where two signals in the bundle disagree
 * about how trustworthy it is, the more cautious one wins and the
 * disagreement is recorded in `gate.notes` and shown on the page.
 */

import { existsSync, readFileSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { REPORT_FALLBACK, REPORT_PATH } from '../../site.config.mjs';
import { SCHEMA_VERSION } from './types';
import type {
  ArmSummary,
  Cell,
  Comparison,
  ReportBundle,
  StatusKind,
  TaskSummary,
  Verdict,
} from './types';

const SITE_ROOT = fileURLToPath(new URL('../../', import.meta.url));

export interface Gate {
  /** The status the site renders under, after escalation. */
  kind: StatusKind;
  /** The status the bundle declared, before escalation. */
  declaredKind: StatusKind;
  /** False for `no-results`: no chart, figure or number may be drawn. */
  showFigures: boolean;
  /** True for `mock`: every figure carries a watermark and a banner. */
  isMock: boolean;
  /** True for `pilot`: real measurements, underpowered, shown with intervals. */
  isPilot: boolean;
  /** The words that must appear next to every figure, or null. */
  figureLabel: string | null;
  /** Disagreements the loader found and resolved, rendered on the page. */
  notes: string[];
}

export interface LoadedReport {
  bundle: ReportBundle;
  gate: Gate;
  /** Absolute path of the bundle that was read. */
  source: string;
  /** True when the default bundle was missing and the fallback was used. */
  usedFallback: boolean;
}

let cached: LoadedReport | null = null;

export function loadReport(): LoadedReport {
  if (cached) return cached;

  const primary = resolveFrom(REPORT_PATH);
  const fallback = resolveFrom(REPORT_FALLBACK);
  const usedFallback = !existsSync(primary);
  const source = usedFallback ? fallback : primary;

  if (!existsSync(source)) {
    throw new Error(
      `No report bundle at ${primary} and no committed fallback at ${fallback}. ` +
        'The fallback is part of the repository; restore src/data/report.empty.json.',
    );
  }

  const raw = JSON.parse(readFileSync(source, 'utf8')) as unknown;
  const bundle = validate(raw, source);
  const notes: string[] = [];
  if (usedFallback) {
    notes.push(
      `No report bundle was found at ${REPORT_PATH}, so the committed ` +
        'empty bundle was rendered instead. Nothing on this build was measured.',
    );
  }
  const gate = makeGate(bundle, notes);
  const sanitised = gate.showFigures ? bundle : stripResults(bundle);

  assertInvariants(sanitised, gate);
  cached = { bundle: sanitised, gate, source, usedFallback };
  return cached;
}

function resolveFrom(p: string): string {
  return isAbsolute(p) ? p : resolve(SITE_ROOT, p);
}

function validate(raw: unknown, source: string): ReportBundle {
  if (typeof raw !== 'object' || raw === null) {
    throw new Error(`${source}: report bundle must be a JSON object`);
  }
  const b = raw as Partial<ReportBundle>;
  if (b.schema_version !== SCHEMA_VERSION) {
    throw new Error(
      `${source}: schema_version is ${JSON.stringify(b.schema_version)}, this site ` +
        `renders schema_version ${SCHEMA_VERSION}. Refusing to guess at the difference.`,
    );
  }
  for (const key of [
    'generated_at',
    'status',
    'provenance',
    'arms',
    'comparisons',
    'tasks',
    'cells',
  ] as const) {
    if (b[key] === undefined) throw new Error(`${source}: required field "${key}" is missing`);
  }
  const kind = b.status?.kind;
  if (!kind || !['no-results', 'mock', 'pilot', 'measured'].includes(kind)) {
    throw new Error(`${source}: status.kind is ${JSON.stringify(kind)}, which is not in the enum`);
  }
  if (typeof b.status?.headline !== 'string' || b.status.headline.length === 0) {
    throw new Error(`${source}: status.headline is required and must be a non-empty string`);
  }
  return {
    ...(b as ReportBundle),
    arms: b.arms ?? [],
    comparisons: b.comparisons ?? [],
    tasks: b.tasks ?? [],
    cells: b.cells ?? [],
    strata: b.strata ?? [],
    failure_modes: b.failure_modes ?? [],
  };
}

const FIGURE_LABEL = 'MOCK DATA — not a measurement';

function makeGate(bundle: ReportBundle, notes: string[]): Gate {
  const declaredKind = bundle.status.kind;
  let kind: StatusKind = declaredKind;

  // provenance.is_mock and status.kind must agree; the cautious one wins.
  const flaggedMock = bundle.provenance?.is_mock === true;
  if (flaggedMock && kind !== 'mock') {
    notes.push(
      `provenance.is_mock is true but status.kind says "${declaredKind}". ` +
        'This build is treated as mock data.',
    );
    kind = 'mock';
  }
  if (!flaggedMock && declaredKind === 'mock') {
    notes.push(
      'status.kind says "mock" but provenance.is_mock is not set. ' +
        'This build is treated as mock data.',
    );
  }

  // A bundle that claims measurements but carries no trials is not a result.
  const trialsInRows =
    sum(bundle.arms.map((a) => a.n ?? 0)) + sum(bundle.cells.map((c) => c.n ?? 0));
  const trialsClaimed = bundle.provenance?.trials_total ?? 0;
  if (kind !== 'no-results' && trialsInRows === 0) {
    notes.push(
      `status.kind says "${declaredKind}" but the bundle contains no trials ` +
        '(every arm and cell has n = 0). Rendered as no-results.',
    );
    kind = 'no-results';
  }
  if (kind !== 'no-results' && trialsClaimed === 0 && trialsInRows > 0) {
    notes.push(
      'provenance.trials_total is 0 while arms and cells carry trials. ' +
        'The per-arm counts are what the figures use.',
    );
  }

  return {
    kind,
    declaredKind,
    showFigures: kind !== 'no-results',
    isMock: kind === 'mock',
    isPilot: kind === 'pilot',
    figureLabel: kind === 'mock' ? FIGURE_LABEL : kind === 'pilot' ? 'PILOT — underpowered' : null,
    notes,
  };
}

/**
 * Remove every measurement from a bundle that has none.
 *
 * Identity survives (which arms exist, which tasks exist, where the bundle
 * came from); quantities do not. Verdicts are quantities: a verdict computed
 * from no trials is "insufficient-data" whatever the file says.
 */
function stripResults(bundle: ReportBundle): ReportBundle {
  return {
    ...bundle,
    arms: [],
    cells: [],
    strata: [],
    failure_modes: [],
    comparisons: bundle.comparisons.map(
      (c): Comparison => ({
        hypothesis: c.hypothesis,
        claim: c.claim,
        fails_if: c.fails_if,
        treatment: c.treatment,
        reference: c.reference,
        ci_method: c.ci_method,
        p_method: c.p_method,
        n_pairs: 0,
        n_tasks: 0,
        diff: null,
        ci_lo: null,
        ci_hi: null,
        discordant_b: null,
        discordant_c: null,
        p_value: null,
        p_adjusted: null,
        verdict: 'insufficient-data',
      }),
    ),
    tasks: bundle.tasks.map(
      (t): TaskSummary => ({
        ...t,
        probe_known: null,
        headroom_band: t.headroom_band === 'saturated' ? t.headroom_band : 'unknown',
      }),
    ),
  };
}

/**
 * Build-time assertions. If a future edit lets a measurement through the
 * `no-results` gate, `pnpm build` fails instead of publishing it.
 */
function assertInvariants(bundle: ReportBundle, gate: Gate): void {
  if (gate.showFigures) return;
  const leaks: string[] = [];
  if (bundle.arms.length) leaks.push('arms');
  if (bundle.cells.length) leaks.push('cells');
  if (bundle.strata?.length) leaks.push('strata');
  if (bundle.failure_modes?.length) leaks.push('failure_modes');
  if (bundle.comparisons.some((c) => c.verdict !== 'insufficient-data')) leaks.push('verdicts');
  if (bundle.comparisons.some((c) => typeof c.diff === 'number')) leaks.push('effect sizes');
  if (bundle.tasks.some((t) => typeof t.probe_known === 'number')) leaks.push('probe scores');
  if (leaks.length) {
    throw new Error(
      `no-results build still carries ${leaks.join(', ')}. ` +
        'The site must not render a number that was never measured; fix stripResults().',
    );
  }
}

/* ---------------------------------------------------------------- queries */

export function armById(bundle: ReportBundle): Map<string, ArmSummary> {
  return new Map(bundle.arms.map((a) => [a.id, a]));
}

export function cellsForTask(bundle: ReportBundle, taskId: string): Cell[] {
  return bundle.cells.filter((c) => c.task === taskId);
}

export function taskById(bundle: ReportBundle, id: string): TaskSummary | undefined {
  return bundle.tasks.find((t) => t.id === id);
}

export function totalTrials(bundle: ReportBundle): number {
  const declared = bundle.provenance?.trials_total;
  if (typeof declared === 'number') return declared;
  return sum(bundle.arms.map((a) => a.n ?? 0));
}

export const VERDICTS: Verdict[] = [
  'supported',
  'not-supported',
  'inconclusive',
  'insufficient-data',
];

function sum(values: number[]): number {
  return values.reduce((a, b) => a + (Number.isFinite(b) ? b : 0), 0);
}
