# Aitrium Radiotherapy (Proof of Concept)

`aitrium-radiotherapy` is a local MCP server for radiotherapy DICOM analysis.

It exposes eight tools:
- `rt_inspect`
- `rt_dvh`
- `rt_dvh_metrics`
- `rt_margin`
- `rt_anonymize_metadata`
- `rt_anonymize_template_get`
- `rt_anonymize_template_update`
- `rt_anonymize_template_reset`

## Important Safety Notice

- This software is a proof of concept.
- This software is not a medical device.
- This software is not for clinical diagnosis, treatment planning, or patient-care decisions.
- Use anonymized/de-identified datasets only.
- Any outputs must be independently validated by qualified clinical professionals.

## Purpose

The goal is to provide deterministic local analysis that can be used by:
- AI agent workflows (MCP clients such as Codex or Claude)
- direct local CLI/script workflows (no Codex/Claude required)

Core DVH computation is powered by the standalone [`aitrium-dvh`](https://github.com/NewLeaf-ai/aitrium-dvh) crate.

## PII and Data Handling

What this project does to reduce PII risk:
- Analysis runs locally in the `aitrium-radiotherapy-server` process.
- Tool execution does not require cloud APIs for DVH/inspection computation.
- Recommended workflow is de-identified DICOM only.

Important warning for cloud-hosted agent platforms (for example, Codex/Claude cloud sessions):
- MCP tool arguments and tool results may be processed by the model provider platform.
- If paths, metadata, or outputs contain identifiers, those identifiers may leave your local machine through the agent workflow.

Recommended controls:
- Use anonymized/de-identified datasets only.
- Avoid patient identifiers in file/directory names.
- Review provider retention/logging settings before use.
- For sensitive workflows, use local/offline agent deployments.

## Installation

### macOS/Linux (GitHub Release)

```bash
curl -fsSL https://github.com/NewLeaf-ai/aitrium-radiotherapy/releases/latest/download/install.sh | bash
```

### macOS/Linux (Public GCS Distribution)

```bash
curl -fsSL "https://storage.googleapis.com/<bucket>/aitrium-radiotherapy-vX.Y.Z/install.sh" | \
  bash -s -- --release-base-url "https://storage.googleapis.com/<bucket>/aitrium-radiotherapy-vX.Y.Z"
```

### Windows PowerShell

```powershell
irm https://github.com/NewLeaf-ai/aitrium-radiotherapy/releases/latest/download/install.ps1 | iex
```

Common installer flags:
- `--version <semver|latest>`
- `--channel stable|beta`
- `--agent codex|claude|both|none`
- `--no-skill`
- `--no-mcp`
- `--bin-dir <path>`
- `--release-base-url <url>`
- `--manifest-url <url>`

## Verify Installation

```bash
aitrium-radiotherapy-server --version
aitrium-radiotherapy-server self-test --json
```

## Updating Existing Installations

Re-running the installer is the supported update path. It is idempotent and replaces the installed runtime binary.

### Update to latest stable

```bash
curl -fsSL https://github.com/NewLeaf-ai/aitrium-radiotherapy/releases/latest/download/install.sh | bash
```

### Update from public GCS release

```bash
BASE="https://storage.googleapis.com/<bucket>/aitrium-radiotherapy-vX.Y.Z"
curl -fsSL "$BASE/install.sh" | bash -s -- --release-base-url "$BASE"
```

### Pin a specific version (recommended for reproducibility)

```bash
curl -fsSL https://github.com/NewLeaf-ai/aitrium-radiotherapy/releases/latest/download/install.sh | \
  bash -s -- --version 0.1.2
```

## Replace a Prior Test Installation

If you previously registered a test server name (for example `aitrium-radiotherapy-test`):

1. Remove old test MCP registrations:
```bash
codex mcp remove aitrium-radiotherapy-test || true
claude mcp remove aitrium-radiotherapy-test -s user || true
```

2. Reinstall current release to your intended bin path:
```bash
curl -fsSL https://github.com/NewLeaf-ai/aitrium-radiotherapy/releases/latest/download/install.sh | \
  bash -s -- --agent both --bin-dir "$HOME/.local/bin"
```

3. Verify active MCP registrations:
```bash
codex mcp get aitrium-radiotherapy --json
claude mcp get aitrium-radiotherapy -s user
```

## Use Case A: Agent Workflow (MCP)

Use this path when you want Codex/Claude to call tools automatically.

Manual registration (if you skipped installer auto-registration):

```bash
codex mcp add aitrium-radiotherapy -- "$HOME/.local/bin/aitrium-radiotherapy-server"
claude mcp add --scope user aitrium-radiotherapy "$HOME/.local/bin/aitrium-radiotherapy-server"
```

Verify registration:

```bash
codex mcp get aitrium-radiotherapy --json
claude mcp get aitrium-radiotherapy -s user
```

Then ask your agent to call:
- `rt_inspect` for dataset discovery
- `rt_dvh_metrics` for compact rule-oriented metrics
- `rt_dvh` when full DVH curves are needed
- `rt_margin` for directional margin analysis between two structures
- `rt_anonymize_metadata` for policy-driven metadata de-identification
- `rt_anonymize_template_get` to inspect effective runtime template policy
- `rt_anonymize_template_update` to persist template changes to a single editable copy
- `rt_anonymize_template_reset` to remove custom copy and return to built-in fallback

## Use Case B: Direct Local CLI Workflow (No Codex/Claude)

Use this path when you cannot or do not want to use cloud agents.

### 1) Inspect a DICOM directory

```bash
aitrium-radiotherapy-server inspect --path /path/to/dicom_dir
```

### 2) Compute DVH output

```bash
aitrium-radiotherapy-server dvh \
  --rtstruct /path/to/RTSTRUCT.dcm \
  --rtdose /path/to/RTDOSE.dcm \
  --structure PTV_60 \
  --structure Heart
```

### 3) Compute targeted metrics

```bash
aitrium-radiotherapy-server dvh-metrics \
  --rtstruct /path/to/RTSTRUCT.dcm \
  --rtdose /path/to/RTDOSE.dcm \
  --metric 'd95=dav:95' \
  --metric 'v20=vad:20:percent' \
  --metric 'mean=stat:mean_gy'
```

Alternative metrics input forms:
- `--metrics-json '<json-array>'`
- `--metrics-file /path/to/metrics.json`

All command outputs are JSON.

### 4) Compute directional margin between structures

```bash
aitrium-radiotherapy-server margin \
  --rtstruct /path/to/RTSTRUCT.dcm \
  --from CTV_70 \
  --to PTV_70 \
  --direction posterior \
  --coverage-thresholds 3,5,7
```

Margin parameter semantics:
- `--direction`: anatomical direction filter applied to source (`--from`) boundary samples before clearance extraction. Valid values:
  - `uniform`: no directional filtering.
  - `lateral`: conservative lateral clearance, computed as `min(left, right)`.
  - `posterior`: toward patient back.
  - `anterior`: toward patient front.
  - `left`: toward patient left side.
  - `right`: toward patient right side.
  - `superior`: toward patient head.
  - `inferior`: toward patient feet.
- `--coverage-thresholds 3,5,7`: evaluates coverage at 3 mm, 5 mm, and 7 mm. For a threshold `t`, coverage is `% of source boundary samples with clearance >= t`.
- Clearance sign convention: positive means the source is inside the target with margin; negative means the source protrudes outside the target.
- `--summary-percentile 5`: makes `summary_mm` the 5th percentile clearance, which is the default policy-facing metric.
- `--direction-cone`: cone half-angle in degrees for directional filtering (`45` means a 45-degree cone around the selected direction).
- `--xy-resolution`: synthetic in-plane voxel size in mm for the RTSTRUCT-only clearance engine.
- `--z-resolution`: optional synthetic slice spacing in mm. Omit it to let the engine choose automatically.
- `--max-voxels`: cap for the synthetic grid size; the engine auto-coarsens if it would exceed this limit.

### 5) Metadata anonymization (dry-run or write)

```bash
aitrium-radiotherapy-server anonymize-metadata \
  --source /path/to/source \
  --template aitrium_default
```

```bash
aitrium-radiotherapy-server anonymize-metadata \
  --source /path/to/source \
  --output /path/to/anonymized-copy \
  --template strict_phi_safe \
  --write
```

Write mode emits DICOM files as `MODALITY.SOPInstanceUID.dcm` using anonymized SOP Instance UIDs.

### Parameter Semantics Quick Reference

- `dvh --interpolation --z-segments N`: enables additional contour-plane sampling between original z planes (`N=0` means none).
- `dvh --max-points N` and `--precision N`: output-size controls only (they downsample/round returned DVH arrays; they do not change core DVH stats).
- `dvh-metrics --metric 'd95=dav:95'`: `dav` means dose-at-volume-percent.
- `dvh-metrics --metric 'v20=vad:20:percent'`: `vad` means volume-at-dose; final token is output unit (`percent` or `cc`).
- `margin --direction <value>`: valid values are `uniform`, `lateral`, `posterior`, `anterior`, `left`, `right`, `superior`, `inferior`.
- `margin --coverage-thresholds 3,5,7`: computes coverage at 3 mm, 5 mm, and 7 mm thresholds using `% boundary samples with clearance >= threshold`.
- `anonymize-metadata --write`: enables copy-on-write output mode; without `--write`, execution is dry-run only.

## Practical Use Cases

- Local DICOM triage: inspect structures, plans, and dose objects in a dataset.
- Agent-assisted RTQA prototyping: compute DVH metrics for rule evaluation workflows.
- Batch research analysis: run repeatable metric extraction across anonymized RT datasets.
- Policy-driven metadata sanitization: create copy-on-write anonymized datasets with audit-style reports.

## Tool Summary

- `rt_inspect`: scan DICOM RT study metadata from a folder path.
- `rt_dvh`: compute DVH outputs from explicit `RTSTRUCT` + `RTDOSE` paths.
- `rt_dvh_metrics`: compute compact metrics (for example `D@V`, `V@D`, selected stats).
- `rt_margin`: compute directional A -> B margin statistics with coverage thresholds.
- `rt_anonymize_metadata`: policy-based DICOM metadata anonymization (no pixel transformations).
- `rt_anonymize_template_get`: read effective runtime template alias (`aitrium_template`).
- `rt_anonymize_template_update`: persist runtime template edits (single custom copy).
- `rt_anonymize_template_reset`: delete runtime custom copy and use built-in fallback.

Canonical schemas are in `schemas/`.

See [`docs/ANONYMIZATION.md`](docs/ANONYMIZATION.md) for policy details and templates.
Template options include `strict_phi_safe`, `research_balanced`, `minimal_explicit`, `aitrium_default`, and runtime alias `aitrium_template`.

## Coming Soon

- Local-agent setup guides (fully local MCP + local model workflows).
- Local privacy hardening checklist for regulated environments.
- Reference integrations for local-first agent runtimes.

## Development

```bash
cargo check
cargo test
```

Local refresh helper:

```bash
./scripts/dev-refresh.sh
```

## License

MIT
