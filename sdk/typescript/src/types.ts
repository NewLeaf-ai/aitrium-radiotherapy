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
