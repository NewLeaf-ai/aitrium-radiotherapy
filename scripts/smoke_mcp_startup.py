#!/usr/bin/env python3
"""
Smoke test aitrium-radiotherapy MCP startup behavior for packaged release binaries.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
from pathlib import Path


EXPECTED_TOOLS = {"rt_inspect", "rt_dvh", "rt_dvh_metrics"}
EXPECTED_PROTOCOL_VERSION = "2024-11-05"
SELF_SCHEMA_VERSION = "1.0.0"


def run_process(binary: Path, args: list[str], payload: bytes, timeout_s: int = 8) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        [str(binary), *args],
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout_s,
        check=False,
    )


def parse_newline_json(stdout: bytes) -> list[dict]:
    try:
        text = stdout.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise AssertionError(f"stdout is not utf-8: {exc}") from exc

    lines = [line.strip() for line in text.splitlines() if line.strip()]
    if not lines:
        raise AssertionError("no JSON response lines returned")

    parsed: list[dict] = []
    for line in lines:
        try:
            parsed.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise AssertionError(f"invalid JSON response line: {line}") from exc
    return parsed


def parse_framed_json(stdout: bytes) -> dict:
    marker = b"\r\n\r\n"
    idx = stdout.find(marker)
    if idx < 0:
        raise AssertionError("framed response missing CRLF header delimiter")
    header = stdout[:idx].decode("utf-8", errors="replace")
    body = stdout[idx + len(marker) :]

    content_length = None
    for line in header.splitlines():
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        if name.strip().lower() == "content-length":
            try:
                content_length = int(value.strip())
            except ValueError as exc:
                raise AssertionError(f"invalid Content-Length value: {value.strip()}") from exc

    if content_length is None:
        raise AssertionError("framed response missing Content-Length header")
    if len(body) < content_length:
        raise AssertionError(
            f"framed body shorter than Content-Length ({len(body)} < {content_length})"
        )

    try:
        return json.loads(body[:content_length].decode("utf-8"))
    except Exception as exc:
        raise AssertionError("framed payload is not valid JSON") from exc


def assert_initialize_response(response: dict) -> None:
    protocol = (
        response.get("result", {})
        .get("protocolVersion")
    )
    if protocol != EXPECTED_PROTOCOL_VERSION:
        raise AssertionError(
            f"unexpected protocolVersion '{protocol}', expected '{EXPECTED_PROTOCOL_VERSION}'"
        )

    server_name = response.get("result", {}).get("serverInfo", {}).get("name")
    if server_name != "aitrium-radiotherapy-server":
        raise AssertionError(
            f"unexpected serverInfo.name '{server_name}', expected 'aitrium-radiotherapy-server'"
        )


def check_version(binary: Path) -> str:
    result = run_process(binary, ["--version"], b"", timeout_s=4)
    if result.returncode != 0:
        raise AssertionError(
            f"--version exited {result.returncode}; stderr={result.stderr.decode('utf-8', errors='replace')}"
        )
    output = result.stdout.decode("utf-8", errors="replace").strip()
    if not re.match(r"^\d+\.\d+\.\d+([.-][A-Za-z0-9.]+)?$", output):
        raise AssertionError(f"unexpected --version output '{output}'")
    return output


def check_initialize_newline(binary: Path) -> str:
    payload = b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n'
    result = run_process(binary, ["serve-stdio"], payload)
    if result.returncode != 0:
        raise AssertionError(
            f"serve-stdio newline initialize exited {result.returncode}; stderr={result.stderr.decode('utf-8', errors='replace')}"
        )
    responses = parse_newline_json(result.stdout)
    assert_initialize_response(responses[0])
    return "newline initialize ok"


def build_frame(body: bytes, content_length_first: bool) -> bytes:
    if content_length_first:
        header = (
            f"Content-Length: {len(body)}\r\n"
            "Content-Type: application/json\r\n"
            "\r\n"
        )
    else:
        header = (
            "Content-Type: application/json\r\n"
            f"Content-Length: {len(body)}\r\n"
            "\r\n"
        )
    return header.encode("utf-8") + body


def check_initialize_framed(binary: Path, content_length_first: bool) -> str:
    body = b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
    payload = build_frame(body, content_length_first)
    result = run_process(binary, ["serve-stdio"], payload)
    if result.returncode != 0:
        raise AssertionError(
            f"serve-stdio framed initialize exited {result.returncode}; stderr={result.stderr.decode('utf-8', errors='replace')}"
        )
    response = parse_framed_json(result.stdout)
    assert_initialize_response(response)
    return (
        "framed initialize ok (Content-Length first)"
        if content_length_first
        else "framed initialize ok (Content-Type first)"
    )


def check_tools_list(binary: Path) -> str:
    payload = (
        b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n'
        b'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n'
    )
    result = run_process(binary, ["serve-stdio"], payload)
    if result.returncode != 0:
        raise AssertionError(
            f"tools/list check exited {result.returncode}; stderr={result.stderr.decode('utf-8', errors='replace')}"
        )
    responses = parse_newline_json(result.stdout)
    tools_response = next((item for item in responses if item.get("id") == 2), None)
    if tools_response is None:
        raise AssertionError("missing tools/list response id=2")
    tools = tools_response.get("result", {}).get("tools")
    if not isinstance(tools, list):
        raise AssertionError("tools/list result.tools is not an array")
    tool_names = {tool.get("name") for tool in tools if isinstance(tool, dict)}
    missing = sorted(EXPECTED_TOOLS - tool_names)
    if missing:
        raise AssertionError(f"missing required tools: {', '.join(missing)}")
    return f"tools={len(tool_names)}"


def run_checks(binary: Path) -> list[dict]:
    checks: list[tuple[str, callable]] = [
        ("version_flag", lambda: check_version(binary)),
        ("initialize_newline", lambda: check_initialize_newline(binary)),
        (
            "initialize_framed_content_length_first",
            lambda: check_initialize_framed(binary, True),
        ),
        (
            "initialize_framed_content_type_first",
            lambda: check_initialize_framed(binary, False),
        ),
        ("tools_list_expected", lambda: check_tools_list(binary)),
    ]

    results: list[dict] = []
    for check_id, fn in checks:
        try:
            detail = fn()
            results.append({"id": check_id, "passed": True, "detail": detail})
        except subprocess.TimeoutExpired as exc:
            results.append(
                {"id": check_id, "passed": False, "detail": f"timed out after {exc.timeout}s"}
            )
        except Exception as exc:  # noqa: BLE001 - explicit report surface
            results.append({"id": check_id, "passed": False, "detail": str(exc)})
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    binary = Path(args.binary)
    if not binary.exists():
        raise SystemExit(f"Binary not found: {binary}")

    checks = run_checks(binary)
    passed = all(item["passed"] for item in checks)
    passed_count = sum(1 for item in checks if item["passed"])
    failed_count = len(checks) - passed_count

    report = {
        "schema_version": SELF_SCHEMA_VERSION,
        "target": args.target,
        "binary": str(binary),
        "timestamp_utc": dt.datetime.now(tz=dt.timezone.utc).isoformat().replace("+00:00", "Z"),
        "passed": passed,
        "checks": checks,
        "summary": {"passed": passed_count, "failed": failed_count},
    }

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {output}")

    if not passed:
        print("MCP startup smoke checks failed:", file=sys.stderr)
        for item in checks:
            if not item["passed"]:
                print(f"  - {item['id']}: {item['detail']}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
