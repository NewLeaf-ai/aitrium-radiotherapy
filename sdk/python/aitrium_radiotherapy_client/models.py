from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class ErrorCode(str, Enum):
    INVALID_INPUT = "INVALID_INPUT"
    FILE_NOT_FOUND = "FILE_NOT_FOUND"
    DICOM_PARSE_ERROR = "DICOM_PARSE_ERROR"
    MATCHING_ERROR = "MATCHING_ERROR"
    COMPUTE_ERROR = "COMPUTE_ERROR"
    INTERNAL = "INTERNAL"


class ApiErrorModel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    code: ErrorCode
    message: str
    details: Any | None = None


class StudyModalities(BaseModel):
    model_config = ConfigDict(extra="forbid")

    CT: int
    RTSTRUCT: int
    RTPLAN: int
    RTDOSE: int


class StructureInfo(BaseModel):
    model_config = ConfigDict(extra="forbid")

    roi_number: int
    name: str
    category: str
    observation_type: str | None = None
    volume_cc: float | None = None


class DoseReference(BaseModel):
    model_config = ConfigDict(extra="forbid")

    reference_type: str | None = None
    structure_type: str | None = None
    prescription_dose_gy: float | None = None
    referenced_roi_number: int | None = None


class PlanInfo(BaseModel):
    model_config = ConfigDict(extra="forbid")

    plan_name: str | None = None
    sop_instance_uid: str
    dose_references: list[DoseReference]


class DoseGridDimensions(BaseModel):
    model_config = ConfigDict(extra="forbid")

    rows: int
    cols: int
    frames: int


class DoseGridSpacing(BaseModel):
    model_config = ConfigDict(extra="forbid")

    row: float
    col: float


class DoseGridInfo(BaseModel):
    model_config = ConfigDict(extra="forbid")

    sop_instance_uid: str
    dimensions: DoseGridDimensions
    pixel_spacing_mm: DoseGridSpacing
    dose_scaling_gy: float


class StudyInspection(BaseModel):
    model_config = ConfigDict(extra="forbid")

    study_instance_uid: str
    modalities: StudyModalities
    structures: list[StructureInfo]
    plans: list[PlanInfo]
    dose_grids: list[DoseGridInfo]
    rtstruct_path: str
    rtdose_path: str
    rtplan_path: str | None = None


class RtInspectResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    total_files: int
    total_dicom_files: int
    studies: list[StudyInspection]
    warnings: list[str] = []


class DvhStats(BaseModel):
    model_config = ConfigDict(extra="allow")


class RoiDvh(BaseModel):
    model_config = ConfigDict(extra="forbid")

    roi_name: str
    stats: DvhStats
    doses_gy: list[float] | None = None
    volumes_cc: list[float] | None = None
    volumes_pct: list[float] | None = None


class RtDvhResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    dvhs: list[RoiDvh]
    warnings: list[str] = []


class DoseAtVolumeMetric(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    type: Literal["dose_at_volume"]
    volume_percent: float
    value: float
    unit: Literal["gy"]


class VolumeAtDoseMetric(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    type: Literal["volume_at_dose"]
    dose_gy: float
    volume_unit: Literal["percent", "cc"] | None = None
    value: float
    unit: Literal["percent", "cc"]


class StatMetric(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    type: Literal["stat"]
    stat: str
    value: float
    unit: Literal["gy", "cc", "percent", "ratio", "count"]


DvhMetricValue = Annotated[
    DoseAtVolumeMetric | VolumeAtDoseMetric | StatMetric,
    Field(discriminator="type"),
]


class RoiMetricResult(BaseModel):
    model_config = ConfigDict(extra="forbid")

    roi_name: str
    metrics: list[DvhMetricValue]


class RtDvhMetricsResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    structures: list[RoiMetricResult]
    warnings: list[str] = []


class ToolSpec(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str
    description: str
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]


class AnonymizeSourceSummary(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_path: str
    total_files: int
    dicom_files: int
    non_dicom_files: int


class AnonymizeOutputSummary(BaseModel):
    model_config = ConfigDict(extra="forbid")

    output_path: str | None = None
    files_written: int
    dicom_written: int
    non_dicom_copied: int


class AnonymizeActionCounts(BaseModel):
    model_config = ConfigDict(extra="forbid")

    keep: int
    remove: int
    empty: int
    replace: int


class AnonymizeRuleCounts(BaseModel):
    model_config = ConfigDict(extra="forbid")

    tag: int
    vr: int
    default_private: int
    default_unknown_public: int
    default_keep: int


class AnonymizeSafetyChecks(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_exists: bool
    source_is_directory: bool
    output_not_source: bool
    output_not_inside_source: bool
    output_is_new_or_explicit_override: bool
    fail_closed: bool


class AnonymizeDecisionTrace(BaseModel):
    model_config = ConfigDict(extra="forbid")

    file: str
    selector: str
    keyword: str | None = None
    vr: str
    action: str
    rule_source: str


class RtAnonymizeMetadataResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    mode: Literal["dry_run", "write"]
    source_summary: AnonymizeSourceSummary
    output_summary: AnonymizeOutputSummary
    action_counts: AnonymizeActionCounts
    rule_counts: AnonymizeRuleCounts
    warnings: list[str] = []
    errors: list[str] = []
    safety_checks: AnonymizeSafetyChecks
    duration_ms: int
    decision_trace: list[AnonymizeDecisionTrace] = []


class RtAnonymizeTemplateGetResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    template_name: Literal["aitrium_template"]
    template_path: str
    source: Literal["runtime", "built_in_fallback"]
    policy: dict[str, Any]
    warnings: list[str] = []


class RtAnonymizeTemplateUpdateResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    template_name: Literal["aitrium_template"]
    template_path: str
    source: Literal["runtime"]
    policy: dict[str, Any]
    warnings: list[str] = []


class RtAnonymizeTemplateResetResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_version: str
    template_name: Literal["aitrium_template"]
    template_path: str
    deleted: bool
    source_after_reset: Literal["built_in_fallback"]
    warnings: list[str] = []
