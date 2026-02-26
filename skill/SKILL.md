---
name: aitrium-radiotherapy
description: Inspects radiotherapy DICOM studies and computes DVH metrics using rt_inspect, rt_dvh, and rt_dvh_metrics. Use when users ask about RTSTRUCT/RTDOSE discovery, plan QA, dose coverage, D95/D2, DVH curves, or ROI dose statistics.
compatibility: Requires an MCP server named aitrium-radiotherapy exposing tools rt_inspect, rt_dvh, and rt_dvh_metrics. Intended for tool-based agents (Codex/Claude MCP), not shell-only workflows.
---

# RT Analysis Skill

Use this skill with the `aitrium-radiotherapy` MCP server.

## Preconditions

1. Confirm MCP connectivity before calling tools.
2. If unavailable, ask the user to register/restart MCP first.

Host checks:

- Codex: `codex mcp get aitrium-radiotherapy --json`
- Claude Code: `claude mcp get aitrium-radiotherapy`

## Available Tools

- `aitrium-radiotherapy:rt_inspect`
- `aitrium-radiotherapy:rt_dvh`
- `aitrium-radiotherapy:rt_dvh_metrics`

## Invocation Rules

- `rt_inspect`, `rt_dvh`, and `rt_dvh_metrics` are MCP tool names, not shell binaries.
- Do not run shell commands like `which rt_inspect` or `rt_inspect ...`.
- Do not replace tool calls with local fallback scripts unless the user explicitly asks.

Call via MCP (fully qualified tool names):

- `aitrium-radiotherapy:rt_inspect` with `{"path":"<dicom_directory>"}`
- `aitrium-radiotherapy:rt_dvh` with `{"rtstruct_path":"<RS.dcm>","rtdose_path":"<RD.dcm>","structures":["PTV"],"interpolation":false,"z_segments":0,"include_curves":false}`
- `aitrium-radiotherapy:rt_dvh_metrics` with `{"rtstruct_path":"<RS.dcm>","rtdose_path":"<RD.dcm>","structures":["PTV"],"metrics":[{"id":"d95","type":"dose_at_volume","volume_percent":95},{"id":"v60","type":"volume_at_dose","dose_gy":60,"volume_unit":"percent"},{"id":"mean","type":"stat","stat":"mean_gy"}]}`

Do not use MCP CLI commands as a substitute for tool invocation:

- Avoid `which rt_inspect`
- Avoid `codex mcp ...` as a tool-call path
- Call the MCP tools directly through the agent's tool interface

## Workflow

1. Call `aitrium-radiotherapy:rt_inspect` first to discover available studies, structures, plans, and dose grids.
2. Select explicit `rtstruct_path` and `rtdose_path` from inspection output.
3. Prefer `aitrium-radiotherapy:rt_dvh_metrics` for atomic dose-coverage requests to minimize token usage.
4. Use `aitrium-radiotherapy:rt_dvh` with `include_curves=true` only when full DVH curves are explicitly needed.
5. Use DVH stats/metrics for dose coverage checks.

## Interpretation Guidance

- `D95_gy`: dose received by at least 95% of a structure volume.
- `D2_gy`: near-maximum dose metric.
- `volumes_pct`: cumulative percent volume receiving at least each dose bin (returned when `include_curves=true`).
- `homogeneity_index`: `(D2 - D98) / D50`.

## Units

- Dose: Gy
- Volume: cc and %

## Safety

- Treat output as computational support, not a clinical decision on its own.
- Surface parsing/matching warnings to users in final responses.
- If a tool call fails with `Unexpected response type`, treat it as MCP protocol mismatch and ask for server reinstall/restart before retrying.
