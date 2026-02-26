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
