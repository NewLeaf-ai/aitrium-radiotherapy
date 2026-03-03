use anyhow::{bail, Context};
use aitrium_radiotherapy_server::self_test::{current_build_info, run_self_test, SelfTestReport};
use aitrium_radiotherapy_server::tools::ToolRegistry;
use aitrium_radiotherapy_server::transport::manual_jsonrpc::ManualJsonRpcTransport;
use aitrium_radiotherapy_server::transport::TransportAdapter;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return run_stdio_server();
    }

    let registry = ToolRegistry::new();

    match args[0].as_str() {
        "--version" | "-V" => {
            if args.len() > 1 {
                bail!("Unexpected arguments for --version");
            }
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--build-info" => {
            let json = args.get(1).map(|v| v.as_str()) == Some("--json");
            if args.len() > 2 || (!json && args.len() > 1) {
                bail!("Usage: aitrium-radiotherapy-server --build-info [--json]");
            }
            print_build_info(json)
        }
        "self-test" => {
            let json = args.get(1).map(|v| v.as_str()) == Some("--json");
            if args.len() > 2 || (!json && args.len() > 1) {
                bail!("Usage: aitrium-radiotherapy-server self-test [--json]");
            }
            let report = run_self_test().context("Self-test execution failed")?;
            print_self_test_report(&report, json)?;
            if report.passed {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
        "serve-stdio" => {
            if args.len() > 1 {
                bail!("Usage: aitrium-radiotherapy-server serve-stdio");
            }
            run_stdio_server()
        }
        "inspect" => run_cli_inspect(&registry, &args[1..]),
        "dvh" => run_cli_dvh(&registry, &args[1..]),
        "dvh-metrics" | "dvh_metrics" => run_cli_dvh_metrics(&registry, &args[1..]),
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        unknown => {
            bail!(
                "Unknown command '{}'. Run 'aitrium-radiotherapy-server --help' for usage.",
                unknown
            );
        }
    }
}

fn print_help() {
    println!("aitrium-radiotherapy-server {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  aitrium-radiotherapy-server                           Start MCP stdio server");
    println!("  aitrium-radiotherapy-server serve-stdio               Start MCP stdio server");
    println!("  aitrium-radiotherapy-server --version                  Print version");
    println!("  aitrium-radiotherapy-server --build-info [--json]      Print build info");
    println!("  aitrium-radiotherapy-server self-test [--json]         Run runtime self-test");
    println!("  aitrium-radiotherapy-server inspect --path <dir>");
    println!("  aitrium-radiotherapy-server dvh --rtstruct <RS.dcm> --rtdose <RD.dcm> [options]");
    println!("  aitrium-radiotherapy-server dvh-metrics --rtstruct <RS.dcm> --rtdose <RD.dcm> [options]");
    println!();
    println!("dvh options:");
    println!("  --structures <name1,name2>      Comma-separated structure names");
    println!("  --structure <name>              Repeatable structure name");
    println!("  --interpolation [true|false]    Enable XY interpolation (default false)");
    println!("  --z-segments <N>                Interpolation segments between dose planes (default 0)");
    println!("  --include-curves [true|false]   Include DVH arrays (default false)");
    println!("  --max-points <N>                Downsample curve points");
    println!("  --precision <N>                 Round curve values to N decimals");
    println!();
    println!("dvh-metrics options:");
    println!("  --structures <name1,name2>      Comma-separated structure names");
    println!("  --structure <name>              Repeatable structure name");
    println!("  --interpolation [true|false]    Enable XY interpolation (default false)");
    println!("  --z-segments <N>                Interpolation segments between dose planes (default 0)");
    println!("  --metrics-json '<json-array>'   Metrics as JSON array");
    println!("  --metrics-file <path.json>      Metrics JSON file");
    println!("  --metric <expr>                 Repeatable compact metric expression");
    println!();
    println!("Metric expression examples:");
    println!("  --metric 'd95=dav:95'");
    println!("  --metric 'v20=vad:20:percent'");
    println!("  --metric 'mean=stat:mean_gy'");
}

#[derive(Debug, Default)]
struct CliArgs {
    positionals: Vec<String>,
    values: BTreeMap<String, Vec<String>>,
    flags: BTreeSet<String>,
}

impl CliArgs {
    fn parse(tokens: &[String]) -> anyhow::Result<Self> {
        let mut parsed = Self::default();
        let mut i = 0usize;

        while i < tokens.len() {
            let token = &tokens[i];
            if let Some(name) = token.strip_prefix("--") {
                if name.is_empty() {
                    bail!("Invalid empty flag: {}", token);
                }

                if let Some((key, value)) = name.split_once('=') {
                    if key.is_empty() {
                        bail!("Invalid flag syntax: {}", token);
                    }
                    parsed
                        .values
                        .entry(key.to_string())
                        .or_default()
                        .push(value.to_string());
                    i += 1;
                    continue;
                }

                if i + 1 < tokens.len() && !tokens[i + 1].starts_with("--") {
                    parsed
                        .values
                        .entry(name.to_string())
                        .or_default()
                        .push(tokens[i + 1].clone());
                    i += 2;
                    continue;
                }

                parsed.flags.insert(name.to_string());
                i += 1;
            } else {
                parsed.positionals.push(token.clone());
                i += 1;
            }
        }

        Ok(parsed)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|values| values.last())
            .map(|value| value.as_str())
    }

    fn values(&self, name: &str) -> Vec<&str> {
        self.values
            .get(name)
            .map(|values| values.iter().map(|value| value.as_str()).collect())
            .unwrap_or_default()
    }

    fn parse_bool(&self, name: &str, default: bool) -> anyhow::Result<bool> {
        match (self.flag(name), self.value(name)) {
            (_, Some(raw)) => parse_bool_literal(raw).with_context(|| {
                format!("Invalid boolean value for --{name}: '{raw}'. Use true/false")
            }),
            (true, None) => Ok(true),
            (false, None) => Ok(default),
        }
    }

    fn parse_u32(&self, name: &str) -> anyhow::Result<Option<u32>> {
        match self.value(name) {
            Some(raw) => Ok(Some(raw.parse::<u32>().with_context(|| {
                format!("Invalid integer value for --{name}: '{raw}'")
            })?)),
            None => Ok(None),
        }
    }

    fn parse_u8(&self, name: &str) -> anyhow::Result<Option<u8>> {
        match self.value(name) {
            Some(raw) => Ok(Some(raw.parse::<u8>().with_context(|| {
                format!("Invalid integer value for --{name}: '{raw}'")
            })?)),
            None => Ok(None),
        }
    }
}

fn parse_bool_literal(input: &str) -> anyhow::Result<bool> {
    match input.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        _ => bail!("unsupported boolean literal"),
    }
}

fn collect_structures(args: &CliArgs) -> Vec<String> {
    let mut output = Vec::new();

    for item in args.values("structure") {
        let trimmed = item.trim();
        if !trimmed.is_empty() {
            output.push(trimmed.to_string());
        }
    }

    if let Some(csv) = args.value("structures") {
        output.extend(
            csv.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned),
        );
    }

    output
}

fn run_cli_inspect(registry: &ToolRegistry, tokens: &[String]) -> anyhow::Result<()> {
    let args = CliArgs::parse(tokens)?;
    let path = args
        .value("path")
        .map(ToOwned::to_owned)
        .or_else(|| args.positionals.first().cloned())
        .context("inspect requires --path <dicom_dir> or a positional directory path")?;

    if args.positionals.len() > 1 {
        bail!("Too many positional arguments for inspect");
    }

    execute_cli_tool(registry, "rt_inspect", json!({"path": path}))
}

fn run_cli_dvh(registry: &ToolRegistry, tokens: &[String]) -> anyhow::Result<()> {
    let args = CliArgs::parse(tokens)?;

    let rtstruct_path = args
        .value("rtstruct")
        .map(ToOwned::to_owned)
        .or_else(|| args.positionals.first().cloned())
        .context("dvh requires --rtstruct <path> (or first positional)")?;

    let rtdose_path = args
        .value("rtdose")
        .map(ToOwned::to_owned)
        .or_else(|| args.positionals.get(1).cloned())
        .context("dvh requires --rtdose <path> (or second positional)")?;

    if args.positionals.len() > 2 {
        bail!("Too many positional arguments for dvh");
    }

    let interpolation = args.parse_bool("interpolation", false)?;
    let include_curves = args.parse_bool("include-curves", false)?;
    let z_segments = args.parse_u32("z-segments")?.unwrap_or(0);
    let max_points = args.parse_u32("max-points")?;
    let precision = args.parse_u8("precision")?;
    let structures = collect_structures(&args);

    let mut payload = Map::new();
    payload.insert("rtstruct_path".to_string(), Value::String(rtstruct_path));
    payload.insert("rtdose_path".to_string(), Value::String(rtdose_path));
    payload.insert("interpolation".to_string(), Value::Bool(interpolation));
    payload.insert("z_segments".to_string(), Value::from(z_segments));
    payload.insert("include_curves".to_string(), Value::Bool(include_curves));

    if !structures.is_empty() {
        payload.insert("structures".to_string(), json!(structures));
    }
    if let Some(value) = max_points {
        payload.insert("max_points".to_string(), Value::from(value));
    }
    if let Some(value) = precision {
        payload.insert("precision".to_string(), Value::from(value));
    }

    execute_cli_tool(registry, "rt_dvh", Value::Object(payload))
}

fn run_cli_dvh_metrics(registry: &ToolRegistry, tokens: &[String]) -> anyhow::Result<()> {
    let args = CliArgs::parse(tokens)?;

    let rtstruct_path = args
        .value("rtstruct")
        .map(ToOwned::to_owned)
        .or_else(|| args.positionals.first().cloned())
        .context("dvh-metrics requires --rtstruct <path> (or first positional)")?;

    let rtdose_path = args
        .value("rtdose")
        .map(ToOwned::to_owned)
        .or_else(|| args.positionals.get(1).cloned())
        .context("dvh-metrics requires --rtdose <path> (or second positional)")?;

    if args.positionals.len() > 2 {
        bail!("Too many positional arguments for dvh-metrics");
    }

    let interpolation = args.parse_bool("interpolation", false)?;
    let z_segments = args.parse_u32("z-segments")?.unwrap_or(0);
    let structures = collect_structures(&args);
    let metrics = parse_metric_specs(&args)?;

    let mut payload = Map::new();
    payload.insert("rtstruct_path".to_string(), Value::String(rtstruct_path));
    payload.insert("rtdose_path".to_string(), Value::String(rtdose_path));
    payload.insert("interpolation".to_string(), Value::Bool(interpolation));
    payload.insert("z_segments".to_string(), Value::from(z_segments));
    payload.insert("metrics".to_string(), Value::Array(metrics));

    if !structures.is_empty() {
        payload.insert("structures".to_string(), json!(structures));
    }

    execute_cli_tool(registry, "rt_dvh_metrics", Value::Object(payload))
}

fn parse_metric_specs(args: &CliArgs) -> anyhow::Result<Vec<Value>> {
    let mut sources = 0usize;
    if args.value("metrics-json").is_some() {
        sources += 1;
    }
    if args.value("metrics-file").is_some() {
        sources += 1;
    }
    if !args.values("metric").is_empty() {
        sources += 1;
    }

    if sources == 0 {
        bail!(
            "dvh-metrics requires one metric source: --metrics-json, --metrics-file, or --metric"
        );
    }
    if sources > 1 {
        bail!(
            "Use exactly one of --metrics-json, --metrics-file, or --metric (repeatable)"
        );
    }

    if let Some(raw) = args.value("metrics-json") {
        return parse_metrics_json(raw, "--metrics-json");
    }

    if let Some(path) = args.value("metrics-file") {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read metrics file: {path}"))?;
        return parse_metrics_json(&contents, "--metrics-file");
    }

    let mut output = Vec::new();
    for expression in args.values("metric") {
        output.push(parse_metric_expression(expression)?);
    }
    Ok(output)
}

fn parse_metrics_json(raw: &str, source: &str) -> anyhow::Result<Vec<Value>> {
    let value: Value =
        serde_json::from_str(raw).with_context(|| format!("Invalid JSON in {source}"))?;
    let array = value
        .as_array()
        .with_context(|| format!("{source} must be a JSON array"))?;
    if array.is_empty() {
        bail!("{source} must contain at least one metric entry");
    }
    Ok(array.clone())
}

fn parse_metric_expression(expression: &str) -> anyhow::Result<Value> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        bail!("Empty --metric expression");
    }

    let (id, body) = if let Some((lhs, rhs)) = trimmed.split_once('=') {
        let parsed_id = lhs.trim();
        if parsed_id.is_empty() {
            bail!("Metric id cannot be empty in expression: {trimmed}");
        }
        (Some(parsed_id.to_string()), rhs.trim())
    } else {
        (None, trimmed)
    };

    let mut parts = body.split(':');
    let metric_type = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("Metric expression missing type")?;

    let mut metric = Map::new();
    if let Some(id) = id {
        metric.insert("id".to_string(), Value::String(id));
    }

    match metric_type {
        "dav" | "dose_at_volume" => {
            let volume_percent = parts
                .next()
                .context("dose_at_volume metric requires a volume percent")?
                .trim()
                .parse::<f64>()
                .with_context(|| format!("Invalid volume percent in metric: {trimmed}"))?;
            metric.insert("type".to_string(), Value::String("dose_at_volume".to_string()));
            metric.insert("volume_percent".to_string(), Value::from(volume_percent));
        }
        "vad" | "volume_at_dose" => {
            let dose_gy = parts
                .next()
                .context("volume_at_dose metric requires dose_gy")?
                .trim()
                .parse::<f64>()
                .with_context(|| format!("Invalid dose_gy in metric: {trimmed}"))?;
            let volume_unit = parts.next().map(str::trim).unwrap_or("percent");
            if volume_unit != "percent" && volume_unit != "cc" {
                bail!(
                    "Invalid volume unit '{}' in metric '{}'; use 'percent' or 'cc'",
                    volume_unit,
                    trimmed
                );
            }
            metric.insert("type".to_string(), Value::String("volume_at_dose".to_string()));
            metric.insert("dose_gy".to_string(), Value::from(dose_gy));
            metric.insert(
                "volume_unit".to_string(),
                Value::String(volume_unit.to_string()),
            );
        }
        "stat" => {
            let field = parts
                .next()
                .context("stat metric requires a stat field")?
                .trim();
            metric.insert("type".to_string(), Value::String("stat".to_string()));
            metric.insert("stat".to_string(), Value::String(field.to_string()));
        }
        other => {
            bail!(
                "Unsupported metric type '{}' in '{}'. Use dav, vad, or stat",
                other,
                trimmed
            )
        }
    }

    if parts.next().is_some() {
        bail!("Too many ':' segments in metric expression: {trimmed}");
    }

    Ok(Value::Object(metric))
}

fn execute_cli_tool(registry: &ToolRegistry, tool_name: &str, arguments: Value) -> anyhow::Result<()> {
    match registry.call(tool_name, arguments) {
        Ok(output) => {
            println!("{}", serde_json::to_string_pretty(&output)?);
            Ok(())
        }
        Err(error) => {
            eprintln!("{}", serde_json::to_string_pretty(&error)?);
            std::process::exit(2);
        }
    }
}

fn print_build_info(as_json: bool) -> anyhow::Result<()> {
    let info = current_build_info();
    if as_json {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    println!("name={}", info.name);
    println!("version={}", info.version);
    println!("transport_default={}", info.transport_default);
    println!("commit_sha={}", info.commit_sha);
    println!("build_id={}", info.build_id);
    Ok(())
}

fn print_self_test_report(report: &SelfTestReport, as_json: bool) -> anyhow::Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!(
        "Self-test {} ({} checks)",
        if report.passed { "PASSED" } else { "FAILED" },
        report.checks.len()
    );
    for check in &report.checks {
        println!(
            "- {:<30} {} ({})",
            check.id,
            if check.passed { "ok" } else { "failed" },
            check.detail
        );
    }
    Ok(())
}

fn run_stdio_server() -> anyhow::Result<()> {
    env_logger::Builder::from_default_env()
        .format_target(false)
        .filter_level(log::LevelFilter::Info)
        .init();

    let registry = ToolRegistry::new();
    let transport =
        std::env::var("AITRIUM_RADIOTHERAPY_TRANSPORT").unwrap_or_else(|_| "manual_jsonrpc".to_string());

    match transport.as_str() {
        "manual_jsonrpc" | "manual" => ManualJsonRpcTransport.run(&registry),
        "mcp_crate" => {
            log::warn!(
                "AITRIUM_RADIOTHERAPY_TRANSPORT=mcp_crate requested; MCP crate adapter is pending spike outcome. Falling back to manual_jsonrpc."
            );
            ManualJsonRpcTransport.run(&registry)
        }
        other => {
            log::warn!("Unknown transport '{other}'. Falling back to manual_jsonrpc.");
            ManualJsonRpcTransport.run(&registry)
        }
    }
}
