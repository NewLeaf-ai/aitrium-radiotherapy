pub mod dvh;
pub mod inspect;

use crate::types::{ApiError, ApiResult, ErrorCode, ToolSpec};
use serde_json::Value;

#[derive(Debug, Default)]
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn list(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "rt_inspect".to_string(),
                description: "Scan DICOM RT datasets and return structured study, structure, plan, and dose metadata".to_string(),
                input_schema: schema_from_file("../../schemas/rt_inspect.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_inspect.output.schema.json"),
            },
            ToolSpec {
                name: "rt_dvh".to_string(),
                description: "Compute DVHs for one RTSTRUCT + RTDOSE pair with optional structure filtering. Use max_points and precision to reduce output size when include_curves is true.".to_string(),
                input_schema: schema_from_file("../../schemas/rt_dvh.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_dvh.output.schema.json"),
            },
            ToolSpec {
                name: "rt_dvh_metrics".to_string(),
                description: "Compute targeted DVH metrics (D@V, V@D, stat fields) with compact output".to_string(),
                input_schema: schema_from_file("../../schemas/rt_dvh_metrics.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_dvh_metrics.output.schema.json"),
            },
        ]
    }

    pub fn call(&self, name: &str, arguments: Value) -> ApiResult<Value> {
        match name {
            "rt_inspect" => inspect::handle(arguments),
            "rt_dvh" => dvh::handle(arguments),
            "rt_dvh_metrics" => dvh::handle_metrics(arguments),
            _ => Err(ApiError::new(
                ErrorCode::InvalidInput,
                format!("Unknown tool: {name}"),
            )),
        }
    }
}

fn schema_from_file(path: &str) -> Value {
    let raw = include_str!("../../schemas/rt_inspect.input.schema.json");
    let mut cache = serde_json::json!({});
    // Static include selection keeps schemas colocated with files and avoids runtime I/O.
    let selected = match path {
        "../../schemas/rt_inspect.input.schema.json" => raw,
        "../../schemas/rt_inspect.output.schema.json" => {
            include_str!("../../schemas/rt_inspect.output.schema.json")
        }
        "../../schemas/rt_dvh.input.schema.json" => {
            include_str!("../../schemas/rt_dvh.input.schema.json")
        }
        "../../schemas/rt_dvh.output.schema.json" => {
            include_str!("../../schemas/rt_dvh.output.schema.json")
        }
        "../../schemas/rt_dvh_metrics.input.schema.json" => {
            include_str!("../../schemas/rt_dvh_metrics.input.schema.json")
        }
        "../../schemas/rt_dvh_metrics.output.schema.json" => {
            include_str!("../../schemas/rt_dvh_metrics.output.schema.json")
        }
        _ => "{}",
    };

    if let Ok(parsed) = serde_json::from_str::<Value>(selected) {
        cache = parsed;
    }

    cache
}
