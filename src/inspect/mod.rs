mod dose_reader;
mod plan_reader;
pub mod scanner;
mod structure_reader;

use crate::inspect::dose_reader::read_dose_grid_metadata;
use crate::inspect::plan_reader::read_plan;
use crate::inspect::scanner::{scan_dicom_directory, DicomFileInfo, StudyBucket};
use crate::inspect::structure_reader::read_structures;
use crate::types::{
    ApiResult, RtInspectResponse, StudyInspection, StudyModalities, SCHEMA_VERSION,
};
use std::path::PathBuf;

pub fn inspect_directory(path: &str) -> ApiResult<RtInspectResponse> {
    let path = PathBuf::from(path);
    let scan = scan_dicom_directory(&path)?;

    let mut warnings = scan.warnings;
    let mut studies = Vec::new();

    for study in scan.studies {
        let (selected_rtstruct, selected_rtdose, selected_rtplan, match_warnings) =
            select_primary_paths(&study);
        warnings.extend(match_warnings);

        let structures = if let Some(ref rtstruct_path) = selected_rtstruct {
            match read_structures(rtstruct_path) {
                Ok(structures) => structures,
                Err(error) => {
                    warnings.push(format!(
                        "Failed to parse structures from {}: {}",
                        rtstruct_path.display(),
                        error.message
                    ));
                    Vec::new()
                }
            }
        } else {
            warnings.push(format!(
                "Study {} has no RTSTRUCT file",
                study.study_instance_uid
            ));
            Vec::new()
        };

        let mut plans = Vec::new();
        for plan in &study.rtplan_files {
            match read_plan(&plan.path) {
                Ok(parsed) => plans.push(parsed),
                Err(error) => warnings.push(format!(
                    "Failed to parse RTPLAN {}: {}",
                    plan.path.display(),
                    error.message
                )),
            }
        }

        let mut dose_grids = Vec::new();
        for dose in &study.rtdose_files {
            match read_dose_grid_metadata(&dose.path) {
                Ok(metadata) => dose_grids.push(metadata),
                Err(error) => warnings.push(format!(
                    "Failed to parse RTDOSE metadata {}: {}",
                    dose.path.display(),
                    error.message
                )),
            }
        }

        studies.push(StudyInspection {
            study_instance_uid: study.study_instance_uid,
            modalities: StudyModalities {
                ct: study.ct_files.len(),
                rtstruct: study.rtstruct_files.len(),
                rtplan: study.rtplan_files.len(),
                rtdose: study.rtdose_files.len(),
            },
            structures,
            plans,
            dose_grids,
            rtstruct_path: selected_rtstruct
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            rtdose_path: selected_rtdose
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            rtplan_path: selected_rtplan.map(|path| path.display().to_string()),
        });
    }

    Ok(RtInspectResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        total_files: scan.total_files,
        total_dicom_files: scan.total_dicom_files,
        studies,
        warnings,
    })
}

#[derive(Debug, Clone)]
struct MatchCandidate {
    rtstruct_path: PathBuf,
    rtdose_path: PathBuf,
    rtplan_path: Option<PathBuf>,
    score: i32,
}

fn select_primary_paths(
    study: &StudyBucket,
) -> (
    Option<PathBuf>,
    Option<PathBuf>,
    Option<PathBuf>,
    Vec<String>,
) {
    let mut warnings = Vec::new();
    let mut candidates = Vec::<MatchCandidate>::new();

    for dose in &study.rtdose_files {
        let plan_candidates = if dose.referenced_rt_plan_uids.is_empty() {
            study.rtplan_files.iter().collect::<Vec<_>>()
        } else {
            study
                .rtplan_files
                .iter()
                .filter(|plan| {
                    dose.referenced_rt_plan_uids
                        .iter()
                        .any(|uid| uid == &plan.sop_instance_uid)
                })
                .collect::<Vec<_>>()
        };

        if plan_candidates.is_empty() {
            for rtstruct in &study.rtstruct_files {
                candidates.push(MatchCandidate {
                    rtstruct_path: rtstruct.path.clone(),
                    rtdose_path: dose.path.clone(),
                    rtplan_path: None,
                    score: reference_score(rtstruct, dose, None),
                });
            }
            continue;
        }

        for plan in plan_candidates {
            let struct_candidates = if plan.referenced_struct_set_uids.is_empty() {
                study.rtstruct_files.iter().collect::<Vec<_>>()
            } else {
                study
                    .rtstruct_files
                    .iter()
                    .filter(|rtstruct| {
                        plan.referenced_struct_set_uids
                            .iter()
                            .any(|uid| uid == &rtstruct.sop_instance_uid)
                    })
                    .collect::<Vec<_>>()
            };

            if struct_candidates.is_empty() {
                continue;
            }

            for rtstruct in struct_candidates {
                candidates.push(MatchCandidate {
                    rtstruct_path: rtstruct.path.clone(),
                    rtdose_path: dose.path.clone(),
                    rtplan_path: Some(plan.path.clone()),
                    score: reference_score(rtstruct, dose, Some(plan)),
                });
            }
        }
    }

    if candidates.is_empty() {
        let fallback_rtstruct = study.rtstruct_files.first().map(|value| value.path.clone());
        let fallback_rtdose = study.rtdose_files.first().map(|value| value.path.clone());
        let fallback_rtplan = study.rtplan_files.first().map(|value| value.path.clone());

        if fallback_rtstruct.is_some() || fallback_rtdose.is_some() {
            warnings.push(format!(
                "Study {}: falling back to first-file pairing due to missing reference links",
                study.study_instance_uid
            ));
        }

        return (
            fallback_rtstruct,
            fallback_rtdose,
            fallback_rtplan,
            warnings,
        );
    }

    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.rtstruct_path.cmp(&b.rtstruct_path))
            .then(a.rtdose_path.cmp(&b.rtdose_path))
            .then(a.rtplan_path.cmp(&b.rtplan_path))
    });

    let best = candidates[0].clone();
    if best.score <= 0 {
        warnings.push(format!(
            "Study {}: selected pairing has weak reference confidence",
            study.study_instance_uid
        ));
    }

    (
        Some(best.rtstruct_path),
        Some(best.rtdose_path),
        best.rtplan_path,
        warnings,
    )
}

fn reference_score(
    rtstruct: &DicomFileInfo,
    dose: &DicomFileInfo,
    plan: Option<&DicomFileInfo>,
) -> i32 {
    let mut score = 0_i32;

    if let Some(plan) = plan {
        if dose
            .referenced_rt_plan_uids
            .iter()
            .any(|uid| uid == &plan.sop_instance_uid)
        {
            score += 4;
        }

        if plan
            .referenced_struct_set_uids
            .iter()
            .any(|uid| uid == &rtstruct.sop_instance_uid)
        {
            score += 4;
        }

        if let (Some(plan_frame), Some(struct_frame)) = (
            plan.frame_of_reference_uid.as_ref(),
            rtstruct.frame_of_reference_uid.as_ref(),
        ) {
            if plan_frame == struct_frame {
                score += 1;
            }
        }
    }

    if let (Some(struct_frame), Some(dose_frame)) = (
        rtstruct.frame_of_reference_uid.as_ref(),
        dose.frame_of_reference_uid.as_ref(),
    ) {
        if struct_frame == dose_frame {
            score += 2;
        }
    }

    if let Some(dose_frame) = dose.frame_of_reference_uid.as_ref() {
        if rtstruct
            .referenced_frame_of_reference_uids
            .iter()
            .any(|uid| uid == dose_frame)
        {
            score += 2;
        }
    }

    score
}
