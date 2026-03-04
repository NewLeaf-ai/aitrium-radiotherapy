export type ErrorCode =
  | "INVALID_INPUT"
  | "FILE_NOT_FOUND"
  | "DICOM_PARSE_ERROR"
  | "MATCHING_ERROR"
  | "COMPUTE_ERROR"
  | "INTERNAL";

export interface ApiError {
  code: ErrorCode;
  message: string;
  details?: unknown;
}

export interface ToolSpec {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  output_schema: Record<string, unknown>;
}

export interface StudyModalities {
  CT: number;
  RTSTRUCT: number;
  RTPLAN: number;
  RTDOSE: number;
}

export interface StructureInfo {
  roi_number: number;
  name: string;
  category: "Target" | "Organ" | "External" | "Device" | "Other";
  observation_type?: string | null;
  volume_cc?: number | null;
}

export interface DoseReference {
  reference_type?: string | null;
  structure_type?: string | null;
  prescription_dose_gy?: number | null;
  referenced_roi_number?: number | null;
}

export interface PlanInfo {
  plan_name?: string | null;
  sop_instance_uid: string;
  dose_references: DoseReference[];
}

export interface DoseGridDimensions {
  rows: number;
  cols: number;
  frames: number;
}

export interface DoseGridSpacing {
  row: number;
  col: number;
}

export interface DoseGridInfo {
  sop_instance_uid: string;
  dimensions: DoseGridDimensions;
  pixel_spacing_mm: DoseGridSpacing;
  dose_scaling_gy: number;
}

export interface StudyInspection {
  study_instance_uid: string;
  modalities: StudyModalities;
  structures: StructureInfo[];
  plans: PlanInfo[];
  dose_grids: DoseGridInfo[];
  rtstruct_path: string;
  rtdose_path: string;
  rtplan_path?: string | null;
}

export interface RtInspectResponse {
  schema_version: string;
  total_files: number;
  total_dicom_files: number;
  studies: StudyInspection[];
  warnings?: string[];
}

export interface RoiDvh {
  roi_name: string;
  stats: Record<string, number>;
  doses_gy?: number[];
  volumes_cc?: number[];
  volumes_pct?: number[];
}

export interface RtDvhResponse {
  schema_version: string;
  dvhs: RoiDvh[];
  warnings?: string[];
}

export type VolumeUnit = "percent" | "cc";
export type MetricUnit = "gy" | "cc" | "percent" | "ratio" | "count";
export type DvhStatField =
  | "n_bins"
  | "total_cc"
  | "min_gy"
  | "max_gy"
  | "mean_gy"
  | "d100_gy"
  | "d98_gy"
  | "d95_gy"
  | "d90_gy"
  | "d80_gy"
  | "d70_gy"
  | "d60_gy"
  | "d50_gy"
  | "d40_gy"
  | "d30_gy"
  | "d20_gy"
  | "d10_gy"
  | "d5_gy"
  | "d2_gy"
  | "d1_gy"
  | "d0_gy"
  | "homogeneity_index";

export type DvhMetricSpec =
  | {
      id?: string;
      type: "dose_at_volume";
      volume_percent: number;
    }
  | {
      id?: string;
      type: "volume_at_dose";
      dose_gy: number;
      volume_unit?: VolumeUnit;
    }
  | {
      id?: string;
      type: "stat";
      stat: DvhStatField;
    };

export type DvhMetricValue =
  | {
      id: string;
      type: "dose_at_volume";
      volume_percent: number;
      value: number;
      unit: "gy";
    }
  | {
      id: string;
      type: "volume_at_dose";
      dose_gy: number;
      volume_unit?: VolumeUnit;
      value: number;
      unit: "percent" | "cc";
    }
  | {
      id: string;
      type: "stat";
      stat: DvhStatField;
      value: number;
      unit: MetricUnit;
    };

export interface RoiMetricResult {
  roi_name: string;
  metrics: DvhMetricValue[];
}

export interface RtDvhMetricsResponse {
  schema_version: string;
  structures: RoiMetricResult[];
  warnings?: string[];
}

export interface AnonymizePolicy {
  tag_rules?: Record<string, unknown>;
  vr_rules?: Record<string, unknown>;
  defaults?: {
    private_tag_default?: "keep" | "remove" | "empty" | "replace";
    unknown_public_default?: "keep" | "remove" | "empty" | "replace";
  };
}

export interface RtAnonymizeMetadataInput {
  source_path: string;
  output_path?: string;
  policy?: AnonymizePolicy;
  policy_path?: string;
  template?:
    | "strict_phi_safe"
    | "research_balanced"
    | "minimal_explicit"
    | "aitrium_default"
    | "aitrium_template";
  policy_overrides?: Record<string, unknown>;
  dry_run?: boolean;
  allow_existing_output?: boolean;
  report_path?: string;
  max_workers?: number;
  fail_on_error?: boolean;
  include_trace?: boolean;
  deterministic_uid_secret?: string;
}

export interface AnonymizeSourceSummary {
  source_path: string;
  total_files: number;
  dicom_files: number;
  non_dicom_files: number;
}

export interface AnonymizeOutputSummary {
  output_path?: string | null;
  files_written: number;
  dicom_written: number;
  non_dicom_copied: number;
}

export interface AnonymizeActionCounts {
  keep: number;
  remove: number;
  empty: number;
  replace: number;
}

export interface AnonymizeRuleCounts {
  tag: number;
  vr: number;
  default_private: number;
  default_unknown_public: number;
  default_keep: number;
}

export interface AnonymizeSafetyChecks {
  source_exists: boolean;
  source_is_directory: boolean;
  output_not_source: boolean;
  output_not_inside_source: boolean;
  output_is_new_or_explicit_override: boolean;
  fail_closed: boolean;
}

export interface AnonymizeDecisionTrace {
  file: string;
  selector: string;
  keyword?: string | null;
  vr: string;
  action: string;
  rule_source: string;
}

export interface RtAnonymizeMetadataResponse {
  schema_version: string;
  mode: "dry_run" | "write";
  source_summary: AnonymizeSourceSummary;
  output_summary: AnonymizeOutputSummary;
  action_counts: AnonymizeActionCounts;
  rule_counts: AnonymizeRuleCounts;
  warnings?: string[];
  errors?: string[];
  safety_checks: AnonymizeSafetyChecks;
  duration_ms: number;
  decision_trace?: AnonymizeDecisionTrace[];
}

export interface RtAnonymizeTemplateGetInput {
  template?: "aitrium_template";
}

export interface RtAnonymizeTemplateUpdateInput {
  template?: "aitrium_template";
  policy?: AnonymizePolicy;
  policy_overrides?: Record<string, unknown>;
}

export interface RtAnonymizeTemplateResetInput {
  template?: "aitrium_template";
}

export interface RtAnonymizeTemplateGetResponse {
  schema_version: string;
  template_name: "aitrium_template";
  template_path: string;
  source: "runtime" | "built_in_fallback";
  policy: AnonymizePolicy;
  warnings?: string[];
}

export interface RtAnonymizeTemplateUpdateResponse {
  schema_version: string;
  template_name: "aitrium_template";
  template_path: string;
  source: "runtime";
  policy: AnonymizePolicy;
  warnings?: string[];
}

export interface RtAnonymizeTemplateResetResponse {
  schema_version: string;
  template_name: "aitrium_template";
  template_path: string;
  deleted: boolean;
  source_after_reset: "built_in_fallback";
  warnings?: string[];
}
