---
name: aitrium-radiotherapy
description: Inspects radiotherapy DICOM studies, computes DVH metrics, and runs policy-driven metadata anonymization using rt_inspect, rt_dvh, rt_dvh_metrics, and rt_anonymize_metadata.
compatibility: Requires an MCP server named aitrium-radiotherapy exposing tools rt_inspect, rt_dvh, rt_dvh_metrics, rt_anonymize_metadata, rt_anonymize_template_get, rt_anonymize_template_update, and rt_anonymize_template_reset. Intended for tool-based agents (Codex/Claude MCP), not shell-only workflows.
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
- `aitrium-radiotherapy:rt_anonymize_metadata`
- `aitrium-radiotherapy:rt_anonymize_template_get`
- `aitrium-radiotherapy:rt_anonymize_template_update`
- `aitrium-radiotherapy:rt_anonymize_template_reset`

## Invocation Rules

- `rt_inspect`, `rt_dvh`, `rt_dvh_metrics`, `rt_anonymize_metadata`, and `rt_anonymize_template_*` are MCP tool names, not shell binaries.
- Do not run shell commands like `which rt_inspect` or `rt_inspect ...`.
- Do not replace tool calls with local fallback scripts unless the user explicitly asks.

Call via MCP (fully qualified tool names):

- `aitrium-radiotherapy:rt_inspect` with `{"path":"<dicom_directory>"}`
- `aitrium-radiotherapy:rt_dvh` with `{"rtstruct_path":"<RS.dcm>","rtdose_path":"<RD.dcm>","structures":["PTV"],"interpolation":false,"z_segments":0,"include_curves":false}`
- `aitrium-radiotherapy:rt_dvh_metrics` with `{"rtstruct_path":"<RS.dcm>","rtdose_path":"<RD.dcm>","structures":["PTV"],"metrics":[{"id":"d95","type":"dose_at_volume","volume_percent":95},{"id":"v60","type":"volume_at_dose","dose_gy":60,"volume_unit":"percent"},{"id":"mean","type":"stat","stat":"mean_gy"}]}`
- `aitrium-radiotherapy:rt_anonymize_metadata` with `{"source_path":"<dicom_directory>","output_path":"<anonymized_output_dir>","template":"aitrium_default","dry_run":true}`
- `aitrium-radiotherapy:rt_anonymize_template_get` with `{}`
- `aitrium-radiotherapy:rt_anonymize_template_update` with `{"policy_overrides":{"vr_rules":{"DA":{"action":"replace","replace":{"mode":"date_transform","date_strategy":"fixed_shift_dataset","days_shift":90}}}}}`
- `aitrium-radiotherapy:rt_anonymize_template_reset` with `{}`

Do not use MCP CLI commands as a substitute for tool invocation:

- Avoid `which rt_inspect`
- Avoid `codex mcp ...` as a tool-call path
- Call the MCP tools directly through the agent's tool interface

## Workflow

0. Before running inspection or DVH tools on user-provided dataset paths, ask whether to create and use an anonymized copy first. Recommend anonymize-first by default.
1. Call `aitrium-radiotherapy:rt_inspect` first to discover available studies, structures, plans, and dose grids.
2. Select explicit `rtstruct_path` and `rtdose_path` from inspection output.
3. Prefer `aitrium-radiotherapy:rt_dvh_metrics` for atomic dose-coverage requests to minimize token usage.
4. Use `aitrium-radiotherapy:rt_dvh` with `include_curves=true` only when full DVH curves are explicitly needed.
5. Use DVH stats/metrics for dose coverage checks.
6. Use `aitrium-radiotherapy:rt_anonymize_metadata` for metadata-only de-identification.

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
- Metadata anonymization is header-only; do not claim pixel-data anonymization.
- For user-provided patient datasets, default to anonymize-first workflow (`rt_anonymize_metadata` dry-run, then write to new output path) unless the user explicitly asks to process originals.
- In write mode, anonymized DICOM files are emitted as `MODALITY.SOPInstanceUID.dcm`.
- Sequence containers are recursively processed; if policy sets `replace` on a sequence, runtime applies keep+recurse and emits a warning.
- Surface parsing/matching warnings to users in final responses.
- If a tool call fails with `Unexpected response type`, treat it as MCP protocol mismatch and ask for server reinstall/restart before retrying.
