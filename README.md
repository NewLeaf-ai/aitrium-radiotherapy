# Aitrium Radiotherapy (Proof of Concept)

`aitrium-radiotherapy` is a local MCP server for radiotherapy DICOM analysis.

It exposes three tools:
- `rt_inspect`
- `rt_dvh`
- `rt_dvh_metrics`

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

## Practical Use Cases

- Local DICOM triage: inspect structures, plans, and dose objects in a dataset.
- Agent-assisted RTQA prototyping: compute DVH metrics for rule evaluation workflows.
- Batch research analysis: run repeatable metric extraction across anonymized RT datasets.

## Tool Summary

- `rt_inspect`: scan DICOM RT study metadata from a folder path.
- `rt_dvh`: compute DVH outputs from explicit `RTSTRUCT` + `RTDOSE` paths.
- `rt_dvh_metrics`: compute compact metrics (for example `D@V`, `V@D`, selected stats).

Canonical schemas are in `schemas/`.

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
