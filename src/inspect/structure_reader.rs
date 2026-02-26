use crate::types::{ApiError, ErrorCode, ROITypeCategory, StructureInfo};
use dicom_core::value::Value as DicomValue;
use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::open_file;
use std::collections::HashMap;
use std::path::Path;

pub fn read_structures(rtstruct_path: &Path) -> Result<Vec<StructureInfo>, ApiError> {
    let obj = open_file(rtstruct_path).map_err(|error| {
        ApiError::new(
            ErrorCode::DicomParseError,
            format!(
                "Failed to open RTSTRUCT {}: {error}",
                rtstruct_path.display()
            ),
        )
    })?;

    let observation_types = extract_observation_types(&obj);

    let mut structures = Vec::new();
    if let Ok(sequence) = obj.element(tags::STRUCTURE_SET_ROI_SEQUENCE) {
        if let DicomValue::Sequence(items) = sequence.value() {
            for item in items.items() {
                let roi_number = item
                    .element(tags::ROI_NUMBER)
                    .ok()
                    .and_then(|element| element.to_int::<i32>().ok())
                    .unwrap_or_default();

                let name = item
                    .element(tags::ROI_NAME)
                    .ok()
                    .and_then(|element| element.to_str().ok())
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| format!("ROI_{roi_number}"));

                let volume_cc = parse_f64(item, tags::ROI_VOLUME);
                let observation_type = observation_types.get(&roi_number).cloned();
                let category = categorize_roi_type(observation_type.as_deref(), &name);

                structures.push(StructureInfo {
                    roi_number,
                    name,
                    category,
                    observation_type,
                    volume_cc,
                });
            }
        }
    }

    structures.sort_by(|a, b| a.roi_number.cmp(&b.roi_number).then(a.name.cmp(&b.name)));

    Ok(structures)
}

fn extract_observation_types(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
) -> HashMap<i32, String> {
    let mut observations = HashMap::new();

    let Ok(sequence) = obj.element(Tag(0x3006, 0x0080)) else {
        return observations;
    };

    if let DicomValue::Sequence(items) = sequence.value() {
        for item in items.items() {
            let Some(roi_number) = item
                .element(Tag(0x3006, 0x0084))
                .ok()
                .and_then(|element| element.to_int::<i32>().ok())
            else {
                continue;
            };

            if let Some(observation_type) = item
                .element(Tag(0x3006, 0x00A4))
                .ok()
                .and_then(|element| element.to_str().ok())
                .map(|value| value.to_string())
            {
                observations.insert(roi_number, observation_type);
            }
        }
    }

    observations
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

fn categorize_roi_type(rt_roi_interpreted_type: Option<&str>, roi_name: &str) -> ROITypeCategory {
    let name_lower = roi_name.to_ascii_lowercase();

    let is_target_name = name_lower.starts_with("ptv")
        || name_lower.starts_with("ctv")
        || name_lower.starts_with("gtv")
        || name_lower.starts_with("itv")
        || name_lower.ends_with("ptv")
        || name_lower.ends_with("ctv")
        || name_lower.ends_with("gtv")
        || name_lower.ends_with("itv")
        || name_lower.contains("_ptv")
        || name_lower.contains("ptv_")
        || name_lower.contains("-ptv")
        || name_lower.contains("ptv-")
        || name_lower.contains("_ctv")
        || name_lower.contains("ctv_")
        || name_lower.contains("-ctv")
        || name_lower.contains("ctv-")
        || name_lower.contains("_gtv")
        || name_lower.contains("gtv_")
        || name_lower.contains("-gtv")
        || name_lower.contains("gtv-")
        || name_lower.contains("_itv")
        || name_lower.contains("itv_")
        || name_lower.contains("-itv")
        || name_lower.contains("itv-")
        || name_lower.starts_with("boost")
        || name_lower.starts_with("target");

    if is_target_name {
        return ROITypeCategory::Target;
    }

    if let Some(observation) = rt_roi_interpreted_type {
        match observation.to_ascii_lowercase().as_str() {
            "ptv" | "ctv" | "gtv" | "itv" | "treated_volume" | "irrad_volume" => {
                return ROITypeCategory::Target
            }
            "support" | "avoidance" | "avoid" => return ROITypeCategory::Device,
            "marker" | "bolus" | "registration" | "isocenter" | "fixation" | "cavity"
            | "contrast_agent" | "brachy_channel" | "brachy_accessory" | "brachy_src_app"
            | "brachy_chnl_shld" | "dose_region" | "control" | "dose_measurement" => {
                return ROITypeCategory::Device
            }
            "oar" | "organ" => return ROITypeCategory::Organ,
            "external" => return ROITypeCategory::External,
            _ => {}
        }
    }

    if name_lower.contains("body") || name_lower.contains("external") || name_lower == "ext" {
        ROITypeCategory::External
    } else if name_lower.contains("couch")
        || name_lower.contains("table")
        || name_lower.contains("support")
        || name_lower.contains("avoidance")
        || name_lower.contains("avoid")
        || name_lower.contains("bolus")
        || name_lower.contains("marker")
        || name_lower.contains("fiducial")
    {
        ROITypeCategory::Device
    } else {
        ROITypeCategory::Other
    }
}
