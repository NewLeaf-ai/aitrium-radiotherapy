use crate::anonymize;
use crate::types::{ApiError, ErrorCode, RtAnonymizeMetadataRequest};
use serde_json::Value;

pub fn handle(arguments: Value) -> Result<Value, ApiError> {
    let request: RtAnonymizeMetadataRequest =
        serde_json::from_value(arguments).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidInput,
                format!("Invalid rt_anonymize_metadata input: {error}"),
            )
        })?;

    let output = anonymize::execute(request)?;
    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_anonymize_metadata output: {error}"),
        )
    })
}
