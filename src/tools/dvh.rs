use crate::types::{
    ApiError, DvhMetricQuery, DvhMetricSpec, DvhMetricValue, DvhStatField, ErrorCode, MetricUnit,
    RoiDvhOutput, RoiMetricOutput, RtDvhMetricsRequest, RtDvhMetricsResponse, RtDvhRequest,
    RtDvhResponse, VolumeUnit, SCHEMA_VERSION,
};
use aitrium_dvh::{parse_rtdose, parse_rtstruct, to_json_format, DvhEngine, DvhOptions, DvhStats};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub fn handle(arguments: Value) -> Result<Value, ApiError> {
    let request: RtDvhRequest = serde_json::from_value(arguments).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid rt_dvh input: {error}"),
        )
    })?;

    let computation = compute_dvhs(
        &request.rtstruct_path,
        &request.rtdose_path,
        request.structures,
        request.interpolation,
        request.z_segments,
    )?;

    let dvhs = computation
        .items
        .into_iter()
        .map(|item| {
            let (doses_gy, volumes_cc, volumes_pct) = if request.include_curves {
                let mut doses = item.doses_gy;
                let mut vols_cc = item.volumes_cc;
                let mut vols_pct = item.volumes_pct;

                // Downsample curves if max_points is specified
                if let Some(max) = request.max_points {
                    let max = max as usize;
                    if doses.len() > max && max >= 2 {
                        let indices = uniform_sample_indices(doses.len(), max);
                        doses = pick_indices(&doses, &indices);
                        vols_cc = pick_indices(&vols_cc, &indices);
                        vols_pct = pick_indices(&vols_pct, &indices);
                    }
                }

                // Round values if precision is specified
                if let Some(decimals) = request.precision {
                    let factor = 10f64.powi(decimals as i32);
                    round_vec(&mut doses, factor);
                    round_vec(&mut vols_cc, factor);
                    round_vec(&mut vols_pct, factor);
                }

                (Some(doses), Some(vols_cc), Some(vols_pct))
            } else {
                (None, None, None)
            };

            RoiDvhOutput {
                roi_name: item.roi_name,
                stats: item.stats,
                doses_gy,
                volumes_cc,
                volumes_pct,
            }
        })
        .collect::<Vec<_>>();

    let output = RtDvhResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        dvhs,
        warnings: computation.warnings,
    };

    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_dvh output: {error}"),
        )
    })
}

pub fn handle_metrics(arguments: Value) -> Result<Value, ApiError> {
    let request: RtDvhMetricsRequest = serde_json::from_value(arguments).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid rt_dvh_metrics input: {error}"),
        )
    })?;

    validate_metric_specs(&request.metrics)?;
    let metric_ids = resolve_metric_ids(&request.metrics)?;

    let computation = compute_dvhs(
        &request.rtstruct_path,
        &request.rtdose_path,
        request.structures,
        request.interpolation,
        request.z_segments,
    )?;

    let mut structures = Vec::new();
    for item in &computation.items {
        let metrics = request
            .metrics
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let (value, unit) = evaluate_metric(spec, item)?;

                Ok(DvhMetricValue {
                    id: metric_ids[index].clone(),
                    query: spec.query.clone(),
                    value,
                    unit,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;

        structures.push(RoiMetricOutput {
            roi_name: item.roi_name.clone(),
            metrics,
        });
    }

    let output = RtDvhMetricsResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        structures,
        warnings: computation.warnings,
    };

    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_dvh_metrics output: {error}"),
        )
    })
}

#[derive(Debug, Clone)]
struct DvhComputation {
    items: Vec<DvhComputationItem>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct DvhComputationItem {
    roi_name: String,
    stats: DvhStats,
    doses_gy: Vec<f64>,
    volumes_cc: Vec<f64>,
    volumes_pct: Vec<f64>,
}

fn compute_dvhs(
    rtstruct_path: &str,
    rtdose_path: &str,
    structures: Option<Vec<String>>,
    interpolation: bool,
    z_segments: u32,
) -> Result<DvhComputation, ApiError> {
    let rtstruct_path = Path::new(rtstruct_path);
    if !rtstruct_path.exists() {
        return Err(ApiError::new(
            ErrorCode::FileNotFound,
            format!("RTSTRUCT not found: {}", rtstruct_path.display()),
        ));
    }

    let rtdose_path = Path::new(rtdose_path);
    if !rtdose_path.exists() {
        return Err(ApiError::new(
            ErrorCode::FileNotFound,
            format!("RTDOSE not found: {}", rtdose_path.display()),
        ));
    }

    let rois = parse_rtstruct(rtstruct_path).map_err(|error| {
        ApiError::new(
            ErrorCode::DicomParseError,
            format!("Failed to parse RTSTRUCT: {error}"),
        )
    })?;

    let dose_grid = parse_rtdose(rtdose_path).map_err(|error| {
        ApiError::new(
            ErrorCode::DicomParseError,
            format!("Failed to parse RTDOSE: {error}"),
        )
    })?;

    let interpolation_resolution_mm = if interpolation {
        Some((
            dose_grid.pixel_spacing_row_mm / 2.0,
            dose_grid.pixel_spacing_col_mm / 2.0,
        ))
    } else {
        None
    };

    let options = DvhOptions {
        interpolation_resolution_mm,
        interpolation_segments_between_planes: z_segments,
        ..DvhOptions::default()
    };

    let requested_names = structures
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .collect::<Vec<_>>();

    let requested_set = if requested_names.is_empty() {
        None
    } else {
        Some(
            requested_names
                .iter()
                .map(|item| item.to_ascii_lowercase())
                .collect::<BTreeSet<_>>(),
        )
    };

    let mut warnings = Vec::new();
    let mut items = Vec::new();

    for roi in &rois {
        let include = requested_set
            .as_ref()
            .map(|set| set.contains(&roi.name.to_ascii_lowercase()))
            .unwrap_or(true);
        if !include {
            continue;
        }

        match DvhEngine::calculate_dvh(roi, &dose_grid, &options) {
            Ok(result) => {
                let base = to_json_format(&result);
                let total_cc = base.stats.total_cc;
                let volumes_pct = base
                    .volumes_cc
                    .iter()
                    .map(|value| {
                        if total_cc > 0.0 {
                            value / total_cc * 100.0
                        } else {
                            0.0
                        }
                    })
                    .collect::<Vec<_>>();

                items.push(DvhComputationItem {
                    roi_name: base.roi_name,
                    stats: base.stats,
                    doses_gy: base.doses_gy,
                    volumes_cc: base.volumes_cc,
                    volumes_pct,
                });
            }
            Err(error) => {
                warnings.push(format!("Failed to compute DVH for '{}': {error}", roi.name));
            }
        }
    }

    if !requested_names.is_empty() {
        let found = items
            .iter()
            .map(|item| item.roi_name.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();

        for requested in requested_names {
            if !found.contains(&requested.to_ascii_lowercase()) {
                warnings.push(format!("Requested structure not found: {requested}"));
            }
        }
    }

    Ok(DvhComputation { items, warnings })
}

fn validate_metric_specs(metrics: &[DvhMetricSpec]) -> Result<(), ApiError> {
    if metrics.is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "At least one metric query is required",
        ));
    }

    let mut ids = BTreeSet::new();
    for spec in metrics {
        if let Some(id) = &spec.id {
            if id.trim().is_empty() {
                return Err(ApiError::new(
                    ErrorCode::InvalidInput,
                    "Metric id values cannot be empty",
                ));
            }
            if !ids.insert(id.clone()) {
                return Err(ApiError::new(
                    ErrorCode::InvalidInput,
                    format!("Duplicate metric id: {id}"),
                ));
            }
        }

        match &spec.query {
            DvhMetricQuery::DoseAtVolume { volume_percent } => {
                if !(0.0..=100.0).contains(volume_percent) {
                    return Err(ApiError::new(
                        ErrorCode::InvalidInput,
                        format!("volume_percent must be within [0, 100], got {volume_percent}"),
                    ));
                }
            }
            DvhMetricQuery::VolumeAtDose { dose_gy, .. } => {
                if *dose_gy < 0.0 {
                    return Err(ApiError::new(
                        ErrorCode::InvalidInput,
                        format!("dose_gy must be >= 0, got {dose_gy}"),
                    ));
                }
            }
            DvhMetricQuery::Stat { .. } => {}
        }
    }

    Ok(())
}

fn resolve_metric_ids(metrics: &[DvhMetricSpec]) -> Result<Vec<String>, ApiError> {
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::with_capacity(metrics.len());

    for (index, spec) in metrics.iter().enumerate() {
        let id = spec.id.clone().unwrap_or_else(|| format!("m{}", index + 1));
        if !seen.insert(id.clone()) {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                format!("Duplicate metric id after normalization: {id}"),
            ));
        }
        resolved.push(id);
    }

    Ok(resolved)
}

fn evaluate_metric(
    spec: &DvhMetricSpec,
    item: &DvhComputationItem,
) -> Result<(f64, MetricUnit), ApiError> {
    match &spec.query {
        DvhMetricQuery::DoseAtVolume { volume_percent } => {
            let value = interpolate_dose_at_volume_percent(
                &item.doses_gy,
                &item.volumes_pct,
                *volume_percent,
            )
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::ComputeError,
                    format!(
                        "Unable to compute dose_at_volume metric for ROI '{}'",
                        item.roi_name
                    ),
                )
            })?;
            Ok((value, MetricUnit::Gy))
        }
        DvhMetricQuery::VolumeAtDose {
            dose_gy,
            volume_unit,
        } => {
            let value = match volume_unit {
                VolumeUnit::Percent => {
                    interpolate_volume_at_dose(&item.doses_gy, &item.volumes_pct, *dose_gy)
                }
                VolumeUnit::Cc => {
                    interpolate_volume_at_dose(&item.doses_gy, &item.volumes_cc, *dose_gy)
                }
            }
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::ComputeError,
                    format!(
                        "Unable to compute volume_at_dose metric for ROI '{}'",
                        item.roi_name
                    ),
                )
            })?;

            let unit = match volume_unit {
                VolumeUnit::Percent => MetricUnit::Percent,
                VolumeUnit::Cc => MetricUnit::Cc,
            };
            Ok((value, unit))
        }
        DvhMetricQuery::Stat { stat } => Ok(stat_value(&item.stats, *stat)),
    }
}

fn stat_value(stats: &DvhStats, stat: DvhStatField) -> (f64, MetricUnit) {
    match stat {
        DvhStatField::NBins => (stats.n_bins as f64, MetricUnit::Count),
        DvhStatField::TotalCc => (stats.total_cc, MetricUnit::Cc),
        DvhStatField::MinGy => (stats.min_gy, MetricUnit::Gy),
        DvhStatField::MaxGy => (stats.max_gy, MetricUnit::Gy),
        DvhStatField::MeanGy => (stats.mean_gy, MetricUnit::Gy),
        DvhStatField::D100Gy => (stats.d100_gy, MetricUnit::Gy),
        DvhStatField::D98Gy => (stats.d98_gy, MetricUnit::Gy),
        DvhStatField::D95Gy => (stats.d95_gy, MetricUnit::Gy),
        DvhStatField::D90Gy => (stats.d90_gy, MetricUnit::Gy),
        DvhStatField::D80Gy => (stats.d80_gy, MetricUnit::Gy),
        DvhStatField::D70Gy => (stats.d70_gy, MetricUnit::Gy),
        DvhStatField::D60Gy => (stats.d60_gy, MetricUnit::Gy),
        DvhStatField::D50Gy => (stats.d50_gy, MetricUnit::Gy),
        DvhStatField::D40Gy => (stats.d40_gy, MetricUnit::Gy),
        DvhStatField::D30Gy => (stats.d30_gy, MetricUnit::Gy),
        DvhStatField::D20Gy => (stats.d20_gy, MetricUnit::Gy),
        DvhStatField::D10Gy => (stats.d10_gy, MetricUnit::Gy),
        DvhStatField::D5Gy => (stats.d5_gy, MetricUnit::Gy),
        DvhStatField::D2Gy => (stats.d2_gy, MetricUnit::Gy),
        DvhStatField::D1Gy => (stats.d1_gy, MetricUnit::Gy),
        DvhStatField::D0Gy => (stats.d0_gy, MetricUnit::Gy),
        DvhStatField::HomogeneityIndex => (stats.homogeneity_index, MetricUnit::Ratio),
    }
}

fn interpolate_dose_at_volume_percent(
    doses_gy: &[f64],
    volumes_pct: &[f64],
    target_volume_percent: f64,
) -> Option<f64> {
    if doses_gy.len() != volumes_pct.len() || doses_gy.is_empty() {
        return None;
    }

    if target_volume_percent >= volumes_pct[0] {
        return Some(doses_gy[0]);
    }

    let last_index = volumes_pct.len() - 1;
    if target_volume_percent <= volumes_pct[last_index] {
        return Some(doses_gy[last_index]);
    }

    for index in 0..last_index {
        let v0 = volumes_pct[index];
        let v1 = volumes_pct[index + 1];
        if range_contains(v0, v1, target_volume_percent) {
            return Some(interpolate_x_from_y(
                doses_gy[index],
                doses_gy[index + 1],
                v0,
                v1,
                target_volume_percent,
            ));
        }
    }

    None
}

fn interpolate_volume_at_dose(
    doses_gy: &[f64],
    volumes: &[f64],
    target_dose_gy: f64,
) -> Option<f64> {
    if doses_gy.len() != volumes.len() || doses_gy.is_empty() {
        return None;
    }

    if target_dose_gy <= doses_gy[0] {
        return Some(volumes[0]);
    }

    let last_index = doses_gy.len() - 1;
    if target_dose_gy >= doses_gy[last_index] {
        return Some(volumes[last_index]);
    }

    for index in 0..last_index {
        let d0 = doses_gy[index];
        let d1 = doses_gy[index + 1];
        if range_contains(d0, d1, target_dose_gy) {
            return Some(interpolate_y_from_x(
                d0,
                d1,
                volumes[index],
                volumes[index + 1],
                target_dose_gy,
            ));
        }
    }

    None
}

fn range_contains(a: f64, b: f64, value: f64) -> bool {
    (a <= value && value <= b) || (b <= value && value <= a)
}

fn interpolate_x_from_y(x0: f64, x1: f64, y0: f64, y1: f64, y: f64) -> f64 {
    let delta = y1 - y0;
    if delta.abs() < f64::EPSILON {
        return x0;
    }
    let t = (y - y0) / delta;
    x0 + t * (x1 - x0)
}

fn interpolate_y_from_x(x0: f64, x1: f64, y0: f64, y1: f64, x: f64) -> f64 {
    let delta = x1 - x0;
    if delta.abs() < f64::EPSILON {
        return y0;
    }
    let t = (x - x0) / delta;
    y0 + t * (y1 - y0)
}

/// Generate `count` uniformly spaced indices from `[0, len)`, always including
/// the first and last index.
fn uniform_sample_indices(len: usize, count: usize) -> Vec<usize> {
    if count >= len {
        return (0..len).collect();
    }
    if count <= 1 {
        return vec![0];
    }
    let mut indices = Vec::with_capacity(count);
    for i in 0..count {
        let idx = (i as f64 * (len - 1) as f64 / (count - 1) as f64).round() as usize;
        indices.push(idx);
    }
    indices.dedup();
    indices
}

fn pick_indices(data: &[f64], indices: &[usize]) -> Vec<f64> {
    indices.iter().map(|&i| data[i]).collect()
}

fn round_vec(values: &mut [f64], factor: f64) {
    for v in values.iter_mut() {
        *v = (*v * factor).round() / factor;
    }
}
