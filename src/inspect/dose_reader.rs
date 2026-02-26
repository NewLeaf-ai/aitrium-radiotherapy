use crate::types::{ApiError, DoseGridDimensions, DoseGridInfo, DoseGridSpacing, ErrorCode};
use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::open_file;
use std::path::Path;

pub fn read_dose_grid_metadata(path: &Path) -> Result<DoseGridInfo, ApiError> {
    let obj = open_file(path).map_err(|error| {
        ApiError::new(
            ErrorCode::DicomParseError,
            format!("Failed to open RTDOSE {}: {error}", path.display()),
        )
    })?;

    let rows = parse_usize(&obj, tags::ROWS).unwrap_or(0);
    let cols = parse_usize(&obj, tags::COLUMNS).unwrap_or(0);
    let frames = parse_usize(&obj, tags::NUMBER_OF_FRAMES).unwrap_or(1);

    let pixel_spacing = parse_multi_f64(&obj, tags::PIXEL_SPACING);
    let spacing = DoseGridSpacing {
        row: pixel_spacing.first().copied().unwrap_or(0.0),
        col: pixel_spacing.get(1).copied().unwrap_or(0.0),
    };

    let dose_scaling_gy = parse_f64(&obj, tags::DOSE_GRID_SCALING).unwrap_or(0.0);

    let sop_instance_uid = obj
        .element(tags::SOP_INSTANCE_UID)
        .ok()
        .and_then(|element| element.to_str().ok())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.display().to_string());

    Ok(DoseGridInfo {
        sop_instance_uid,
        dimensions: DoseGridDimensions { rows, cols, frames },
        pixel_spacing_mm: spacing,
        dose_scaling_gy,
    })
}

fn parse_usize(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
    tag: Tag,
) -> Option<usize> {
    obj.element(tag)
        .ok()?
        .to_int::<i32>()
        .ok()
        .map(|value| value as usize)
}

fn parse_f64(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
    tag: Tag,
) -> Option<f64> {
    if let Ok(element) = obj.element(tag) {
        if let Ok(value) = element.to_float64() {
            return Some(value);
        }
        if let Ok(value) = element.to_float32() {
            return Some(value as f64);
        }
        if let Ok(value) = element.to_str() {
            if let Ok(parsed) = value.parse::<f64>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn parse_multi_f64(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
    tag: Tag,
) -> Vec<f64> {
    let Ok(element) = obj.element(tag) else {
        return Vec::new();
    };

    if let Ok(values) = element.to_multi_float64() {
        return values;
    }

    if let Ok(value) = element.to_str() {
        return value
            .split('\\')
            .filter_map(|part| part.parse::<f64>().ok())
            .collect();
    }

    Vec::new()
}
