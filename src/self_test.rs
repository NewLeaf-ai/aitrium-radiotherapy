use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const SELF_TEST_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub name: String,
    pub version: String,
    pub transport_default: String,
    pub commit_sha: String,
    pub build_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestSummary {
    pub passed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestReport {
    pub schema_version: String,
    pub passed: bool,
    pub server: BuildInfo,
    pub checks: Vec<SelfTestCheck>,
    pub summary: SelfTestSummary,
}

#[derive(Debug)]
struct CommandOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

pub fn current_build_info() -> BuildInfo {
    BuildInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        transport_default: "manual_jsonrpc".to_string(),
        commit_sha: option_env!("AITRIUM_RADIOTHERAPY_COMMIT_SHA")
            .unwrap_or("unknown")
            .to_string(),
        build_id: option_env!("AITRIUM_RADIOTHERAPY_BUILD_ID")
            .unwrap_or("local")
            .to_string(),
    }
}

pub fn run_self_test() -> Result<SelfTestReport> {
    let exe = std::env::current_exe().context("Unable to determine current executable path")?;
    let mut checks = Vec::new();

    push_check(&mut checks, "version_flag", || check_version_flag(&exe));
    push_check(&mut checks, "initialize_newline", || {
        check_initialize_newline(&exe)
    });
    push_check(
        &mut checks,
        "initialize_framed_content_length_first",
        || check_initialize_framed(&exe, true),
    );
    push_check(&mut checks, "initialize_framed_content_type_first", || {
        check_initialize_framed(&exe, false)
    });
    push_check(&mut checks, "tools_list_expected", || {
        check_tools_list(&exe)
    });

    let passed_count = checks.iter().filter(|check| check.passed).count();
    let failed_count = checks.len().saturating_sub(passed_count);

    Ok(SelfTestReport {
        schema_version: SELF_TEST_SCHEMA_VERSION.to_string(),
        passed: failed_count == 0,
        server: current_build_info(),
        checks,
        summary: SelfTestSummary {
            passed: passed_count,
            failed: failed_count,
        },
    })
}

fn push_check<F>(checks: &mut Vec<SelfTestCheck>, id: &str, check: F)
where
    F: FnOnce() -> Result<String>,
{
    let (passed, detail) = match check() {
        Ok(detail) => (true, detail),
        Err(error) => (false, error.to_string()),
    };

    checks.push(SelfTestCheck {
        id: id.to_string(),
        passed,
        detail,
    });
}

fn check_version_flag(exe: &Path) -> Result<String> {
    let output = run_command_with_input(exe, &["--version"], b"", Duration::from_secs(4))?;
    if output.exit_code != 0 {
        bail!(
            "Expected exit code 0 for --version, got {} (stderr: {})",
            output.exit_code,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version != env!("CARGO_PKG_VERSION") {
        bail!(
            "Unexpected --version output '{}', expected '{}'",
            version,
            env!("CARGO_PKG_VERSION")
        );
    }

    Ok(format!("version={version}"))
}

fn check_initialize_newline(exe: &Path) -> Result<String> {
    let input = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
"#;
    let output = run_command_with_input(exe, &["serve-stdio"], input, Duration::from_secs(6))?;
    let responses = parse_newline_responses(&output.stdout)?;
    let response = responses
        .first()
        .context("No response received for newline initialize")?;
    assert_initialize_response(response)?;
    Ok("newline initialize succeeded".to_string())
}

fn check_initialize_framed(exe: &Path, content_length_first: bool) -> Result<String> {
    let request_body = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let framed_request = build_framed_request(request_body, content_length_first);
    let output = run_command_with_input(
        exe,
        &["serve-stdio"],
        &framed_request,
        Duration::from_secs(6),
    )?;

    let response = parse_framed_response(&output.stdout)?;
    assert_initialize_response(&response)?;
    Ok(if content_length_first {
        "framed initialize (Content-Length first) succeeded".to_string()
    } else {
        "framed initialize (Content-Type first) succeeded".to_string()
    })
}

fn check_tools_list(exe: &Path) -> Result<String> {
    let input = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
"#;
    let output = run_command_with_input(exe, &["serve-stdio"], input, Duration::from_secs(6))?;
    let responses = parse_newline_responses(&output.stdout)?;
    let response = responses
        .iter()
        .find(|value| value.get("id") == Some(&Value::from(2)))
        .context("Missing tools/list response with id=2")?;

    let tools = response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .context("tools/list response missing result.tools[]")?;

    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();

    for required in ["rt_inspect", "rt_dvh", "rt_dvh_metrics"] {
        if !tool_names.contains(required) {
            bail!("tools/list missing required tool '{required}'");
        }
    }

    Ok(format!("tools/list returned {}", tool_names.len()))
}

fn assert_initialize_response(response: &Value) -> Result<()> {
    let protocol = response
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        .context("initialize response missing result.protocolVersion")?;
    if protocol != MCP_PROTOCOL_VERSION {
        bail!(
            "Unexpected protocolVersion '{}', expected '{}'",
            protocol,
            MCP_PROTOCOL_VERSION
        );
    }

    let server_name = response
        .pointer("/result/serverInfo/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if server_name != "aitrium-radiotherapy-server" {
        bail!(
            "Unexpected initialize serverInfo.name '{}', expected 'aitrium-radiotherapy-server'",
            server_name
        );
    }
    Ok(())
}

fn build_framed_request(request_body: &[u8], content_length_first: bool) -> Vec<u8> {
    let mut payload = Vec::new();
    let content_length = request_body.len();
    if content_length_first {
        payload.extend_from_slice(format!("Content-Length: {content_length}\r\n").as_bytes());
        payload.extend_from_slice(b"Content-Type: application/json\r\n");
    } else {
        payload.extend_from_slice(b"Content-Type: application/json\r\n");
        payload.extend_from_slice(format!("Content-Length: {content_length}\r\n").as_bytes());
    }
    payload.extend_from_slice(b"\r\n");
    payload.extend_from_slice(request_body);
    payload
}

fn run_command_with_input(
    exe: &Path,
    args: &[&str],
    input: &[u8],
    timeout: Duration,
) -> Result<CommandOutput> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to launch '{}'", exe.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input)
            .with_context(|| format!("Failed writing stdin for '{}'", exe.display()))?;
    }

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("Failed to query process status")? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "Command '{}' timed out after {}s",
                exe.display(),
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(10));
    };

    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_end(&mut stdout)
            .context("Failed reading process stdout")?;
    }
    let mut stderr = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        err.read_to_end(&mut stderr)
            .context("Failed reading process stderr")?;
    }

    Ok(CommandOutput {
        exit_code: status.code().unwrap_or(1),
        stdout,
        stderr,
    })
}

fn parse_newline_responses(stdout: &[u8]) -> Result<Vec<Value>> {
    let text = String::from_utf8(stdout.to_vec()).context("Response stdout is not valid UTF-8")?;
    let mut responses = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        responses.push(
            serde_json::from_str::<Value>(trimmed)
                .with_context(|| format!("Invalid JSON response line: {trimmed}"))?,
        );
    }

    if responses.is_empty() {
        bail!("No JSON response lines were returned");
    }
    Ok(responses)
}

fn parse_framed_response(stdout: &[u8]) -> Result<Value> {
    let header_end = stdout
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("Framed response missing CRLF header delimiter")?;

    let header_bytes = &stdout[..header_end];
    let body = &stdout[(header_end + 4)..];
    let header_text = std::str::from_utf8(header_bytes).context("Invalid UTF-8 in frame header")?;

    let mut content_length = None;
    for line in header_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("Content-Length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("Failed parsing framed response Content-Length")?,
            );
        }
    }

    let length = content_length.context("Framed response missing Content-Length header")?;
    if body.len() < length {
        bail!(
            "Framed response body shorter than Content-Length (len={}, body={})",
            length,
            body.len()
        );
    }

    serde_json::from_slice::<Value>(&body[..length]).context("Framed response body is not JSON")
}
