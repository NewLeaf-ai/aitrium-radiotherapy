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

    // Fractionation: NumberOfFractionsPlanned from FractionGroupSequence
    let number_of_fractions = obj
        .element(Tag(0x300A, 0x0070)) // FractionGroupSequence
        .ok()
        .and_then(|seq| match seq.value() {
            DicomValue::Sequence(items) => items.items().first().and_then(|item| {
                item.element(Tag(0x300A, 0x0078)) // NumberOfFractionsPlanned
                    .ok()
                    .and_then(|e| e.to_int::<i32>().ok())
            }),
            _ => None,
        });

    // Plan date
    let plan_date = obj
        .element(Tag(0x300A, 0x0006))
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    // Plan geometry (PATIENT, TREATMENT_DEVICE, etc.)
    let plan_geometry = obj
        .element(Tag(0x300A, 0x000C))
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    // Beam information from BeamSequence
    let (radiation_type, beam_count, beam_types, beam_energies_mv) = extract_beam_info(&obj);

    Ok(PlanInfo {
        plan_name,
        sop_instance_uid,
        dose_references,
        number_of_fractions,
        plan_date,
        plan_geometry,
        radiation_type,
        beam_count,
        beam_types,
        beam_energies_mv,
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

fn extract_beam_info(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
) -> (Option<String>, Option<usize>, Option<Vec<String>>, Option<Vec<f64>>) {
    let Ok(sequence) = obj.element(Tag(0x300A, 0x00B0)) else {
        // BeamSequence not present
        return (None, None, None, None);
    };

    let DicomValue::Sequence(items) = sequence.value() else {
        return (None, None, None, None);
    };

    let beams = items.items();
    if beams.is_empty() {
        return (None, Some(0), None, None);
    }

    let mut types = std::collections::BTreeSet::new();
    let mut energies = std::collections::BTreeSet::new();
    let mut radiation = None;

    for beam in beams {
        // BeamType (tag 300A,00C4): STATIC, DYNAMIC
        if let Some(bt) = beam
            .element(Tag(0x300A, 0x00C4))
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            types.insert(bt);
        }

        // RadiationType (tag 300A,00C6): PHOTON, ELECTRON, etc.
        if radiation.is_none() {
            radiation = beam
                .element(Tag(0x300A, 0x00C6))
                .ok()
                .and_then(|e| e.to_str().ok())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
        }

        // NominalBeamEnergy (tag 300A,0114): in MV. Some plans store this on
        // the first control point rather than directly on the beam item.
        if let Some(energy) = parse_nominal_beam_energy(beam) {
            // Use ordered bits for BTreeSet<f64>
            energies.insert(energy.to_bits());
        }
    }

    let beam_types = if types.is_empty() {
        None
    } else {
        Some(types.into_iter().collect())
    };

    let beam_energies_mv = if energies.is_empty() {
        None
    } else {
        Some(energies.into_iter().map(f64::from_bits).collect())
    };

    (radiation, Some(beams.len()), beam_types, beam_energies_mv)
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

fn parse_nominal_beam_energy(beam: &dicom_object::InMemDicomObject) -> Option<f64> {
    if let Some(value) = parse_f64(beam, Tag(0x300A, 0x0114)) {
        return Some(value);
    }
    let sequence = beam.element(Tag(0x300A, 0x0111)).ok()?;
    let DicomValue::Sequence(items) = sequence.value() else {
        return None;
    };
    let first = items.items().first()?;
    parse_f64(first, Tag(0x300A, 0x0114))
}

#[cfg(test)]
mod tests {
    use super::read_plan;
    use dicom_core::value::DataSetSequence;
    use dicom_core::{DataElement, Tag, VR};
    use dicom_dictionary_std::tags;
    use dicom_dictionary_std::uids;
    use dicom_object::meta::FileMetaTableBuilder;
    use dicom_object::InMemDicomObject;

    #[test]
    fn read_plan_extracts_beam_energy_from_first_control_point() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plan_path = temp.path().join("rtplan.dcm");

        let fraction_group = InMemDicomObject::from_element_iter([DataElement::new(
            Tag(0x300A, 0x0078),
            VR::IS,
            "39",
        )]);
        let control_point = InMemDicomObject::from_element_iter([DataElement::new(
            Tag(0x300A, 0x0114),
            VR::DS,
            "8.0",
        )]);
        let beam = InMemDicomObject::from_element_iter([
            DataElement::new(Tag(0x300A, 0x00C4), VR::CS, "DYNAMIC"),
            DataElement::new(Tag(0x300A, 0x00C6), VR::CS, "PHOTON"),
            DataElement::new(
                Tag(0x300A, 0x0111),
                VR::SQ,
                DataSetSequence::from(vec![control_point]),
            ),
        ]);

        let dataset = InMemDicomObject::from_element_iter([
            DataElement::new(tags::MODALITY, VR::CS, "RTPLAN"),
            DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::RT_PLAN_STORAGE),
            DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, "1.2.826.0.1.3680043.2.1125.11"),
            DataElement::new(tags::STUDY_INSTANCE_UID, VR::UI, "1.2.826.0.1.3680043.2.1125.12"),
            DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, "1.2.826.0.1.3680043.2.1125.13"),
            DataElement::new(Tag(0x300A, 0x0003), VR::LO, "Pelvis"),
            DataElement::new(
                Tag(0x300A, 0x0070),
                VR::SQ,
                DataSetSequence::from(vec![fraction_group]),
            ),
            DataElement::new(
                Tag(0x300A, 0x00B0),
                VR::SQ,
                DataSetSequence::from(vec![beam]),
            ),
        ]);

        let file = dataset
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid(uids::RT_PLAN_STORAGE)
                    .media_storage_sop_instance_uid("1.2.826.0.1.3680043.2.1125.11"),
            )
            .expect("meta");
        file.write_to_file(&plan_path).expect("write rtplan");

        let plan = read_plan(&plan_path).expect("read plan");

        assert_eq!(plan.number_of_fractions, Some(39));
        assert_eq!(plan.beam_energies_mv, Some(vec![8.0]));
        assert_eq!(plan.radiation_type.as_deref(), Some("PHOTON"));
    }
}
