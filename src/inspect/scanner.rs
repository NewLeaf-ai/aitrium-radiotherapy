use crate::types::{ApiError, ErrorCode};
use dicom_core::value::Value as DicomValue;
use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::open_file;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DicomFileInfo {
    pub path: PathBuf,
    pub modality: String,
    pub study_instance_uid: String,
    pub sop_instance_uid: String,
    pub frame_of_reference_uid: Option<String>,
    pub referenced_frame_of_reference_uids: Vec<String>,
    pub referenced_rt_plan_uids: Vec<String>,
    pub referenced_struct_set_uids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StudyBucket {
    pub study_instance_uid: String,
    pub ct_files: Vec<DicomFileInfo>,
    pub rtstruct_files: Vec<DicomFileInfo>,
    pub rtdose_files: Vec<DicomFileInfo>,
    pub rtplan_files: Vec<DicomFileInfo>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub total_files: u64,
    pub total_dicom_files: u64,
    pub studies: Vec<StudyBucket>,
    pub warnings: Vec<String>,
}

pub fn scan_dicom_directory(path: &Path) -> Result<ScanResult, ApiError> {
    if !path.exists() {
        return Err(ApiError::new(
            ErrorCode::FileNotFound,
            format!("Directory not found: {}", path.display()),
        ));
    }
    if !path.is_dir() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!("Path is not a directory: {}", path.display()),
        ));
    }

    let mut total_files = 0_u64;
    let mut total_dicom_files = 0_u64;
    let mut warnings = Vec::new();
    let mut studies: BTreeMap<String, StudyBucket> = BTreeMap::new();

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        total_files += 1;
        let file_path = entry.path();

        if file_path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("._"))
        {
            continue;
        }

        let info = match read_dicom_file_info(file_path) {
            Ok(Some(info)) => info,
            Ok(None) => continue,
            Err(error) => {
                warnings.push(format!(
                    "Failed to inspect DICOM metadata for {}: {}",
                    file_path.display(),
                    error
                ));
                continue;
            }
        };

        total_dicom_files += 1;

        let study = studies
            .entry(info.study_instance_uid.clone())
            .or_insert_with(|| StudyBucket {
                study_instance_uid: info.study_instance_uid.clone(),
                ct_files: Vec::new(),
                rtstruct_files: Vec::new(),
                rtdose_files: Vec::new(),
                rtplan_files: Vec::new(),
            });

        match info.modality.as_str() {
            "CT" => study.ct_files.push(info),
            "RTSTRUCT" => study.rtstruct_files.push(info),
            "RTDOSE" => study.rtdose_files.push(info),
            "RTPLAN" => study.rtplan_files.push(info),
            _ => {}
        }
    }

    let mut grouped = studies.into_values().collect::<Vec<_>>();
    grouped.sort_by(|a, b| a.study_instance_uid.cmp(&b.study_instance_uid));

    Ok(ScanResult {
        total_files,
        total_dicom_files,
        studies: grouped,
        warnings,
    })
}

fn read_dicom_file_info(path: &Path) -> Result<Option<DicomFileInfo>, String> {
    let obj = match open_file(path) {
        Ok(obj) => obj,
        Err(_) => return Ok(None),
    };

    let modality = match element_string(&obj, tags::MODALITY) {
        Some(value) => value,
        None => return Ok(None),
    };

    let study_instance_uid = element_string(&obj, tags::STUDY_INSTANCE_UID)
        .ok_or_else(|| "Missing StudyInstanceUID".to_string())?;
    let sop_instance_uid = element_string(&obj, tags::SOP_INSTANCE_UID)
        .ok_or_else(|| "Missing SOPInstanceUID".to_string())?;

    let frame_of_reference_uid = element_string(&obj, tags::FRAME_OF_REFERENCE_UID);

    let referenced_frame_of_reference_uids = extract_sequence_uids(
        &obj,
        tags::REFERENCED_FRAME_OF_REFERENCE_SEQUENCE,
        tags::FRAME_OF_REFERENCE_UID,
    );

    let referenced_rt_plan_uids =
        extract_sequence_uids(&obj, tags::REFERENCED_RT_PLAN_SEQUENCE, Tag(0x0008, 0x1155));

    let referenced_struct_set_uids = extract_sequence_uids(
        &obj,
        tags::REFERENCED_STRUCTURE_SET_SEQUENCE,
        Tag(0x0008, 0x1155),
    );

    Ok(Some(DicomFileInfo {
        path: path.to_path_buf(),
        modality,
        study_instance_uid,
        sop_instance_uid,
        frame_of_reference_uid,
        referenced_frame_of_reference_uids,
        referenced_rt_plan_uids,
        referenced_struct_set_uids,
    }))
}

fn element_string(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
    tag: Tag,
) -> Option<String> {
    obj.element(tag)
        .ok()?
        .to_str()
        .ok()
        .map(|value| value.trim().to_string())
}

fn extract_sequence_uids(
    obj: &dicom_object::FileDicomObject<dicom_object::InMemDicomObject>,
    sequence_tag: Tag,
    uid_tag: Tag,
) -> Vec<String> {
    let mut values = Vec::new();

    let Ok(sequence_elem) = obj.element(sequence_tag) else {
        return values;
    };

    if let DicomValue::Sequence(sequence) = sequence_elem.value() {
        for item in sequence.items() {
            if let Some(value) = item
                .element(uid_tag)
                .ok()
                .and_then(|element| element.to_str().ok())
                .map(|uid| uid.to_string())
            {
                values.push(value);
            }
        }
    }

    values
}
