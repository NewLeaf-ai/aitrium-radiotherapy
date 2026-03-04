use crate::anonymize::parse::{
    get_runtime_template, reset_runtime_template, update_runtime_template,
};
use crate::types::{
    ApiError, ErrorCode, RtAnonymizeTemplateGetRequest, RtAnonymizeTemplateResetRequest,
    RtAnonymizeTemplateUpdateRequest,
};
use serde_json::Value;

pub fn handle_get(arguments: Value) -> Result<Value, ApiError> {
    let request: RtAnonymizeTemplateGetRequest =
        serde_json::from_value(arguments).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidInput,
                format!("Invalid rt_anonymize_template_get input: {error}"),
            )
        })?;

    let output = get_runtime_template(request)?;
    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_anonymize_template_get output: {error}"),
        )
    })
}

pub fn handle_update(arguments: Value) -> Result<Value, ApiError> {
    let request: RtAnonymizeTemplateUpdateRequest =
        serde_json::from_value(arguments).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidInput,
                format!("Invalid rt_anonymize_template_update input: {error}"),
            )
        })?;

    let output = update_runtime_template(request)?;
    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_anonymize_template_update output: {error}"),
        )
    })
}

pub fn handle_reset(arguments: Value) -> Result<Value, ApiError> {
    let request: RtAnonymizeTemplateResetRequest =
        serde_json::from_value(arguments).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidInput,
                format!("Invalid rt_anonymize_template_reset input: {error}"),
            )
        })?;

    let output = reset_runtime_template(request)?;
    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_anonymize_template_reset output: {error}"),
        )
    })
}
