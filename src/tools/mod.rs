pub mod anonymize;
pub mod anonymize_template;
pub mod dvh;
pub mod inspect;
pub mod margin;

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
            ToolSpec {
                name: "rt_margin".to_string(),
                description: "Compute RTSTRUCT-only directed A->B boundary clearance with optional anatomical direction filtering (uniform/lateral/posterior/anterior/left/right/superior/inferior). Positive clearance means the source is inside the target with margin; coverage at threshold t means % of boundary samples with clearance >= t.".to_string(),
                input_schema: schema_from_file("../../schemas/rt_margin.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_margin.output.schema.json"),
            },
            ToolSpec {
                name: "rt_anonymize_metadata".to_string(),
                description: "Apply policy-driven DICOM metadata anonymization (metadata only; no pixel transformation) with dry-run/write modes".to_string(),
                input_schema: schema_from_file("../../schemas/rt_anonymize_metadata.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_anonymize_metadata.output.schema.json"),
            },
            ToolSpec {
                name: "rt_anonymize_template_get".to_string(),
                description: "Get effective runtime-editable anonymization template 'aitrium_template' (runtime copy or built-in fallback)".to_string(),
                input_schema: schema_from_file("../../schemas/rt_anonymize_template_get.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_anonymize_template_get.output.schema.json"),
            },
            ToolSpec {
                name: "rt_anonymize_template_update".to_string(),
                description: "Create/update runtime-editable anonymization template 'aitrium_template' using full policy or merged overrides".to_string(),
                input_schema: schema_from_file("../../schemas/rt_anonymize_template_update.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_anonymize_template_update.output.schema.json"),
            },
            ToolSpec {
                name: "rt_anonymize_template_reset".to_string(),
                description: "Reset runtime-editable anonymization template 'aitrium_template' by deleting custom copy and falling back to built-in default".to_string(),
                input_schema: schema_from_file("../../schemas/rt_anonymize_template_reset.input.schema.json"),
                output_schema: schema_from_file("../../schemas/rt_anonymize_template_reset.output.schema.json"),
            },
        ]
    }

    pub fn call(&self, name: &str, arguments: Value) -> ApiResult<Value> {
        match name {
            "rt_inspect" => inspect::handle(arguments),
            "rt_dvh" => dvh::handle(arguments),
            "rt_dvh_metrics" => dvh::handle_metrics(arguments),
            "rt_margin" => margin::handle(arguments),
            "rt_anonymize_metadata" => anonymize::handle(arguments),
            "rt_anonymize_template_get" => anonymize_template::handle_get(arguments),
            "rt_anonymize_template_update" => anonymize_template::handle_update(arguments),
            "rt_anonymize_template_reset" => anonymize_template::handle_reset(arguments),
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
        "../../schemas/rt_margin.input.schema.json" => {
            include_str!("../../schemas/rt_margin.input.schema.json")
        }
        "../../schemas/rt_margin.output.schema.json" => {
            include_str!("../../schemas/rt_margin.output.schema.json")
        }
        "../../schemas/rt_anonymize_metadata.input.schema.json" => {
            include_str!("../../schemas/rt_anonymize_metadata.input.schema.json")
        }
        "../../schemas/rt_anonymize_metadata.output.schema.json" => {
            include_str!("../../schemas/rt_anonymize_metadata.output.schema.json")
        }
        "../../schemas/rt_anonymize_template_get.input.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_get.input.schema.json")
        }
        "../../schemas/rt_anonymize_template_get.output.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_get.output.schema.json")
        }
        "../../schemas/rt_anonymize_template_update.input.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_update.input.schema.json")
        }
        "../../schemas/rt_anonymize_template_update.output.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_update.output.schema.json")
        }
        "../../schemas/rt_anonymize_template_reset.input.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_reset.input.schema.json")
        }
        "../../schemas/rt_anonymize_template_reset.output.schema.json" => {
            include_str!("../../schemas/rt_anonymize_template_reset.output.schema.json")
        }
        _ => "{}",
    };

    if let Ok(parsed) = serde_json::from_str::<Value>(selected) {
        cache = parsed;
    }

    cache
}
