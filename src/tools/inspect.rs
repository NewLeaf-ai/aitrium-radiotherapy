use crate::inspect;
use crate::types::{ApiError, ErrorCode, RtInspectRequest};
use serde_json::Value;

pub fn handle(arguments: Value) -> Result<Value, ApiError> {
    let request: RtInspectRequest = serde_json::from_value(arguments).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidInput,
            format!("Invalid rt_inspect input: {error}"),
        )
    })?;

    let output = inspect::inspect_directory(&request.path)?;
    serde_json::to_value(output).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("Failed to serialize rt_inspect output: {error}"),
        )
    })
}
