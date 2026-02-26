use crate::types::{ApiError, DoseReference, ErrorCode, PlanInfo};
use dicom_core::value::Value as DicomValue;
use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::open_file;
use std::path::Path;

pub fn read_plan(path: &Path) -> Result<PlanInfo, ApiError> {
    let obj = open_file(path).map_err(|error| {
        ApiError::new(
            ErrorCode::DicomParseError,
            format!("Failed to open RTPLAN {}: {error}", path.display()),
        )
    })?;

    let plan_name = obj
        .element(Tag(0x300A, 0x0003))
        .ok()
        .and_then(|element| element.to_str().ok())
        .map(|value| value.to_string());

    let sop_instance_uid = obj
        .element(tags::SOP_INSTANCE_UID)
        .ok()
        .and_then(|element| element.to_str().ok())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path.display().to_string());

    let dose_references = extract_dose_references(&obj);

    Ok(PlanInfo {
        plan_name,
        sop_instance_uid,
        dose_references,
    })
}

fn extract_dose_references(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
) -> Vec<DoseReference> {
    let mut output = Vec::new();

    let Ok(sequence) = obj.element(Tag(0x300A, 0x0010)) else {
        return output;
    };

    if let DicomValue::Sequence(items) = sequence.value() {
        for item in items.items() {
            output.push(DoseReference {
                reference_type: item
                    .element(Tag(0x300A, 0x0020))
                    .ok()
                    .and_then(|element| element.to_str().ok())
                    .map(|value| value.to_string()),
                structure_type: item
                    .element(Tag(0x300A, 0x0014))
                    .ok()
                    .and_then(|element| element.to_str().ok())
                    .map(|value| value.to_string()),
                prescription_dose_gy: parse_f64(item, Tag(0x300A, 0x0026)),
                referenced_roi_number: item
                    .element(Tag(0x3006, 0x0084))
                    .ok()
                    .and_then(|element| element.to_int::<i32>().ok()),
            });
        }
    }

    output
}

fn parse_f64(obj: &dicom_object::InMemDicomObject, tag: Tag) -> Option<f64> {
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
