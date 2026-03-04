use crate::anonymize::policy::AnonymizationPolicy;
use crate::anonymize::validate::validate_policy;
use crate::types::{
    ApiError, ErrorCode, RtAnonymizeMetadataRequest, RtAnonymizeTemplateGetRequest,
    RtAnonymizeTemplateGetResponse, RtAnonymizeTemplateResetRequest,
    RtAnonymizeTemplateResetResponse, RtAnonymizeTemplateUpdateRequest,
    RtAnonymizeTemplateUpdateResponse, SCHEMA_VERSION,
};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const STRICT_TEMPLATE: &str = include_str!("../../policies/templates/strict_phi_safe.yaml");
const RESEARCH_TEMPLATE: &str = include_str!("../../policies/templates/research_balanced.yaml");
const MINIMAL_TEMPLATE: &str = include_str!("../../policies/templates/minimal_explicit.yaml");
const AITRIUM_DEFAULT_TEMPLATE: &str =
    include_str!("../../policies/templates/aitrium_default.yaml");
pub const AITRIUM_TEMPLATE_ALIAS: &str = "aitrium_template";
const RUNTIME_POLICY_DIR_ENV: &str = "AITRIUM_RT_POLICY_DIR";

pub fn template_names() -> &'static [&'static str] {
    &[
        "strict_phi_safe",
        "research_balanced",
        "minimal_explicit",
        "aitrium_default",
        AITRIUM_TEMPLATE_ALIAS,
    ]
}

pub fn load_policy(request: &RtAnonymizeMetadataRequest) -> Result<AnonymizationPolicy, ApiError> {
    let mut base_value = if let Some(inline) = &request.policy {
        inline.clone()
    } else if let Some(path) = &request.policy_path {
        parse_policy_text(&fs::read_to_string(path).map_err(|error| {
            ApiError::new(
                ErrorCode::FileNotFound,
                format!("Failed to read policy file '{path}': {error}"),
            )
        })?)?
    } else {
        template_value(request.template.as_deref().unwrap_or("strict_phi_safe"))?
    };

    if let Some(overrides) = &request.policy_overrides {
        deep_merge(&mut base_value, overrides.clone());
    }

    let policy: AnonymizationPolicy = serde_json::from_value(base_value).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid anonymization policy: {error}"),
        )
    })?;

    validate_policy(&policy)?;
    Ok(policy)
}

fn template_value(name: &str) -> Result<Value, ApiError> {
    if name == AITRIUM_TEMPLATE_ALIAS {
        let (value, _) = load_runtime_template_effective_value()?;
        return Ok(value);
    }

    parse_policy_text(built_in_template_raw(name)?)
}

fn parse_policy_text(raw: &str) -> Result<Value, ApiError> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return Ok(value);
    }

    serde_yaml::from_str::<Value>(raw).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Policy is neither valid JSON nor YAML: {error}"),
        )
    })
}

pub fn parse_policy_json_string(raw: &str) -> Result<Value, ApiError> {
    parse_policy_text(raw)
}

pub fn get_runtime_template(
    request: RtAnonymizeTemplateGetRequest,
) -> Result<RtAnonymizeTemplateGetResponse, ApiError> {
    let template_name = normalize_runtime_template_name(request.template.as_deref())?;
    let template_path = runtime_template_path();
    let (value, source) = load_runtime_template_effective_value()?;
    let policy = policy_from_value(value)?;

    Ok(RtAnonymizeTemplateGetResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        template_name: template_name.to_string(),
        template_path: template_path.display().to_string(),
        source: source.to_string(),
        policy: serialize_policy_json(&policy)?,
        warnings: Vec::new(),
    })
}

pub fn update_runtime_template(
    request: RtAnonymizeTemplateUpdateRequest,
) -> Result<RtAnonymizeTemplateUpdateResponse, ApiError> {
    let template_name = normalize_runtime_template_name(request.template.as_deref())?;
    let template_path = runtime_template_path();

    let mut base_value = if let Some(policy) = request.policy {
        policy
    } else if template_path.exists() {
        parse_policy_file(&template_path)?
    } else {
        parse_policy_text(AITRIUM_DEFAULT_TEMPLATE)?
    };

    if let Some(overrides) = request.policy_overrides {
        deep_merge(&mut base_value, overrides);
    }

    let policy = policy_from_value(base_value)?;
    write_runtime_template_atomic(&template_path, &policy)?;

    Ok(RtAnonymizeTemplateUpdateResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        template_name: template_name.to_string(),
        template_path: template_path.display().to_string(),
        source: "runtime".to_string(),
        policy: serialize_policy_json(&policy)?,
        warnings: Vec::new(),
    })
}

pub fn reset_runtime_template(
    request: RtAnonymizeTemplateResetRequest,
) -> Result<RtAnonymizeTemplateResetResponse, ApiError> {
    let template_name = normalize_runtime_template_name(request.template.as_deref())?;
    let template_path = runtime_template_path();
    let deleted = if template_path.exists() {
        fs::remove_file(&template_path).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!(
                    "Failed to remove runtime template '{}': {error}",
                    template_path.display()
                ),
            )
        })?;
        true
    } else {
        false
    };

    Ok(RtAnonymizeTemplateResetResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        template_name: template_name.to_string(),
        template_path: template_path.display().to_string(),
        deleted,
        source_after_reset: "built_in_fallback".to_string(),
        warnings: Vec::new(),
    })
}

pub fn normalize_optional_output(path: Option<String>) -> Option<String> {
    path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty())
}

pub fn validate_source_path(path: &str) -> Result<(), ApiError> {
    let source = Path::new(path);
    if !source.exists() {
        return Err(ApiError::new(
            ErrorCode::FileNotFound,
            format!("Source path not found: {}", source.display()),
        ));
    }
    if !source.is_dir() {
        return Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!("Source path is not a directory: {}", source.display()),
        ));
    }
    Ok(())
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        (target, value) => {
            *target = value;
        }
    }
}

fn built_in_template_raw(name: &str) -> Result<&'static str, ApiError> {
    match name {
        "strict_phi_safe" => Ok(STRICT_TEMPLATE),
        "research_balanced" => Ok(RESEARCH_TEMPLATE),
        "minimal_explicit" => Ok(MINIMAL_TEMPLATE),
        "aitrium_default" => Ok(AITRIUM_DEFAULT_TEMPLATE),
        _ => Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!("Unknown built-in template '{name}'"),
        )),
    }
}

fn normalize_runtime_template_name(input: Option<&str>) -> Result<&'static str, ApiError> {
    let template = input.unwrap_or(AITRIUM_TEMPLATE_ALIAS).trim();
    if template == AITRIUM_TEMPLATE_ALIAS {
        Ok(AITRIUM_TEMPLATE_ALIAS)
    } else {
        Err(ApiError::new(
            ErrorCode::InvalidInput,
            format!(
                "Unsupported runtime template '{template}'. Only '{AITRIUM_TEMPLATE_ALIAS}' is editable at runtime"
            ),
        ))
    }
}

fn runtime_template_path() -> PathBuf {
    runtime_policy_dir().join(format!("{AITRIUM_TEMPLATE_ALIAS}.yaml"))
}

fn runtime_policy_dir() -> PathBuf {
    if let Ok(custom) = env::var(RUNTIME_POLICY_DIR_ENV) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join("aitrium-radiotherapy")
                .join("policies");
        }
    }

    if let Ok(home) = env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join(".config")
                .join("aitrium-radiotherapy")
                .join("policies");
        }
    }

    PathBuf::from(".aitrium-radiotherapy").join("policies")
}

fn load_runtime_template_effective_value() -> Result<(Value, &'static str), ApiError> {
    let path = runtime_template_path();
    if path.exists() {
        Ok((parse_policy_file(&path)?, "runtime"))
    } else {
        Ok((
            parse_policy_text(AITRIUM_DEFAULT_TEMPLATE)?,
            "built_in_fallback",
        ))
    }
}

fn parse_policy_file(path: &Path) -> Result<Value, ApiError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        ApiError::new(
            ErrorCode::FileNotFound,
            format!("Failed to read policy file '{}': {error}", path.display()),
        )
    })?;
    parse_policy_text(&raw)
}

fn policy_from_value(value: Value) -> Result<AnonymizationPolicy, ApiError> {
    let policy: AnonymizationPolicy = serde_json::from_value(value).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid anonymization policy: {error}"),
        )
    })?;
    validate_policy(&policy)?;
    Ok(policy)
}

fn serialize_policy_json(policy: &AnonymizationPolicy) -> Result<Value, ApiError> {
    serde_json::to_value(policy).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize anonymization policy: {error}"),
        )
    })
}

fn write_runtime_template_atomic(
    path: &Path,
    policy: &AnonymizationPolicy,
) -> Result<(), ApiError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!(
                    "Failed to create runtime policy directory '{}': {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let serialized = serde_yaml::to_string(policy).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize runtime template YAML: {error}"),
        )
    })?;

    let temp_path = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp_path, serialized).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!(
                "Failed to write runtime template temp file '{}': {error}",
                temp_path.display()
            ),
        )
    })?;

    match fs::rename(&temp_path, path) {
        Ok(_) => Ok(()),
        Err(initial_error) => {
            if path.exists() {
                fs::remove_file(path).map_err(|remove_error| {
                    let _ = fs::remove_file(&temp_path);
                    ApiError::new(
                        ErrorCode::Internal,
                        format!(
                            "Failed to replace existing runtime template '{}': initial rename error: {initial_error}; remove error: {remove_error}",
                            path.display()
                        ),
                    )
                })?;
                fs::rename(&temp_path, path).map_err(|rename_error| {
                    let _ = fs::remove_file(&temp_path);
                    ApiError::new(
                        ErrorCode::Internal,
                        format!(
                            "Failed to finalize runtime template write '{}': {rename_error}",
                            path.display()
                        ),
                    )
                })
            } else {
                let _ = fs::remove_file(&temp_path);
                Err(ApiError::new(
                    ErrorCode::Internal,
                    format!(
                        "Failed to finalize runtime template write '{}': {initial_error}",
                        path.display()
                    ),
                ))
            }
        }
    }
}
