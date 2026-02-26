from __future__ import annotations

import json
import subprocess
import threading
from typing import Any, Iterable

from aitrium_radiotherapy_client.exceptions import AitriumRadiotherapyError, TransportError, raise_for_error
from aitrium_radiotherapy_client.models import (
    ApiErrorModel,
    RtDvhMetricsResponse,
    RtDvhResponse,
    RtInspectResponse,
    ToolSpec,
)


class AitriumRadiotherapyClient:
    def __init__(
        self,
        command: Iterable[str] | None = None,
        auto_initialize: bool = True,
    ) -> None:
        cmd = list(command) if command is not None else ["aitrium-radiotherapy-server"]
        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self._lock = threading.Lock()
        self._next_id = 1

        if auto_initialize:
            self._rpc("initialize", {})

    def close(self) -> None:
        if self._proc.poll() is None:
            self._proc.terminate()
            self._proc.wait(timeout=5)

    def __enter__(self) -> "AitriumRadiotherapyClient":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:  # type: ignore[override]
        self.close()

    def list_tools(self) -> list[ToolSpec]:
        result = self._rpc("tools/list", {})
        tools = result.get("tools", [])
        normalized = []
        for tool in tools:
            if not isinstance(tool, dict):
                continue

            normalized_tool = dict(tool)
            if "input_schema" not in normalized_tool and "inputSchema" in normalized_tool:
                normalized_tool["input_schema"] = normalized_tool["inputSchema"]
            if "output_schema" not in normalized_tool and "outputSchema" in normalized_tool:
                normalized_tool["output_schema"] = normalized_tool["outputSchema"]
            normalized_tool.pop("inputSchema", None)
            normalized_tool.pop("outputSchema", None)
            normalized.append(normalized_tool)

        return [ToolSpec.model_validate(tool) for tool in normalized]

    def inspect(self, path: str) -> RtInspectResponse:
        payload = self._call_tool("rt_inspect", {"path": path})
        return RtInspectResponse.model_validate(payload)

    def dvh(
        self,
        rtstruct_path: str,
        rtdose_path: str,
        structures: list[str] | None = None,
        interpolation: bool = False,
        z_segments: int = 0,
        include_curves: bool = False,
    ) -> RtDvhResponse:
        args: dict[str, Any] = {
            "rtstruct_path": rtstruct_path,
            "rtdose_path": rtdose_path,
            "interpolation": interpolation,
            "z_segments": z_segments,
            "include_curves": include_curves,
        }
        if structures is not None:
            args["structures"] = structures

        payload = self._call_tool("rt_dvh", args)
        return RtDvhResponse.model_validate(payload)

    def dvh_metrics(
        self,
        rtstruct_path: str,
        rtdose_path: str,
        metrics: list[dict[str, Any]],
        structures: list[str] | None = None,
        interpolation: bool = False,
        z_segments: int = 0,
    ) -> RtDvhMetricsResponse:
        args: dict[str, Any] = {
            "rtstruct_path": rtstruct_path,
            "rtdose_path": rtdose_path,
            "metrics": metrics,
            "interpolation": interpolation,
            "z_segments": z_segments,
        }
        if structures is not None:
            args["structures"] = structures

        payload = self._call_tool("rt_dvh_metrics", args)
        return RtDvhMetricsResponse.model_validate(payload)

    def _call_tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self._rpc("tools/call", {"name": name, "arguments": arguments})
        is_error = bool(result.get("isError"))
        payload: Any = result.get("structuredContent")

        if payload is None:
            content = result.get("content", [])
            first = content[0] if content else {}
            if isinstance(first, dict) and "json" in first:
                payload = first.get("json")
            elif isinstance(first, dict) and first.get("type") == "text":
                text = first.get("text")
                if isinstance(text, str):
                    try:
                        payload = json.loads(text)
                    except json.JSONDecodeError as error:
                        raise TransportError(f"Invalid JSON in text tool payload: {error}") from error

        if payload is None:
            payload = {}

        if is_error:
            error_model = ApiErrorModel.model_validate(payload)
            raise_for_error(error_model)

        if not isinstance(payload, dict):
            raise TransportError("Expected JSON object tool payload")

        return payload

    def _rpc(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            request_id = self._next_id
            self._next_id += 1

            request = {
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params,
            }

            if self._proc.stdin is None or self._proc.stdout is None:
                raise TransportError("Client process pipes are unavailable")

            self._proc.stdin.write(json.dumps(request) + "\n")
            self._proc.stdin.flush()

            while True:
                line = self._proc.stdout.readline()
                if line == "":
                    stderr_tail = ""
                    if self._proc.stderr is not None:
                        stderr_tail = self._proc.stderr.read()[-2000:]
                    raise TransportError(
                        f"Server closed connection while waiting for response to method '{method}'. stderr={stderr_tail!r}"
                    )

                response = json.loads(line)
                if response.get("id") != request_id:
                    continue

                if response.get("error"):
                    raise TransportError(f"JSON-RPC error: {response['error']}")

                result = response.get("result")
                if not isinstance(result, dict):
                    raise TransportError("Expected JSON object result")
                return result
