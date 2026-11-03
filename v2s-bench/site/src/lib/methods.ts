/**
 * The rest of the frozen design: sections 2-12 of docs/preregistration.md,
 * as structured content for /methodology.
 *
 * The threats-to-validity table and the "what would make us say v2s is not
 * useful" commitments are here because a benchmark run by the authors of the
 * tool under test has to publish them where a reader will actually see them,
 * not in a file nobody opens.
 */

/** Section 2. What the adjacent literature already settles, and what follows. */
export const PRIOR_WORK: Array<{ finding: string; source: string; consequence: string }> = [
  {
    finding:
      'Skills vs. no skills on containerised tasks: 33.9% → 50.5% (+16.6 pp), 87 tasks, 18 model×harness configs, 3 trials per cell.',
    source: 'SkillsBench, arXiv:2602.12670',
    consequence:
      'H1 is expected to be positive. On its own it is close to uninformative, so it is not the headline.',
  },
  {
    finding:
      'Same source information, varying only the packaging: skill 61.9% / raw 59.1% / workflow-memory 55.9%, paired CI [+0.76, +11.36].',
    source: '"Demystifying Agent Skills", arXiv:2608.14036',
    consequence:
      'The honest effect is single-digit points, not 17. This sets the sample size and makes A6 the comparison that matters.',
  },
  {
    finding:
      'A published null: +8.9 pp, 95% CI [−6.8, +24.6], p = 0.71, at 61× token overhead. Skills matter less when the environment already corrects the agent.',
    source: 'Chacko et al., arXiv:2605.20023',
    consequence:
      'We stratify by feedback bandwidth and pre-commit to publishing a null.',
  },
  {
    finding:
      'Nobody controls for context length; SkillsBench names length-matched baselines as its own top limitation.',
    source: 'arXiv:2602.12670, limitations',
    consequence: 'The placebo arm (A5) is this benchmark’s cheapest novel contribution.',
  },
  {
    finding:
      'Topically-adjacent irrelevant documents can improve performance as a "magic word", while random text helps least.',
    source: 'arXiv:2411.19463',
    consequence:
      'A random-noise placebo is the wrong placebo. Ours runs the real pipeline on the wrong video.',
  },
  {
    finding:
      'Compact skills +18.9 pp vs comprehensive documentation +5.7 pp; 2–3 skills +20.0 pp vs 4+ skills +5.2 pp.',
    source: 'arXiv:2602.12670',
    consequence:
      'Skill length and step count are recorded as covariates and tested as moderators, not assumed away.',
  },
  {
    finding:
      'Models answer NetHack quiz questions correctly and then act wrongly in-game — a knowing–doing gap.',
    source: 'BALROG, arXiv:2411.13543',
    consequence:
      'Knowing the fact and doing the procedure are different measurements, so both are measured.',
  },
  {
    finding:
      'Feeding a model the paper describing an environment beat every RL baseline with zero interaction (Crafter 27.3 vs 14.5).',
    source: 'SPRING',
    consequence: 'A known-large effect the harness can be calibrated against.',
  },
  {
    finding: 'Oracle plans lift a frontier model from 20.0% → 50.0% on tasks built from YouTube tutorials.',
    source: 'VideoGUI (NeurIPS 2024), Table 8',
    consequence: 'An external estimate of the ceiling for a video-derived procedure — our A4.',
  },
  {
    finding: 'Harness changes swung the same model from 62.7% → 99.9% on the same tasks.',
    source: 'ARC-AGI-3',
    consequence: 'The harness is content-hashed and pinned exactly like the model.',
  },
  {
    finding:
      '10–15% of tasks in borrowed agent benchmarks are defective; 15.3% of FAIL verdicts across five computer-use benchmarks were wrong; some suites pass empty responses and score trivial agents at 38%.',
    source: 'Epoch AI; arXiv:2607.28367; ABC, arXiv:2507.02825',
    consequence:
      'Every task ships an oracle that must pass and a trivial-agent gate that must fail.',
  },
  {
    finding: 'Per-config gains span +4.1 to +25.7 pp across model×harness configs.',
    source: 'arXiv:2602.12670',
    consequence:
      'A single-config result does not generalise; multi-model replication is in scope.',
  },
];

/** Section 2, closing. What is actually new here. */
export const WHAT_IS_NEW: string[] = [
  'A length-and-pipeline-matched placebo — the first control for the "any extra context helps" confound that the closest prior work names as its own chief limitation.',
  'A video-specific ablation: pixels vs. narration (A2), which no general skill benchmark can run because no other system reads commands off frames.',
  'An execution-verification ablation (A3) — testing whether a verification badge changes downstream behaviour, not just confidence.',
  'Headroom calibration by direct knowledge probes, turning the knowing–doing gap into a declared stratification instead of a confound.',
  'A discovery-gap measurement (A7): whether a skill is even triggered.',
];

export const BASELINE_RULES: Array<{ title: string; body: string }> = [
  {
    title: 'Same tools',
    body: 'Every arm may read --help, man pages, READMEs and the filesystem, and may run exploratory commands. The baseline is allowed to discover the procedure. We do not win by blindfolding it.',
  },
  {
    title: 'A real prompt',
    body: 'Committed at tasks/_prompts/baseline.md, identical across arms, instructing the agent to inspect available documentation before acting.',
  },
  {
    title: 'Matched budget and retries',
    body: 'Identical max_turns, wall-clock and attempt counts. Retrying alone raises measured accuracy, so an arm with more attempts is not a comparison.',
  },
];

export const BLINDING_RULES: string[] = [
  'Checkers live in tasks/<id>/check/, oracles in tasks/<id>/oracle/, materials in artifacts/skills/<id>/.',
  'Skill generation runs with check/ and oracle/ absent from its workspace.',
  'v2sbench validate --blind rejects any material containing a declared forbidden literal, a checker or oracle filename, or a mined string literal from either. Without this rule a skill stops being a procedure and becomes a hidden answer key.',
  'A4’s author records the blindness claim, with name and date, in artifacts/skills/<id>/human-skill/AUTHORING.md.',
];

/** Section 6. Run by CI before any trial, and published with the results. */
export const INTEGRITY_GATES: Array<{ title: string; body: string }> = [
  {
    title: 'Oracle gate',
    body: 'Every task ships a known-good solution driving the real tool. It must pass the checker in a fresh environment. This catches defective tasks and discharges "the task is achievable". An oracle that writes the expected end state directly instead of performing the procedure fails review.',
  },
  {
    title: 'Trivial-agent gate',
    body: 'A null agent — no material, no actions — is run against every checker and must score 0. Published suites have been found to pass empty responses and to score trivial agents at 38%; this gate refutes that class of error directly.',
  },
  { title: 'Leakage gate', body: 'The grader-blind rules above, enforced by a literal-mining check.' },
  {
    title: 'Modality gate',
    body: 'On gui_only tasks, a trial whose actions were entirely shell is flagged: roughly 15% of tasks in published GUI suites are silently solvable from a terminal, so a skill saying "just run this command" can manufacture a large, meaningless lift.',
  },
  {
    title: 'Determinism gate',
    body: 'Hermetic videos render byte-identically; the task suite and harness are content-hashed.',
  },
];

export const ADMISSION_CRITERIA: Array<{ title: string; body: string }> = [
  {
    title: 'Mechanically checkable',
    body: 'Success is decided by executing assertions against the final state — files, exit codes, app documents, game state. No LLM judge on the primary endpoint.',
  },
  {
    title: 'Achievable',
    body: 'The task’s oracle must pass the checker in a fresh environment. A task nobody can pass measures nothing.',
  },
  {
    title: 'The video teaches it',
    body: 'The declared knowledge gap is present in the source video, at a cited timestamp.',
  },
  {
    title: 'Safe',
    body: 'No task requires a destructive, networked or credentialed action. Tasks run offline in a throwaway container.',
  },
  {
    title: 'Licence-clean',
    body: 'No video bytes, no game ROMs, no copyrighted assets in the repository.',
  },
];

export const HEADROOM_BANDS: Array<{ band: string; rule: string }> = [
  {
    band: 'high',
    rule: 'probe_known below 0.40 — the model answered the key question cold fewer than 4 times in 10',
  },
  { band: 'medium', rule: 'probe_known from 0.40 up to 0.90' },
  { band: 'low', rule: 'probe_known at or above 0.90 — the model can already state the fact' },
  {
    band: 'saturated',
    rule: 'probe_known at or above 0.90 and feedback_bandwidth = high — the only pre-declared exclusion, and it is declared on the probe, never on the outcome',
  },
  { band: 'unknown', rule: 'not probed yet' },
];

/** Section 5.3. The exclusion rule, which is the easiest place to cheat. */
export const EXCLUSION_RULE = [
  'The closest prior work excludes "tasks with no measurable separation between conditions as low-signal". That is selection on the outcome and it mechanically inflates the reported lift — plausibly part of why +16.6 pp over-predicts a later null of +8.9 pp.',
  'So the only pre-declared exclusion is probe_known at or above 0.9 together with feedback_bandwidth = high, both measured before any arm is run. The primary endpoint is reported twice — on the full suite and on the probe-filtered subset — with the full-suite figure given first.',
  'No task is ever dropped for failing to separate the arms.',
];

export const VIDEO_TIERS: Array<{
  id: string;
  name: string;
  what: string;
  strength: string;
  weakness: string;
}> = [
  {
    id: 'hermetic',
    name: 'Hermetic',
    what: 'Rendered deterministically at run time by v2sbench render from a committed recipe: a real terminal session, dialog sequence or walkthrough, drawn frame by frame and narrated by TTS. Nothing binary is committed.',
    strength: 'Perfect reproducibility, no licensing question, no contamination.',
    weakness:
      'Flat colours, clean fonts, no compression artifacts, no cursor jitter, synthetic narration — so extraction quality on this tier overstates real-world performance. It bounds the mechanism, not the product.',
  },
  {
    id: 'real',
    name: 'Real',
    what: 'Creative-Commons or public-domain screencasts, referenced by URL plus sha256 plus licence, never redistributed by this repository.',
    strength: 'External validity.',
    weakness:
      'Reproducible only while the URL resolves. A hash mismatch invalidates the row and the harness refuses to score it.',
  },
];

/** Section 7.2. Why A2 and A6 are fair controls rather than strawmen. */
export const NARRATION_RULE =
  'In every hermetic recipe the narration is written the way a narrator really speaks — "now we claim the slot and pass the token along" — while the exact syntax appears only on screen. If narration alone sufficed, A2 and A6 would match A1 and the benchmark’s most important comparison would collapse to zero by construction.';

/** Section 8.1. Minimum detectable effect, in percentage points. Assumed, not measured. */
export const MDE_TABLE = {
  assumptions: 'α = .05, 80% power, .35 → .50, ICC = .5, ρ = .6',
  columns: ['K=1', 'K=3', 'K=5', 'K=10'],
  rows: [
    { n: 50, values: [22.9, 16.6, 15.0, 13.7] },
    { n: 75, values: [18.7, 13.5, 12.3, 11.2] },
    { n: 100, values: [16.2, 11.7, 10.6, 9.7], target: true },
    { n: 150, values: [13.2, 9.6, 8.7, 7.9] },
  ],
  target: 'Target: N = 100 tasks, K = 3 trials (MDE ≈ 11.7 pp). Minimum defensible: N = 60, K = 3.',
  pilot:
    'These numbers are conditional on assumed ICC and ρ, so the first run is a pilot: 15 tasks × 10 trials × {A0, A1} = 300 runs, to measure the actual ICC and inter-arm correlation, after which the table is regenerated and the amendment recorded.',
  difficulty:
    'The honest difficulty: the comparison this will be judged on (A1 vs A6) has a published effect of single-digit points, below the MDE of any grid we can afford. Two mitigations, declared now — contrasts between two skill arms are better powered than A1-vs-A0 because ρ between similar arms is higher; and where power is insufficient we report an estimate with its interval and say so, rather than a verdict.',
};

export const ANALYSIS_PLAN: Array<{ title: string; body: string }> = [
  {
    title: 'Unit of generalisation: the task, not the trial',
    body: 'Treating correlated trials as independent is the most common way agent benchmarks report intervals that are far too narrow — clustered standard errors run up to about 3× the naive ones.',
  },
  {
    title: 'Primary test',
    body: 'Paired sign-flip permutation on per-task mean differences — exact for N ≤ 20, sampled above. Simulated power at N=100, K=5, 15 pp is 0.97 against 0.79 for McNemar on task majorities; binarising discards the within-task gradient.',
  },
  {
    title: 'Robustness check',
    body: 'Exact McNemar on paired trial outcomes, reported alongside and never instead.',
  },
  {
    title: 'Interval',
    body: 'Difference in success rate by cluster bootstrap over tasks — 10 000 resamples, BCa. Declared limitation: at N ≈ 100 both CLT and plain bootstrap intervals under-cover in this clustered setting, so the published interval is optimistic. That is stated next to it rather than discovered later.',
  },
  {
    title: 'Per-task and per-cell rates',
    body: 'Wilson score intervals, matching the convention vts-extract eval already uses. With no trials the Wilson interval is the vacuous [0, 1]: n is what tells you there is no evidence, and the interval must not pretend otherwise.',
  },
  {
    title: 'Also reported',
    body: 'Normalised gain (p_treat − p_ref)/(1 − p_ref), so a ceiling effect does not read as a weak result; estimated ICC; and effective samples per trial 1/(1 + (K−1)·ICC), because K=5 at ICC 0.83 is worth about 1.15 independent trials and that should be visible.',
  },
  {
    title: 'Multiplicity',
    body: 'Holm–Bonferroni across the ablation family (H3, H4, H5, H6, H8). H1 is not in the family. Uncorrected values are also shown.',
  },
  {
    title: 'Difference-of-differences',
    body: 'Between ablations it carries a √2 penalty and is reported as an estimate only — no p-value, no verdict.',
  },
  {
    title: 'Maximising the paired correlation',
    body: 'Identical container digests across arms, byte-identical task prompts, arms time-interleaved within each (task, seed) rather than run as separate campaigns, and the same adapter version throughout. A run that cannot hold these constant is reported as unpaired.',
  },
  {
    title: 'Stopping rule',
    body: 'N is fixed in advance. No peeking-and-extending; any extension is an amendment with its own analysis.',
  },
  {
    title: 'Exclusions',
    body: 'The probe rule above, plus trials whose harness or container errored — recorded as error, reported as a rate per arm, never silently dropped and never counted as failures.',
  },
];

export const SECONDARY_ENDPOINTS: Array<{ name: string; what: string }> = [
  {
    name: 'turns, wall_secs, tokens_in, tokens_out, usd_cost',
    what: 'Cost of the attempt — tokens and dated dollars, because prices move.',
  },
  { name: 'first_try', what: 'Success with no failed-then-retried action.' },
  {
    name: 'key_action_hit',
    what: 'Whether the trace contains the task’s declared key actions. Procedure fidelity, independent of whether the task happened to pass.',
  },
  { name: 'hallucinated_cmd_rate', what: 'Invoked commands or subcommands that do not exist.' },
  { name: 'policy_violations', what: 'Forbidden or destructive actions attempted. This is H7.' },
  {
    name: 'skill_cited',
    what: 'For skill arms, whether the trace shows the skill was actually read and followed. An arm that wins without reading the skill is evidence of a harness leak, not of utility.',
  },
  {
    name: 'action_modality, skill_steps, material length',
    what: 'The GUI-bypass check, and the compactness moderators prior work found to matter.',
  },
  { name: 'score, milestones_reached, depth', what: 'Game tier only.' },
];

export const COST_IS_A_RESULT =
  'Cost is a first-class result, not an appendix: a published null measured +8.9 pp at 61× token overhead, and a long demonstration can cost hundreds of thousands of tokens against roughly 2k for a skill. A skill that helps slightly and costs 60× is a different product decision from one that helps slightly and is free.';

export const JUDGE_POLICY = [
  'The primary endpoint never uses a judge.',
  'Judges are confined to two labelled secondary quantities: key_action_hit on GUI traces where a regex is inadequate, and a failure-mode taxonomy.',
  'Any judge in use must publish at least 100 human-labelled comparisons with raw agreement, Cohen’s κ, a prevalence-adjusted coefficient, the confusion matrix and the human–human ceiling, plus position-swapped and cross-family replication — because temperature-0 judges still flip about 5% of items and about 40% of close calls.',
];

export const RECORD_FIELDS =
  'One immutable JSON record per trial: task id and suite version, arm, seed, model id and sampling params, harness version and digest, container image digest, video tier and sha256, material kind and sha256 and leakage verdict, the executed-command log, per-assertion results, every metric above, and timestamps. Append-only.';

export const PINNING =
  'Model ids, never aliases — an alias resolves to a different model over time and makes an old figure uninterpretable. Sampling parameters, the agent adapter version captured inside the container, the harness content digest, the task-suite digest, container digests, and the frozen pre-registration commit. A figure without this block is not published. Re-running a frozen suite on a new model is a new row, never an edit to an old one.';

/** Section 10. The rule this website implements. */
export const MOCK_RULE = [
  'The harness ships a stub agent that calls no model, so the whole pipeline can be tested for free. Every record it writes is stamped is_mock, and the report builder refuses to aggregate mock and real records.',
  'This site renders a mock bundle with a watermark on every figure and says so in the provenance block, and it renders a bundle with no trials as empty states rather than zeros.',
  'The failure mode this guards against — fabricated numbers quietly becoming the headline — is the worst thing this project could do.',
];

export interface Threat {
  threat: string;
  mitigation: string;
  residual: string;
}

/** Section 11, in full. A results site that hides this section is not credible. */
export const THREATS: Threat[] = [
  {
    threat: 'We built the tool we are grading',
    mitigation:
      'Pre-registration; grader-blind materials; real-pipeline placebo; ablations designed so a null is visible; raw records released; suite runnable by third parties.',
    residual: 'Non-zero. Independent replication invited.',
  },
  {
    threat: 'Task selection bias',
    mitigation:
      'Admission criteria fixed in advance; probes published; discoverability and feedback-bandwidth splits reported; T1 tools specified before their videos existed; no outcome-based exclusion.',
    residual: 'Choice of domains remains ours.',
  },
  {
    threat: 'Contamination',
    mitigation: 'T1 tools novel by construction; probe scores published.',
    residual: 'T2–T4 contaminated to an unknown degree; probes bound it.',
  },
  {
    threat: 'Weak-baseline inflation',
    mitigation: 'Enforced in code: identical tools, budgets, retries.',
    residual: 'Prompt quality is a judgement call; the prompt is committed.',
  },
  {
    threat: 'Context-length and priming confound',
    mitigation:
      'Real-pipeline placebo matched on length, register and provenance formatting.',
    residual: 'Cannot match topical relevance; A5 is a bound, not an exact control.',
  },
  {
    threat: 'Grader leakage',
    mitigation: 'Structural blinding plus a literal-mining gate.',
    residual: 'A sufficiently indirect leak could pass a string check.',
  },
  {
    threat: 'Defective tasks and verifiers',
    mitigation: 'Oracle gate, trivial-agent gate, modality gate.',
    residual: 'Our own checkers are as fallible as anyone’s.',
  },
  {
    threat: 'Synthetic-video optimism',
    mitigation: 'Two tiers, never averaged; the weakness stated next to the tier.',
    residual: 'Hermetic numbers will look better than reality.',
  },
  {
    threat: 'Under-covering intervals',
    mitigation: 'Stated explicitly next to the interval.',
    residual: 'The published CI is optimistic at N ≈ 100.',
  },
  {
    threat: 'Single-config over-reading',
    mitigation: 'Model id on every figure; multi-model replication in scope.',
    residual:
      'v0.1 may ship one model’s results; published per-config spread is +4.1 to +25.7 pp.',
  },
  {
    threat: 'Harness dominance',
    mitigation: 'Harness content-hashed and pinned.',
    residual: 'A harness improvement mid-series would invalidate comparability.',
  },
  {
    threat: 'LLM nondeterminism',
    mitigation: 'K ≥ 3 seeds; sampling params pinned and recorded; variance reported.',
    residual: 'Cannot be eliminated at temperature 0.',
  },
  {
    threat: 'Flaky environments',
    mitigation: 'error status, never failure; per-arm error rate published.',
    residual: 'T3 and T4 will be flakier than T1 and T2.',
  },
  {
    threat: 'Cost pressure, and therefore small N',
    mitigation: 'N and MDE declared up front; pilot first; tiered run plan.',
    residual: 'Wide intervals on T3 and T4.',
  },
  {
    threat: 'GUI bypass',
    mitigation: 'Modality recorded and gated on gui_only tasks.',
    residual: 'Classification by command inspection is imperfect.',
  },
];

/** Section 12. Committed in advance, before any data existed. */
export const NOT_USEFUL_COMMITMENTS: string[] = [
  'If H1’s interval includes 0 on T1 — the uncontaminated tier where information flow is easiest to demonstrate — the central claim has failed, and this page says so in its first paragraph.',
  'If H1 holds but H5 fails, so the placebo recovers most of the gain, we report that the measured benefit is not attributable to what the video taught.',
  'If H1 holds but H3 and H6 both fail, we report that v2s’s advantage is not distinguishable from transcript summarisation, and that this benchmark does not support the pixel-reading claim in the project README.',
  'If H4 fails, we report that execution verification, as implemented, does not measurably change downstream task success — even though it is one of the project’s headline features.',
  'If the effect appears only where probe_known is low and feedback_bandwidth is low, we report that as the finding: v2s pays off for unfamiliar, silently-failing tools and not in general. A published null with that framing already exists, and the most believable version of this study may look like it.',
];

export const VERDICT_RULES: Array<{ verdict: string; rule: string }> = [
  {
    verdict: 'supported',
    rule: 'The 95% CI for the paired difference excludes 0 in the direction the hypothesis predicted.',
  },
  {
    verdict: 'not-supported',
    rule: 'The interval is informative and does not support the claim — it includes 0 with enough precision to say so, or it points the other way. This is a finding, and it is published with the same weight as a positive one.',
  },
  {
    verdict: 'inconclusive',
    rule: 'The data are real but the interval is too wide to separate the hypothesis from its negation. Where power is insufficient the benchmark reports an estimate with its interval, not a verdict.',
  },
  { verdict: 'insufficient-data', rule: 'Not enough paired trials to compute an interval at all.' },
];
