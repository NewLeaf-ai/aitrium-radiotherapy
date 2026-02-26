use anyhow::{bail, Context};
use aitrium_radiotherapy_server::self_test::{current_build_info, run_self_test, SelfTestReport};
use aitrium_radiotherapy_server::tools::ToolRegistry;
use aitrium_radiotherapy_server::transport::manual_jsonrpc::ManualJsonRpcTransport;
use aitrium_radiotherapy_server::transport::TransportAdapter;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return run_stdio_server();
    }

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
    println!("  aitrium-radiotherapy-server                 Start MCP stdio server");
    println!("  aitrium-radiotherapy-server serve-stdio     Start MCP stdio server");
    println!("  aitrium-radiotherapy-server --version        Print version");
    println!("  aitrium-radiotherapy-server --build-info [--json]");
    println!("  aitrium-radiotherapy-server self-test [--json]");
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
