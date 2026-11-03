/**
 * TypeScript mirror of schemas/report.schema.json (schema_version 1).
 *
 * Every field in the schema appears here and nothing else does. If the site
 * wants to say something the schema cannot carry, that is a schema change,
 * not a field invented in the view layer.
 */

export const SCHEMA_VERSION = 1 as const;

export type StatusKind = 'no-results' | 'mock' | 'pilot' | 'measured';
export type Verdict = 'supported' | 'not-supported' | 'inconclusive' | 'insufficient-data';
export type VideoTier = 'hermetic' | 'real';
export type StratumDimension = 'headroom_band' | 'discoverability' | 'tier' | 'video_tier';

export interface ReportStatus {
  kind: StatusKind;
  headline: string;
  detail?: string;
  prereg_frozen?: boolean;
}

export interface Provenance {
  harness_version?: string;
  suite_version?: string;
  suite_commit?: string | null;
  prereg_commit?: string | null;
  models?: string[];
  run_ids?: string[];
  trials_total?: number;
  trials_excluded?: number;
  usd_total?: number | null;
  video_tiers?: VideoTier[];
  is_mock?: boolean;
}

export interface ArmSummary {
  id: string;
  name: string;
  isolates?: string;
  description?: string;
  n: number;
  successes: number;
  rate: number;
  wilson_lo?: number;
  wilson_hi?: number;
  error_rate?: number;
  mean_usd?: number | null;
  mean_turns?: number | null;
  mean_wall_secs?: number | null;
  policy_violation_rate?: number | null;
  key_action_hit_rate?: number | null;
}

export interface Comparison {
  hypothesis: string;
  claim?: string;
  fails_if?: string;
  treatment: string;
  reference: string;
  n_pairs?: number;
  n_tasks?: number;
  diff?: number | null;
  ci_lo?: number | null;
  ci_hi?: number | null;
  ci_method?: string;
  discordant_b?: number | null;
  discordant_c?: number | null;
  p_value?: number | null;
  p_adjusted?: number | null;
  p_method?: string;
  verdict: Verdict;
}

export interface Stratum {
  dimension?: StratumDimension;
  value?: string;
  treatment?: string;
  reference?: string;
  n_pairs?: number;
  diff?: number | null;
  ci_lo?: number | null;
  ci_hi?: number | null;
}

export interface TaskSummary {
  id: string;
  tier: string;
  title: string;
  goal?: string;
  knowledge_gap?: string;
  discoverability?: string;
  probe_known?: number | null;
  headroom_band?: string;
  saturated?: boolean;
  video_tier?: string;
  video_license?: string | null;
  video_attribution?: string | null;
}

export interface Cell {
  task: string;
  arm: string;
  n: number;
  successes: number;
  errors?: number;
  rate?: number;
  wilson_lo?: number;
  wilson_hi?: number;
  mean_turns?: number | null;
  mean_usd?: number | null;
  key_action_hit_rate?: number | null;
  skill_cited_rate?: number | null;
}

export interface FailureMode {
  label?: string;
  arm?: string;
  count?: number;
  example_trial?: string;
  judge?: string | null;
  judge_kappa?: number | null;
}

export interface ReportBundle {
  schema_version: typeof SCHEMA_VERSION;
  generated_at: string;
  status: ReportStatus;
  provenance: Provenance;
  arms: ArmSummary[];
  comparisons: Comparison[];
  strata?: Stratum[];
  tasks: TaskSummary[];
  cells: Cell[];
  failure_modes?: FailureMode[];
}
