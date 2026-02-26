use crate::tools::ToolRegistry;
use crate::transport::TransportAdapter;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};

#[derive(Debug, Default)]
pub struct ManualJsonRpcTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TransportMode {
    #[default]
    Unknown,
    NewlineJson,
    Framed,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl TransportAdapter for ManualJsonRpcTransport {
    fn run(&self, registry: &ToolRegistry) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut stdout = io::stdout();
        let mut mode = TransportMode::Unknown;

        loop {
            let request = match read_request(&mut reader, &mut mode) {
                Ok(Some(request)) => request,
                Ok(None) => break,
                Err(message) => {
                    let response = JsonRpcResponse {
                        jsonrpc: "2.0",
                        id: Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {message}"),
                            data: None,
                        }),
                    };
                    write_response(&mut stdout, mode, &response)?;
                    continue;
                }
            };

            if let Some(response) = handle_request(registry, request) {
                write_response(&mut stdout, mode, &response)?;
            }
        }

        Ok(())
    }
}

fn write_response(
    stdout: &mut io::Stdout,
    mode: TransportMode,
    response: &JsonRpcResponse,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(response)?;

    match mode {
        TransportMode::Framed => {
            write!(stdout, "Content-Length: {}\r\n\r\n", payload.len())?;
            stdout.write_all(&payload)?;
            stdout.flush()?;
        }
        TransportMode::Unknown | TransportMode::NewlineJson => {
            stdout.write_all(&payload)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn read_request<R: BufRead>(
    reader: &mut R,
    mode: &mut TransportMode,
) -> Result<Option<JsonRpcRequest>, String> {
    match mode {
        TransportMode::Unknown => {
            let first = match read_nonempty_line(reader)? {
                Some(line) => line,
                None => return Ok(None),
            };

            if looks_like_header_line(&first) {
                *mode = TransportMode::Framed;
                let payload = read_framed_payload(reader, first)?;
                parse_request_bytes(&payload)
            } else {
                *mode = TransportMode::NewlineJson;
                parse_request_line(&first)
            }
        }
        TransportMode::NewlineJson => {
            let line = match read_nonempty_line(reader)? {
                Some(line) => line,
                None => return Ok(None),
            };
            parse_request_line(&line)
        }
        TransportMode::Framed => {
            let first = match read_nonempty_line(reader)? {
                Some(line) => line,
                None => return Ok(None),
            };

            if !looks_like_header_line(&first) {
                return Err("Expected framed header line but received non-header input".to_string());
            }

            let payload = read_framed_payload(reader, first)?;
            parse_request_bytes(&payload)
        }
    }
}

fn read_nonempty_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, String> {
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("Failed to read line: {error}"))?;

        if bytes == 0 {
            return Ok(None);
        }

        if line.trim().is_empty() {
            continue;
        }

        return Ok(Some(line));
    }
}

fn parse_request_line(line: &str) -> Result<Option<JsonRpcRequest>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let request =
        serde_json::from_str::<JsonRpcRequest>(trimmed).map_err(|error| format!("{error}"))?;
    Ok(Some(request))
}

fn parse_request_bytes(bytes: &[u8]) -> Result<Option<JsonRpcRequest>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }

    let request =
        serde_json::from_slice::<JsonRpcRequest>(bytes).map_err(|error| format!("{error}"))?;
    Ok(Some(request))
}

fn read_framed_payload<R: BufRead>(
    reader: &mut R,
    first_header: String,
) -> Result<Vec<u8>, String> {
    let headers = read_header_block(reader, first_header)?;
    let content_length = parse_content_length(&headers)
        .ok_or_else(|| "Missing or invalid Content-Length header".to_string())?;

    let mut payload = vec![0_u8; content_length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("Failed to read framed payload: {error}"))?;

    Ok(payload)
}

fn read_header_block<R: BufRead>(
    reader: &mut R,
    first_header: String,
) -> Result<Vec<String>, String> {
    let mut headers = Vec::new();
    headers.push(first_header.trim().to_string());

    loop {
        let mut header_line = String::new();
        let bytes = reader
            .read_line(&mut header_line)
            .map_err(|error| format!("Failed to read header line: {error}"))?;

        if bytes == 0 {
            return Err("Unexpected EOF while reading frame headers".to_string());
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }

        headers.push(trimmed.to_string());
    }

    Ok(headers)
}

fn looks_like_header_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return false;
    }

    trimmed.contains(':')
}

fn parse_content_length(headers: &[String]) -> Option<usize> {
    for header in headers {
        let mut parts = header.splitn(2, ':');
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            return value.parse::<usize>().ok();
        }
    }
    None
}

fn handle_request(registry: &ToolRegistry, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let Some(id) = request.id else {
        if request.method == "notifications/initialized" {
            return None;
        }

        return None;
    };

    match request.method.as_str() {
        "initialize" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "aitrium-radiotherapy-server",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                }
            })),
            error: None,
        }),
        "tools/list" => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "tools": registry
                    .list()
                    .into_iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            // MCP spec field names:
                            "inputSchema": tool.input_schema,
                            "outputSchema": tool.output_schema,
                            // Backward-compatible aliases for existing SDK consumers:
                            "input_schema": tool.input_schema,
                            "output_schema": tool.output_schema
                        })
                    })
                    .collect::<Vec<_>>()
            })),
            error: None,
        }),
        "tools/call" => {
            let params = request.params.unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .or_else(|| params.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            let arguments = params
                .get("arguments")
                .cloned()
                .or_else(|| params.get("input").cloned())
                .unwrap_or_else(|| json!({}));

            match registry.call(&name, arguments) {
                Ok(payload) => Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
                        }],
                        "structuredContent": payload,
                        "isError": false
                    })),
                    error: None,
                }),
                Err(tool_error) => Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string(&tool_error).unwrap_or_else(|_| "{}".to_string())
                        }],
                        "structuredContent": tool_error,
                        "isError": true
                    })),
                    error: None,
                }),
            }
        }
        other => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {other}"),
                data: None,
            }),
        }),
    }
}
