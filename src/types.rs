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
