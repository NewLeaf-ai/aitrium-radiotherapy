use aitrium_radiotherapy_server::tools::ToolRegistry;
use aitrium_radiotherapy_server::types::ErrorCode;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn lists_expected_tools() {
    let registry = ToolRegistry::new();
    let names = registry
        .list()
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "rt_inspect".to_string(),
            "rt_dvh".to_string(),
            "rt_dvh_metrics".to_string(),
            "rt_margin".to_string(),
            "rt_anonymize_metadata".to_string(),
            "rt_anonymize_template_get".to_string(),
            "rt_anonymize_template_update".to_string(),
            "rt_anonymize_template_reset".to_string()
        ]
    );
}

#[test]
fn inspect_returns_file_not_found_for_missing_directory() {
    let registry = ToolRegistry::new();
    let result = registry.call("rt_inspect", json!({"path": "/this/path/does/not/exist"}));

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::FileNotFound.to_string());
}

#[test]
fn dvh_returns_file_not_found_for_missing_files() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_dvh",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "rtdose_path": "/missing/rtdose.dcm"
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::FileNotFound.to_string());
}

#[test]
fn dvh_metrics_validates_required_metrics() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_dvh_metrics",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "rtdose_path": "/missing/rtdose.dcm",
          "metrics": []
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::InvalidInput.to_string());
}

#[test]
fn dvh_metrics_returns_file_not_found_for_missing_files() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_dvh_metrics",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "rtdose_path": "/missing/rtdose.dcm",
          "metrics": [{"type": "stat", "stat": "d95_gy"}]
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::FileNotFound.to_string());
}

#[test]
fn margin_validates_required_structure_names() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_margin",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "from_structure": "",
          "to_structure": "PTV"
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::InvalidInput.to_string());
}

#[test]
fn margin_returns_file_not_found_for_missing_files() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_margin",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "from_structure": "CTV",
          "to_structure": "PTV"
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::FileNotFound.to_string());
}

#[test]
fn margin_rejects_invalid_direction() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_margin",
        json!({
          "rtstruct_path": "/missing/rtstruct.dcm",
          "from_structure": "CTV",
          "to_structure": "PTV",
          "direction": "diagonal"
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::InvalidInput.to_string());
}

#[test]
fn anonymize_returns_file_not_found_for_missing_source() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_anonymize_metadata",
        json!({
          "source_path": "/missing/source/path"
        }),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::FileNotFound.to_string());
}

#[test]
fn anonymize_template_get_returns_effective_template() {
    let registry = ToolRegistry::new();
    let result = registry
        .call("rt_anonymize_template_get", json!({}))
        .expect("expected successful template get");

    assert_eq!(
        result
            .get("template_name")
            .and_then(serde_json::Value::as_str),
        Some("aitrium_template")
    );
    assert!(result.get("policy").is_some());
}

#[test]
fn anonymize_template_update_rejects_unknown_template_name() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_anonymize_template_update",
        json!({"template": "not_supported"}),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::InvalidInput.to_string());
}

#[test]
fn anonymize_template_reset_rejects_unknown_template_name() {
    let registry = ToolRegistry::new();
    let result = registry.call(
        "rt_anonymize_template_reset",
        json!({"template": "not_supported"}),
    );

    let error = result.expect_err("expected error");
    assert_eq!(error.code.to_string(), ErrorCode::InvalidInput.to_string());
}
