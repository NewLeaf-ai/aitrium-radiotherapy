use aitrium_dvh::DvhStats;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    FileNotFound,
    DicomParseError,
    MatchingError,
    ComputeError,
    Internal,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "INVALID_INPUT"),
            Self::FileNotFound => write!(f, "FILE_NOT_FOUND"),
            Self::DicomParseError => write!(f, "DICOM_PARSE_ERROR"),
            Self::MatchingError => write!(f, "MATCHING_ERROR"),
            Self::ComputeError => write!(f, "COMPUTE_ERROR"),
            Self::Internal => write!(f, "INTERNAL"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Deserialize)]
pub struct RtInspectRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtInspectResponse {
    pub schema_version: String,
    pub total_files: u64,
    pub total_dicom_files: u64,
    pub studies: Vec<StudyInspection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudyInspection {
    pub study_instance_uid: String,
    pub modalities: StudyModalities,
    pub structures: Vec<StructureInfo>,
    pub plans: Vec<PlanInfo>,
    pub dose_grids: Vec<DoseGridInfo>,
    pub rtstruct_path: String,
    pub rtdose_path: String,
    pub rtplan_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudyModalities {
    #[serde(rename = "CT")]
    pub ct: usize,
    #[serde(rename = "RTSTRUCT")]
    pub rtstruct: usize,
    #[serde(rename = "RTPLAN")]
    pub rtplan: usize,
    #[serde(rename = "RTDOSE")]
    pub rtdose: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructureInfo {
    pub roi_number: i32,
    pub name: String,
    pub category: ROITypeCategory,
    pub observation_type: Option<String>,
    pub volume_cc: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ROITypeCategory {
    Target,
    Organ,
    External,
    Device,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanInfo {
    pub plan_name: Option<String>,
    pub sop_instance_uid: String,
    pub dose_references: Vec<DoseReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_fractions: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_geometry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radiation_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beam_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beam_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoseReference {
    pub reference_type: Option<String>,
    pub structure_type: Option<String>,
    pub prescription_dose_gy: Option<f64>,
    pub referenced_roi_number: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoseGridInfo {
    pub sop_instance_uid: String,
    pub dimensions: DoseGridDimensions,
    pub pixel_spacing_mm: DoseGridSpacing,
    pub dose_scaling_gy: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoseGridDimensions {
    pub rows: usize,
    pub cols: usize,
    pub frames: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoseGridSpacing {
    pub row: f64,
    pub col: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RtDvhRequest {
    pub rtstruct_path: String,
    pub rtdose_path: String,
    #[serde(default)]
    pub structures: Option<Vec<String>>,
    #[serde(default)]
    pub interpolation: bool,
    #[serde(default)]
    pub z_segments: u32,
    #[serde(default = "default_include_curves")]
    pub include_curves: bool,
    /// When include_curves is true, downsample each curve to at most this many
    /// points. Uses uniform sampling with guaranteed first/last retention.
    /// Default: no limit (full resolution).
    #[serde(default)]
    pub max_points: Option<u32>,
    /// Round curve array values to this many decimal places.
    /// Reduces JSON size significantly (e.g. 4 → "12.3456" vs "12.345600000000001").
    /// Default: no rounding (full f64 precision).
    #[serde(default)]
    pub precision: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtDvhResponse {
    pub schema_version: String,
    pub dvhs: Vec<RoiDvhOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoiDvhOutput {
    pub roi_name: String,
    pub stats: DvhStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doses_gy: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes_cc: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes_pct: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RtDvhMetricsRequest {
    pub rtstruct_path: String,
    pub rtdose_path: String,
    #[serde(default)]
    pub structures: Option<Vec<String>>,
    #[serde(default)]
    pub interpolation: bool,
    #[serde(default)]
    pub z_segments: u32,
    pub metrics: Vec<DvhMetricSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DvhMetricSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub query: DvhMetricQuery,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DvhMetricQuery {
    DoseAtVolume {
        volume_percent: f64,
    },
    VolumeAtDose {
        dose_gy: f64,
        #[serde(default)]
        volume_unit: VolumeUnit,
    },
    Stat {
        stat: DvhStatField,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VolumeUnit {
    #[default]
    Percent,
    Cc,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DvhStatField {
    NBins,
    TotalCc,
    MinGy,
    MaxGy,
    MeanGy,
    D100Gy,
    D98Gy,
    D95Gy,
    D90Gy,
    D80Gy,
    D70Gy,
    D60Gy,
    D50Gy,
    D40Gy,
    D30Gy,
    D20Gy,
    D10Gy,
    D5Gy,
    D2Gy,
    D1Gy,
    D0Gy,
    HomogeneityIndex,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtDvhMetricsResponse {
    pub schema_version: String,
    pub structures: Vec<RoiMetricOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoiMetricOutput {
    pub roi_name: String,
    pub metrics: Vec<DvhMetricValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DvhMetricValue {
    pub id: String,
    #[serde(flatten)]
    pub query: DvhMetricQuery,
    pub value: f64,
    pub unit: MetricUnit,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Gy,
    Cc,
    Percent,
    Ratio,
    Count,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
}

fn default_include_curves() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize)]
pub struct RtAnonymizeMetadataRequest {
    pub source_path: String,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub policy: Option<Value>,
    #[serde(default)]
    pub policy_path: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub policy_overrides: Option<Value>,
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default)]
    pub allow_existing_output: bool,
    #[serde(default)]
    pub report_path: Option<String>,
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    #[serde(default = "default_true")]
    pub fail_on_error: bool,
    #[serde(default)]
    pub include_trace: bool,
    #[serde(default)]
    pub deterministic_uid_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RtAnonymizeTemplateGetRequest {
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RtAnonymizeTemplateUpdateRequest {
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub policy: Option<Value>,
    #[serde(default)]
    pub policy_overrides: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RtAnonymizeTemplateResetRequest {
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtAnonymizeMetadataResponse {
    pub schema_version: String,
    pub mode: String,
    pub source_summary: AnonymizeSourceSummary,
    pub output_summary: AnonymizeOutputSummary,
    pub action_counts: AnonymizeActionCounts,
    pub rule_counts: AnonymizeRuleCounts,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    pub safety_checks: AnonymizeSafetyChecks,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_trace: Vec<AnonymizeDecisionTrace>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtAnonymizeTemplateGetResponse {
    pub schema_version: String,
    pub template_name: String,
    pub template_path: String,
    pub source: String,
    pub policy: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtAnonymizeTemplateUpdateResponse {
    pub schema_version: String,
    pub template_name: String,
    pub template_path: String,
    pub source: String,
    pub policy: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtAnonymizeTemplateResetResponse {
    pub schema_version: String,
    pub template_name: String,
    pub template_path: String,
    pub deleted: bool,
    pub source_after_reset: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnonymizeSourceSummary {
    pub source_path: String,
    pub total_files: u64,
    pub dicom_files: u64,
    pub non_dicom_files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnonymizeOutputSummary {
    pub output_path: Option<String>,
    pub files_written: u64,
    pub dicom_written: u64,
    pub non_dicom_copied: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AnonymizeActionCounts {
    pub keep: u64,
    pub remove: u64,
    pub empty: u64,
    pub replace: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AnonymizeRuleCounts {
    pub tag: u64,
    pub vr: u64,
    pub default_private: u64,
    pub default_unknown_public: u64,
    pub default_keep: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AnonymizeSafetyChecks {
    pub source_exists: bool,
    pub source_is_directory: bool,
    pub output_not_source: bool,
    pub output_not_inside_source: bool,
    pub output_is_new_or_explicit_override: bool,
    pub fail_closed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnonymizeDecisionTrace {
    pub file: String,
    pub selector: String,
    pub keyword: Option<String>,
    pub vr: String,
    pub action: String,
    pub rule_source: String,
}

fn default_true() -> bool {
    true
}

fn default_max_workers() -> u32 {
    let parallelism = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    std::cmp::min(parallelism, 8) as u32
}
