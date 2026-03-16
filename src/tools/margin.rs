use crate::types::{
    ApiError, ErrorCode, MarginCoveragePoint, MarginDiagnostics, MarginDirection, MarginStatus,
    RtMarginRequest, RtMarginResponse, SCHEMA_VERSION,
};
use aitrium_dvh::types::Roi;
use aitrium_dvh::{
    compute_margin_directed_rtstruct_on_rois, parse_rtstruct, DvhError,
    MarginDirection as DvhMarginDirection, MarginOptions, MarginResult,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

const DEFAULT_COVERAGE_THRESHOLDS_MM: [f64; 3] = [3.0, 5.0, 7.0];
const DEFAULT_SUMMARY_PERCENTILE: f64 = 5.0;
const DEFAULT_XY_RESOLUTION_MM: f64 = 1.0;
const DEFAULT_MAX_VOXELS: usize = 5_000_000;
const MAX_SUGGESTIONS: usize = 5;

pub fn handle(arguments: Value) -> Result<Value, ApiError> {
    let request: RtMarginRequest = serde_json::from_value(arguments).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid rt_margin input: {error}"),
        )
    })?;

    let mut warnings = Vec::new();
    validate_request(&request, &mut warnings)?;

    let coverage_thresholds_mm =
        normalize_coverage_thresholds(request.coverage_thresholds_mm.clone(), &mut warnings)?;
    let interpolation_segments = requested_interpolation_segments(&request, &mut warnings);
    let summary_percentile = validate_summary_percentile(
        request
            .summary_percentile
            .unwrap_or(DEFAULT_SUMMARY_PERCENTILE),
    )?;
    let direction_cone_degrees =
        validate_positive_f64(request.direction_cone_degrees, "direction_cone_degrees")?;
    if direction_cone_degrees > 180.0 {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!(
                "direction_cone_degrees must be <= 180, got {}",
                request.direction_cone_degrees
            ),
        ));
    }

    let xy_resolution_mm = validate_positive_f64(
        request.xy_resolution_mm.unwrap_or(DEFAULT_XY_RESOLUTION_MM),
        "xy_resolution_mm",
    )?;
    let z_resolution_mm = match request.z_resolution_mm {
        Some(value) => Some(validate_positive_f64(value, "z_resolution_mm")?),
        None => None,
    };
    let max_voxels = request.max_voxels.unwrap_or(DEFAULT_MAX_VOXELS);
    if max_voxels == 0 {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "max_voxels must be >= 1",
        ));
    }

    let rtstruct_path = Path::new(&request.rtstruct_path);
    if !rtstruct_path.exists() {
        return Err(ApiError::new(
            ErrorCode::FileNotFound,
            format!("RTSTRUCT not found: {}", rtstruct_path.display()),
        ));
    }

    let parse_started = Instant::now();
    let rois = parse_rtstruct(rtstruct_path).map_err(|error| {
        map_dvh_error(
            ErrorCode::DicomParseError,
            "Failed to parse RTSTRUCT",
            error,
        )
    })?;
    let parse_ms = elapsed_millis_u64(parse_started.elapsed().as_millis());

    if rois.is_empty() {
        return Err(ApiError::new(
            ErrorCode::MatchingError,
            "No structures were parsed from RTSTRUCT",
        ));
    }

    let from_structure =
        resolve_margin_structure(&rois, &request.from_structure, "from_structure")?;
    let to_structure = resolve_margin_structure(&rois, &request.to_structure, "to_structure")?;

    let options = MarginOptions {
        interpolation_segments_between_planes: interpolation_segments,
        coverage_thresholds_mm,
        direction: to_dvh_direction(request.direction),
        summary_percentile,
        direction_cone_degrees,
        xy_resolution_mm,
        z_resolution_mm: z_resolution_mm.unwrap_or(0.0),
        max_voxels,
        ..MarginOptions::default()
    };

    let compute_started = Instant::now();
    let result =
        compute_margin_directed_rtstruct_on_rois(&rois, &from_structure, &to_structure, &options)
            .map_err(|error| {
            map_dvh_error(ErrorCode::ComputeError, "Failed to compute margin", error)
        })?;
    let compute_ms = elapsed_millis_u64(compute_started.elapsed().as_millis());

    let result = result.ok_or_else(|| {
        ApiError::new(
            ErrorCode::MatchingError,
            "One or both structures could not be resolved for margin computation",
        )
        .with_details(json!({
            "reason": "structure_not_found_after_resolution",
            "from_structure": from_structure,
            "to_structure": to_structure,
        }))
    })?;

    let response = build_margin_response(
        &request,
        &from_structure,
        &to_structure,
        &result,
        warnings,
        interpolation_segments,
        xy_resolution_mm,
        z_resolution_mm,
        max_voxels,
        parse_ms,
        compute_ms,
    );

    serde_json::to_value(response).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_margin output: {error}"),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn build_margin_response(
    request: &RtMarginRequest,
    from_structure: &str,
    to_structure: &str,
    result: &MarginResult,
    mut warnings: Vec<String>,
    interpolation_segments: u32,
    xy_resolution_mm: f64,
    z_resolution_mm: Option<f64>,
    max_voxels: usize,
    parse_ms: u64,
    compute_ms: u64,
) -> RtMarginResponse {
    let finite_metrics = margin_metrics_are_finite(result);
    let has_samples = result.sample_count > 0;

    let (
        status,
        summary_mm,
        summary_percentile,
        sample_count,
        min_mm,
        p05_mm,
        p50_mm,
        p95_mm,
        mean_mm,
    ) = if finite_metrics && has_samples {
        (
            MarginStatus::Ok,
            Some(result.summary_mm),
            Some(result.summary_percentile),
            Some(result.sample_count as u64),
            Some(result.min_mm),
            Some(result.p05_mm),
            Some(result.p50_mm),
            Some(result.p95_mm),
            Some(result.mean_mm),
        )
    } else {
        warnings.push(
                "Margin engine returned no finite boundary clearance samples; reporting status=no_samples"
                    .to_string(),
            );
        (
            MarginStatus::NoSamples,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    };

    RtMarginResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        from_structure: from_structure.to_string(),
        to_structure: to_structure.to_string(),
        direction: request.direction,
        status,
        summary_mm,
        summary_percentile,
        sample_count,
        min_mm,
        p05_mm,
        p50_mm,
        p95_mm,
        mean_mm,
        coverage: sanitize_coverage_points(
            &result.coverage_within_thresholds,
            finite_metrics && has_samples,
        ),
        diagnostics: MarginDiagnostics {
            engine: "aitrium_dvh_rtstruct_v2".to_string(),
            interpolation_segments,
            direction_cone_degrees: request.direction_cone_degrees,
            requested_xy_resolution_mm: xy_resolution_mm,
            requested_z_resolution_mm: z_resolution_mm,
            z_resolution_auto: z_resolution_mm.is_none(),
            max_voxels,
            parse_ms,
            compute_ms,
        },
        warnings,
    }
}

fn sanitize_coverage_points(points: &[(f64, f64)], keep_values: bool) -> Vec<MarginCoveragePoint> {
    points
        .iter()
        .map(|(threshold_mm, percent_within)| MarginCoveragePoint {
            threshold_mm: finite_or_zero(*threshold_mm),
            percent_within: if keep_values {
                finite_or_zero(*percent_within)
            } else {
                0.0
            },
        })
        .collect()
}

fn validate_request(request: &RtMarginRequest, warnings: &mut Vec<String>) -> Result<(), ApiError> {
    if request.rtstruct_path.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "rtstruct_path is required",
        ));
    }
    if request.from_structure.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "from_structure is required",
        ));
    }
    if request.to_structure.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "to_structure is required",
        ));
    }
    if !request.interpolation && request.z_segments > 0 {
        warnings.push(
            "z_segments was provided but interpolation=false; z_segments is ignored".to_string(),
        );
    }
    Ok(())
}

fn requested_interpolation_segments(request: &RtMarginRequest, _warnings: &mut Vec<String>) -> u32 {
    if request.interpolation {
        request.z_segments
    } else {
        0
    }
}

fn normalize_coverage_thresholds(
    raw: Option<Vec<f64>>,
    warnings: &mut Vec<String>,
) -> Result<Vec<f64>, ApiError> {
    let raw_for_compare = raw.clone();
    let thresholds = raw.unwrap_or_else(|| DEFAULT_COVERAGE_THRESHOLDS_MM.to_vec());
    if thresholds.is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "coverage_thresholds_mm must contain at least one value",
        ));
    }

    let mut normalized = Vec::with_capacity(thresholds.len());
    for threshold in thresholds {
        if !threshold.is_finite() || threshold < 0.0 {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                format!("coverage_thresholds_mm must be finite and >= 0, got {threshold}"),
            ));
        }
        normalized.push(threshold);
    }

    normalized.sort_by(f64::total_cmp);
    normalized.dedup_by(|left, right| (*left - *right).abs() <= f64::EPSILON);

    if raw_thresholds_changed(raw_for_compare.as_deref(), &normalized) {
        warnings.push(
            "coverage_thresholds_mm was normalized (sorted ascending, duplicate values removed)"
                .to_string(),
        );
    }

    Ok(normalized)
}

fn raw_thresholds_changed(raw: Option<&[f64]>, normalized: &[f64]) -> bool {
    match raw {
        Some(values) => {
            values.len() != normalized.len()
                || values
                    .iter()
                    .zip(normalized.iter())
                    .any(|(left, right)| (left - right).abs() > f64::EPSILON)
        }
        None => false,
    }
}

fn validate_summary_percentile(value: f64) -> Result<f64, ApiError> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!("summary_percentile must be finite and in [0,100], got {value}"),
        ));
    }
    Ok(value)
}

fn validate_positive_f64(value: f64, field: &str) -> Result<f64, ApiError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!("{field} must be finite and > 0, got {value}"),
        ));
    }
    Ok(value)
}

fn resolve_margin_structure(
    rois: &[Roi],
    requested: &str,
    field_name: &str,
) -> Result<String, ApiError> {
    let requested_key = normalize_structure_key(requested);
    let matches = rois
        .iter()
        .filter(|roi| normalize_structure_key(&roi.name) == requested_key)
        .map(|roi| roi.name.clone())
        .collect::<Vec<_>>();

    let unique_matches = unique_names(&matches);
    match unique_matches.len() {
        0 => Err(ApiError::new(
            ErrorCode::MatchingError,
            format!("Structure not found for {field_name}: '{requested}'"),
        )
        .with_details(json!({
            "reason": "structure_not_found",
            "field": field_name,
            "requested": requested,
            "suggestions": structure_suggestions(rois, requested),
        }))),
        1 => Ok(unique_matches[0].clone()),
        _ => Err(ApiError::new(
            ErrorCode::MatchingError,
            format!("Ambiguous structure name for {field_name}: '{requested}'"),
        )
        .with_details(json!({
            "reason": "ambiguous_structure",
            "field": field_name,
            "requested": requested,
            "candidates": unique_matches,
        }))),
    }
}

fn structure_suggestions(rois: &[Roi], requested: &str) -> Vec<String> {
    let requested_key = normalize_structure_key(requested);
    let mut scored = unique_names(&rois.iter().map(|roi| roi.name.clone()).collect::<Vec<_>>())
        .into_iter()
        .map(|name| {
            let key = normalize_structure_key(&name);
            let score = if key == requested_key {
                0
            } else if key.starts_with(&requested_key) || requested_key.starts_with(&key) {
                1
            } else if key.contains(&requested_key) || requested_key.contains(&key) {
                2
            } else {
                3
            };
            (score, name)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| left.cmp(right));
    let filtered = scored
        .into_iter()
        .filter(|(score, _)| *score < 3)
        .take(MAX_SUGGESTIONS)
        .map(|(_, name)| name)
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        unique_names(&rois.iter().map(|roi| roi.name.clone()).collect::<Vec<_>>())
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .collect()
    } else {
        filtered
    }
}

fn unique_names(values: &[String]) -> Vec<String> {
    let mut unique = BTreeSet::new();
    for value in values {
        unique.insert(value.clone());
    }
    unique.into_iter().collect()
}

fn normalize_structure_key(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn margin_metrics_are_finite(result: &MarginResult) -> bool {
    [
        result.summary_mm,
        result.summary_percentile,
        result.min_mm,
        result.p05_mm,
        result.p50_mm,
        result.p95_mm,
        result.mean_mm,
    ]
    .into_iter()
    .all(f64::is_finite)
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn to_dvh_direction(direction: MarginDirection) -> Option<DvhMarginDirection> {
    match direction {
        MarginDirection::Uniform => None,
        MarginDirection::Lateral => Some(DvhMarginDirection::Lateral),
        MarginDirection::Posterior => Some(DvhMarginDirection::Posterior),
        MarginDirection::Anterior => Some(DvhMarginDirection::Anterior),
        MarginDirection::Left => Some(DvhMarginDirection::Left),
        MarginDirection::Right => Some(DvhMarginDirection::Right),
        MarginDirection::Superior => Some(DvhMarginDirection::Superior),
        MarginDirection::Inferior => Some(DvhMarginDirection::Inferior),
    }
}

fn map_dvh_error(code: ErrorCode, context: &str, error: DvhError) -> ApiError {
    ApiError::new(code, format!("{context}: {error}"))
}

fn elapsed_millis_u64(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(direction: MarginDirection) -> RtMarginRequest {
        RtMarginRequest {
            rtstruct_path: "/tmp/example_rtstruct.dcm".to_string(),
            from_structure: "CTV".to_string(),
            to_structure: "PTV".to_string(),
            direction,
            interpolation: true,
            z_segments: 2,
            coverage_thresholds_mm: Some(vec![3.0, 5.0, 7.0]),
            summary_percentile: Some(5.0),
            direction_cone_degrees: 45.0,
            xy_resolution_mm: Some(DEFAULT_XY_RESOLUTION_MM),
            z_resolution_mm: None,
            max_voxels: Some(DEFAULT_MAX_VOXELS),
        }
    }

    fn sample_result() -> MarginResult {
        MarginResult {
            min_mm: -1.0,
            p05_mm: 2.5,
            p50_mm: 4.1,
            p95_mm: 6.8,
            mean_mm: 4.3,
            summary_mm: 2.5,
            summary_percentile: 5.0,
            sample_count: 42,
            coverage_within_thresholds: vec![(3.0, 87.5), (5.0, 95.0), (7.0, 100.0)],
        }
    }

    #[test]
    fn build_margin_response_keeps_summary_metrics_for_lateral_direction() {
        let request = sample_request(MarginDirection::Lateral);
        let response = build_margin_response(
            &request,
            "CTV",
            "PTV",
            &sample_result(),
            Vec::new(),
            2,
            DEFAULT_XY_RESOLUTION_MM,
            None,
            DEFAULT_MAX_VOXELS,
            4,
            9,
        );

        assert_eq!(response.direction, MarginDirection::Lateral);
        assert_eq!(response.status, MarginStatus::Ok);
        assert_eq!(response.summary_mm, Some(2.5));
        assert_eq!(response.summary_percentile, Some(5.0));
        assert_eq!(response.sample_count, Some(42));
    }

    #[test]
    fn build_margin_response_sanitizes_non_finite_results() {
        let request = sample_request(MarginDirection::Uniform);
        let mut result = sample_result();
        result.summary_mm = f64::INFINITY;
        result.sample_count = 0;

        let response = build_margin_response(
            &request,
            "CTV",
            "PTV",
            &result,
            Vec::new(),
            0,
            DEFAULT_XY_RESOLUTION_MM,
            None,
            DEFAULT_MAX_VOXELS,
            1,
            2,
        );

        assert_eq!(response.status, MarginStatus::NoSamples);
        assert_eq!(response.summary_mm, None);
        assert_eq!(response.sample_count, None);
        assert!(response
            .coverage
            .iter()
            .all(|point| point.percent_within == 0.0));
    }

    #[test]
    fn normalize_coverage_thresholds_sorts_and_deduplicates() {
        let raw = Some(vec![5.0, 3.0, 3.0, 7.0]);
        let mut warnings = Vec::new();

        let normalized = normalize_coverage_thresholds(raw, &mut warnings).unwrap();

        assert_eq!(normalized, vec![3.0, 5.0, 7.0]);
        assert_eq!(warnings.len(), 1);
    }
}
