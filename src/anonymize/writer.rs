use crate::types::{ApiError, ErrorCode};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WritePlan {
    pub destination_root: PathBuf,
    pub staging_root: Option<PathBuf>,
}

pub fn prepare_write_plan(
    source_root: &Path,
    output_root: &Path,
    allow_existing_output: bool,
) -> Result<WritePlan, ApiError> {
    let source = canonicalize_existing(source_root)?;
    let output_parent = output_root.parent().unwrap_or_else(|| Path::new("."));
    if !output_parent.exists() {
        fs::create_dir_all(output_parent).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidInput,
                format!(
                    "Failed to create output parent directory '{}': {error}",
                    output_parent.display()
                ),
            )
        })?;
    }

    let output = if output_root.exists() {
        canonicalize_existing(output_root)?
    } else {
        output_root.to_path_buf()
    };

    if source == output {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "Output path must be different from source path",
        ));
    }

    if output.starts_with(&source) {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            "Output path cannot be inside source path",
        ));
    }

    if output.exists() {
        if !allow_existing_output {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                format!(
                    "Output path '{}' already exists. Use allow_existing_output to override explicitly",
                    output.display()
                ),
            ));
        }

        return Ok(WritePlan {
            destination_root: output,
            staging_root: None,
        });
    }

    let staging_root = output.with_extension(format!("tmp-anonymize-{}", std::process::id()));

    if staging_root.exists() {
        fs::remove_dir_all(&staging_root).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!(
                    "Failed to clean stale staging path '{}': {error}",
                    staging_root.display()
                ),
            )
        })?;
    }

    fs::create_dir_all(&staging_root).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!(
                "Failed to create staging path '{}': {error}",
                staging_root.display()
            ),
        )
    })?;

    Ok(WritePlan {
        destination_root: output,
        staging_root: Some(staging_root),
    })
}

pub fn finalize_write_plan(plan: &WritePlan) -> Result<(), ApiError> {
    let Some(staging) = &plan.staging_root else {
        return Ok(());
    };

    if plan.destination_root.exists() {
        return Err(ApiError::new(
            ErrorCode::Internal,
            format!(
                "Destination path already exists at finalize: {}",
                plan.destination_root.display()
            ),
        ));
    }

    fs::rename(staging, &plan.destination_root).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!(
                "Failed to promote staging '{}' to destination '{}': {error}",
                staging.display(),
                plan.destination_root.display()
            ),
        )
    })
}

pub fn cleanup_write_plan(plan: &WritePlan) {
    if let Some(staging) = &plan.staging_root {
        let _ = fs::remove_dir_all(staging);
    }
}

pub fn resolve_target_root(plan: &WritePlan) -> &Path {
    if let Some(staging) = &plan.staging_root {
        staging.as_path()
    } else {
        plan.destination_root.as_path()
    }
}

pub fn ensure_parent(path: &Path) -> Result<(), ApiError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to create directory '{}': {error}", parent.display()),
        )
    })
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, ApiError> {
    fs::canonicalize(path).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Failed to canonicalize '{}': {error}", path.display()),
        )
    })
}
