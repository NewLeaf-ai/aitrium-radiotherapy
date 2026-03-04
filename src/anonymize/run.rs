use crate::anonymize::engine::EngineContext;
use crate::anonymize::parse::{load_policy, normalize_optional_output, validate_source_path};
use crate::anonymize::policy::CompiledPolicy;
use crate::anonymize::report::ReportState;
use crate::anonymize::writer::{
    cleanup_write_plan, ensure_parent, finalize_write_plan, prepare_write_plan, resolve_target_root,
};
use crate::types::{
    ApiError, ApiResult, ErrorCode, RtAnonymizeMetadataRequest, RtAnonymizeMetadataResponse,
};
use dicom_dictionary_std::tags;
use dicom_object::{open_file, DefaultDicomObject};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn execute(request: RtAnonymizeMetadataRequest) -> ApiResult<RtAnonymizeMetadataResponse> {
    validate_source_path(&request.source_path)?;

    let output_path = normalize_optional_output(request.output_path.clone());
    let write_mode = !request.dry_run;

    let mut report = ReportState::new(
        if write_mode { "write" } else { "dry_run" },
        &request.source_path,
        output_path.clone(),
    );

    report.response.safety_checks.source_exists = Path::new(&request.source_path).exists();
    report.response.safety_checks.source_is_directory = Path::new(&request.source_path).is_dir();
    report.response.safety_checks.fail_closed = request.fail_on_error;

    if request.max_workers > 1 {
        report.push_warning(
            "max_workers is currently advisory; processing occurs sequentially in this release",
        );
    }

    let policy = load_policy(&request)?;
    let compiled = CompiledPolicy::compile(policy)?;
    let mut engine = EngineContext::new(
        compiled,
        request.include_trace,
        request.deterministic_uid_secret.as_deref(),
    )?;
    let mut reserved_dicom_targets = HashSet::new();

    let source_root = PathBuf::from(&request.source_path);

    if write_mode {
        let output = output_path.clone().ok_or_else(|| {
            ApiError::new(
                ErrorCode::InvalidInput,
                "output_path is required when dry_run=false",
            )
        })?;

        let plan = prepare_write_plan(
            &source_root,
            Path::new(&output),
            request.allow_existing_output,
        )?;

        report
            .response
            .safety_checks
            .output_is_new_or_explicit_override =
            !Path::new(&output).exists() || request.allow_existing_output;
        report.response.safety_checks.output_not_source = plan.destination_root != source_root;
        report.response.safety_checks.output_not_inside_source =
            !plan.destination_root.starts_with(&source_root);

        let target_root = resolve_target_root(&plan).to_path_buf();
        let processing = process_dataset(
            &source_root,
            &target_root,
            true,
            &mut engine,
            &mut report,
            request.fail_on_error,
            &mut reserved_dicom_targets,
        );

        if let Err(error) = processing {
            cleanup_write_plan(&plan);
            return Err(error);
        }

        if let Err(error) = finalize_write_plan(&plan) {
            cleanup_write_plan(&plan);
            return Err(error);
        }
    } else {
        if let Some(output) = output_path {
            let output_path = PathBuf::from(output);
            report.response.safety_checks.output_not_source = output_path != source_root;
            report.response.safety_checks.output_not_inside_source =
                !output_path.starts_with(&source_root);
        } else {
            report.response.safety_checks.output_not_source = true;
            report.response.safety_checks.output_not_inside_source = true;
        }
        report
            .response
            .safety_checks
            .output_is_new_or_explicit_override = true;

        process_dataset(
            &source_root,
            Path::new(""),
            false,
            &mut engine,
            &mut report,
            request.fail_on_error,
            &mut reserved_dicom_targets,
        )?;
    }

    Ok(report.finish())
}

fn process_dataset(
    source_root: &Path,
    target_root: &Path,
    write_mode: bool,
    engine: &mut EngineContext,
    report: &mut ReportState,
    fail_on_error: bool,
    reserved_dicom_targets: &mut HashSet<PathBuf>,
) -> Result<(), ApiError> {
    for entry in WalkDir::new(source_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let source_path = entry.path().to_path_buf();
        report.response.source_summary.total_files += 1;

        let relative_path = source_path.strip_prefix(source_root).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!(
                    "Failed to compute relative path for '{}': {error}",
                    source_path.display()
                ),
            )
        })?;

        let process_result: Result<(), ApiError> = match open_file(&source_path) {
            Ok(file) => {
                report.response.source_summary.dicom_files += 1;
                let transformed =
                    engine.transform_file(file, report, &relative_path.display().to_string())?;

                if write_mode {
                    let target_path = build_dicom_target_path(
                        target_root,
                        relative_path,
                        &transformed,
                        report,
                        reserved_dicom_targets,
                    );
                    ensure_parent(&target_path)?;
                    transformed.write_to_file(&target_path).map_err(|error| {
                        ApiError::new(
                            ErrorCode::Internal,
                            format!(
                                "Failed to write anonymized DICOM '{}': {error}",
                                target_path.display()
                            ),
                        )
                    })?;
                    report.response.output_summary.files_written += 1;
                    report.response.output_summary.dicom_written += 1;
                }
                Ok(())
            }
            Err(_) => {
                report.response.source_summary.non_dicom_files += 1;
                if write_mode {
                    let target_path = target_root.join(relative_path);
                    ensure_parent(&target_path)?;
                    fs::copy(&source_path, &target_path).map_err(|error| {
                        ApiError::new(
                            ErrorCode::Internal,
                            format!(
                                "Failed to copy non-DICOM file '{}' to '{}': {error}",
                                source_path.display(),
                                target_path.display()
                            ),
                        )
                    })?;
                    report.response.output_summary.files_written += 1;
                    report.response.output_summary.non_dicom_copied += 1;
                }
                Ok(())
            }
        };

        if let Err(error) = process_result {
            if fail_on_error {
                return Err(error);
            }
            report.push_error(error.message);
        }
    }

    if !fail_on_error && !report.response.errors.is_empty() {
        report.push_warning("Processing completed with errors in best-effort mode");
    }

    Ok(())
}

fn build_dicom_target_path(
    target_root: &Path,
    relative_path: &Path,
    transformed: &DefaultDicomObject,
    report: &mut ReportState,
    reserved_dicom_targets: &mut HashSet<PathBuf>,
) -> PathBuf {
    let parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let modality_raw = transformed
        .element(tags::MODALITY)
        .ok()
        .and_then(|elem| elem.value().to_str().ok().map(|value| value.into_owned()))
        .unwrap_or_default();
    let modality = sanitize_filename_token(&modality_raw, "UNKMOD");
    if modality_raw.trim().is_empty() {
        report.push_warning(
            "Missing Modality for a DICOM file; using UNKMOD in anonymized output filename",
        );
    }

    let sop_uid_raw = transformed
        .element(tags::SOP_INSTANCE_UID)
        .ok()
        .and_then(|elem| elem.value().to_str().ok().map(|value| value.into_owned()))
        .unwrap_or_default();
    let sop_uid = if sop_uid_raw.trim().is_empty() {
        report.push_warning(
            "Missing SOPInstanceUID for a DICOM file; using generated placeholder in anonymized output filename",
        );
        format!(
            "missinguid{}",
            report.response.output_summary.dicom_written + 1
        )
    } else {
        sanitize_filename_token(&sop_uid_raw, "missinguid")
    };

    let base_name = format!("{modality}.{sop_uid}.dcm");
    let mut candidate = target_root.join(parent).join(base_name);

    if reserved_dicom_targets.contains(&candidate) || candidate.exists() {
        report.push_warning(
            "DICOM output filename collision detected; appending numeric suffix to maintain uniqueness",
        );
        let mut index: u64 = 1;
        loop {
            let fallback_name = format!("{modality}.{sop_uid}.{index}.dcm");
            let fallback_candidate = target_root.join(parent).join(fallback_name);
            if !reserved_dicom_targets.contains(&fallback_candidate) && !fallback_candidate.exists()
            {
                candidate = fallback_candidate;
                break;
            }
            index += 1;
        }
    }

    reserved_dicom_targets.insert(candidate.clone());
    candidate
}

fn sanitize_filename_token(raw: &str, fallback: &str) -> String {
    let sanitized: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}
