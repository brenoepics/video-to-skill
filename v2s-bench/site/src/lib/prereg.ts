/**
 * The frozen design, transcribed from docs/preregistration.md and
 * harness/v2sbench/arms.py.
 *
 * This is site *content*, not internal notes: the hypotheses, the arms, the
 * tiers and the pre-registered comparisons have to be readable by someone
 * deciding whether to believe the numbers, and they have to be readable
 * before any number exists. Nothing here is a measurement -- measurements
 * arrive only through the report bundle.
 *
 * Where the hypotheses are defined: the pre-registration does not carry a
 * standalone hypothesis table. H1 and H3-H6 and H8 are defined by the
 * COMPARISONS tuple in harness/v2sbench/arms.py, which is what the analysis
 * code tests; H2 is defined by section 5.1 and H7 by section 8.4, because
 * neither is a paired two-arm difference. The site says which is which
 * rather than presenting all eight as if they were the same kind of claim.
 */

export type HypothesisFamily = 'primary' | 'ablation' | 'stratified' | 'safety';

export interface Hypothesis {
  id: string;
  /** The claim, as the harness states it. */
  statement: string;
  evidence: string;
  failsIf: string;
  family: HypothesisFamily;
  /** Where the claim is declared. */
  source: string;
  note?: string;
}

export const FAMILY_LABEL: Record<HypothesisFamily, string> = {
  primary: 'Primary',
  ablation: 'Ablation family (Holm–Bonferroni)',
  stratified: 'Stratified',
  safety: 'Safety',
};

export const HYPOTHESES: Hypothesis[] = [
  {
    id: 'H1',
    statement: 'A v2s skill raises task success over a well-prompted baseline.',
    evidence: 'success(A1) − success(A0), paired over tasks',
    failsIf: 'the 95% CI for the paired difference includes 0',
    family: 'primary',
    source: 'arms.py COMPARISONS',
    note: 'Expected to be positive, and close to uninformative on its own: prior work already measures +16.6 pp for skills-vs-no-skills. It is not the headline, and it is not in the correction family.',
  },
  {
    id: 'H2',
    statement: 'The benefit is concentrated where the model lacks the knowledge.',
    evidence: 'effect size per headroom band, from the published probe scores',
    failsIf: 'the effect is flat across bands, or largest where headroom is lowest',
    family: 'stratified',
    source: 'pre-registration §5.1',
    note: 'Answered by the strata table, not by a single comparison. This is the part a reader deciding whether to run v2s actually needs.',
  },
  {
    id: 'H3',
    statement: 'Reading pixels beats reading the transcript.',
    evidence: 'success(A1) − success(A2)',
    failsIf: 'the CI includes 0 — the transcript was enough',
    family: 'ablation',
    source: 'arms.py COMPARISONS',
  },
  {
    id: 'H4',
    statement: 'Execution verification contributes beyond extraction.',
    evidence: 'success(A1) − success(A3)',
    failsIf: 'the CI includes 0 — the badge buys nothing measurable downstream',
    family: 'ablation',
    source: 'arms.py COMPARISONS',
  },
  {
    id: 'H5',
    statement: 'The benefit is not a context-length or priming artifact.',
    evidence: 'success(A1) − success(A5), read against success(A1) − success(A0)',
    failsIf: 'the placebo recovers most of A1’s gain over A0',
    family: 'ablation',
    source: 'arms.py COMPARISONS',
  },
  {
    id: 'H6',
    statement: 'A compiled skill beats pasting the raw transcript.',
    evidence: 'success(A1) − success(A6)',
    failsIf: 'the CI includes 0 — paste the transcript instead',
    family: 'ablation',
    source: 'arms.py COMPARISONS',
    note: 'The comparison this benchmark will actually be judged on: same source information, different packaging. Published effects here are single-digit points, which is what sets the sample size.',
  },
  {
    id: 'H7',
    statement: 'v2s does not degrade safety.',
    evidence: 'policy-violation rate per arm',
    failsIf: 'A1 violates more often than A0',
    family: 'safety',
    source: 'pre-registration §8.4',
    note: 'Answered by the per-arm policy-violation column, not by a paired difference.',
  },
  {
    id: 'H8',
    statement:
      'Explicit invocation beats ambient discovery — there is a discovery gap, and the generated description is leaving value on the table.',
    evidence: 'success(A7) − success(A1)',
    failsIf: 'the CI includes 0 — discovery is already working',
    family: 'ablation',
    source: 'arms.py COMPARISONS',
  },
];

export interface ArmSpec {
  id: string;
  name: string;
  /** What context material the arm supplies (pre-registration section 3). */
  supplies: string;
  /** The confound or component this arm exists to separate out (arms.py). */
  isolates: string;
  /** arms.py's own description. */
  description: string;
  armsets: string[];
}

export const ARMS: ArmSpec[] = [
  {
    id: 'A0',
    name: 'baseline',
    supplies: 'Nothing beyond the task prompt',
    isolates: '(reference condition)',
    description:
      'A competent prompt and nothing else. Keeps every tool the other arms have, including documentation reading and exploratory commands, so the baseline is allowed to discover the procedure on its own.',
    armsets: ['core', 'ablation', 'full'],
  },
  {
    id: 'A1',
    name: 'v2s-full',
    supplies:
      'The compiled, executed-and-verified v2s skill, discovered from its description',
    isolates: 'the product as shipped',
    description:
      'The compiled v2s skill with pixel-read commands, per-step timestamp provenance, and the verification badge written by a sandboxed execution pass.',
    armsets: ['core', 'ablation', 'packaging', 'full'],
  },
  {
    id: 'A2',
    name: 'transcript-only',
    supplies: 'A skill from the same pipeline with frame inspection withheld — ASR only',
    isolates: 'pixel grounding',
    description:
      'A skill compiled from the ASR transcript alone, with frame inspection withheld. Isolates the project’s central claim that commands must be read off pixels rather than paraphrased from narration.',
    armsets: ['ablation', 'full'],
  },
  {
    id: 'A3',
    name: 'v2s-unverified',
    supplies: 'The same extraction with verify/repair not run and the badge stripped',
    isolates: 'execution verification and repair',
    description:
      'The same extraction as A1 with `vts-extract verify` not run, so no step was executed and no repair loop corrected it. Isolates what verification buys downstream, as opposed to what it buys in confidence.',
    armsets: ['ablation', 'full'],
  },
  {
    id: 'A4',
    name: 'human-expert',
    supplies: 'A grader-blind, hand-written skill from the same video',
    isolates: 'the ceiling',
    description:
      'A skill hand-written from the same video by an author who has not read the task checker. Bounds how much of the remaining gap is extraction error rather than the task simply being hard.',
    armsets: ['full'],
  },
  {
    id: 'A5',
    name: 'placebo',
    supplies: 'The real v2s pipeline run on a different video from the same tool family',
    isolates: 'context length, provenance formatting, procedure priming',
    description:
      'Genuine extraction, genuine [t=MM:SS] citations, a genuine verification badge, and a procedure that does not solve this task. Matched to A1 on length, register, formatting and provenance decoration, differing only in whether the content is the right content. Deliberately not random filler: topically-adjacent irrelevant documents have been measured to improve performance while random text helps least, so a noise placebo would understate the very confound it exists to bound.',
    armsets: ['ablation', 'full'],
  },
  {
    id: 'A6',
    name: 'raw-transcript',
    supplies: 'The raw ASR transcript pasted into context',
    isolates: 'the cheap alternative',
    description:
      'The raw ASR transcript, unprocessed. Answers the obvious objection: why compile a skill at all when you could paste the transcript? Published work that holds the source information fixed and varies only the packaging finds a much smaller effect than skill-vs-nothing, so this is the comparison the benchmark will actually be judged on — and the one that sets the sample size.',
    armsets: ['ablation', 'packaging', 'full'],
  },
  {
    id: 'A7',
    name: 'v2s-forced',
    supplies: 'A1’s skill, invoked explicitly instead of discovered',
    isolates: 'skill discovery',
    description:
      'A1 is the realistic condition; A7 is the upper bound if discovery were perfect. The gap between them is a property of the generated description field, not of the procedure, and it is separately actionable: a skill that is never triggered cannot help however good it is.',
    armsets: ['full'],
  },
];

export const ARMSETS: Array<{ id: string; arms: string[]; purpose: string }> = [
  {
    id: 'core',
    arms: ['A0', 'A1'],
    purpose: 'The headline comparison. Cheap enough to run over the whole suite.',
  },
  {
    id: 'ablation',
    arms: ['A0', 'A1', 'A2', 'A3', 'A5', 'A6'],
    purpose: 'The comparisons that distinguish v2s from a transcript summariser.',
  },
  {
    id: 'packaging',
    arms: ['A1', 'A6'],
    purpose:
      'Same information, different wrapping — the cheapest way to test the claim most likely to be challenged.',
  },
  {
    id: 'full',
    arms: ['A0', 'A1', 'A2', 'A3', 'A4', 'A5', 'A6', 'A7'],
    purpose: 'Everything, including the human ceiling and the discovery bound. Expensive.',
  },
];

export interface TierSpec {
  id: string;
  label: string;
  name: string;
  environment: string;
  why: string;
  targetN: number;
}

export const TIERS: TierSpec[] = [
  {
    id: 't1-novel-cli',
    label: 'T1',
    name: 'novel-cli',
    environment: 'Purpose-built CLI tools shipped here, Docker',
    why: 'Zero contamination — the tools did not exist before this benchmark. Measures whether information can flow video → skill → action at all.',
    targetN: 16,
  },
  {
    id: 't2-real-cli',
    label: 'T2',
    name: 'real-cli',
    environment: 'Real pinned open-source CLIs and TUIs, Docker',
    why: 'External validity on tools people use, where the model has partial knowledge.',
    targetN: 14,
  },
  {
    id: 't3-gui',
    label: 'T3',
    name: 'gui',
    environment: 'Real GUI apps under Xvfb in Docker',
    why: 'The regime v2s targets — screencasts of graphical apps.',
    targetN: 8,
  },
  {
    id: 't4-game',
    label: 'T4',
    name: 'game',
    environment: 'Open-source games, headless, seeded',
    why: 'Long-horizon and knowledge-dense, where tutorial video is abundant.',
    targetN: 4,
  },
];

export const TIER_BY_ID = new Map(TIERS.map((t) => [t.id, t]));
export const ARM_BY_ID = new Map(ARMS.map((a) => [a.id, a]));

export function tierLabel(id: string | undefined): string {
  if (!id) return '—';
  return TIER_BY_ID.get(id)?.label ?? id;
}

export function armLabel(id: string): string {
  const arm = ARM_BY_ID.get(id);
  return arm ? `${arm.id} · ${arm.name}` : id;
}
