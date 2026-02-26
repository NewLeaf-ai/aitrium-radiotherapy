# aitrium-radiotherapy

`aitrium-radiotherapy` is a standalone radiotherapy analysis server with M1 tools:

- `rt_inspect`: scan and inspect DICOM RT study metadata.
- `rt_dvh`: compute DVHs from explicit RTSTRUCT + RTDOSE paths (compact by default).
- `rt_dvh_metrics`: compute atomic DVH metrics (D@V, V@D, stat fields) for token-efficient agent use.

It vendors the Rust DVH engine from `newleaf-native/crates/aitrium_dvh` as the computation source of truth and exposes language-neutral JSON contracts for Python and TypeScript SDKs.

## M1 Scope

- Included: `rt_inspect`, `rt_dvh`, `rt_dvh_metrics`, schema contracts, Python SDK, TypeScript SDK.
- Deferred to M2: `rt_overlap`, `rt_margin`.

## Transport Adapter Spike Outcome

The codebase includes a transport seam (`TransportAdapter`) and currently runs a manual stdio JSON-RPC adapter (`manual_jsonrpc`) by default.

- `AITRIUM_RADIOTHERAPY_TRANSPORT=manual_jsonrpc` (default): active adapter.
- `AITRIUM_RADIOTHERAPY_TRANSPORT=mcp_crate`: currently logs warning and falls back to manual adapter.

This keeps tool business logic transport-agnostic while MCP crate integration is finalized.

## Repository Layout

```
aitrium-radiotherapy/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── types.rs
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── inspect.rs
│   │   └── dvh.rs
│   ├── inspect/
│   │   ├── mod.rs
│   │   ├── scanner.rs
│   │   ├── structure_reader.rs
│   │   ├── plan_reader.rs
│   │   └── dose_reader.rs
│   └── transport/
│       ├── mod.rs
│       └── manual_jsonrpc.rs
├── crates/
│   └── aitrium_dvh/
├── schemas/
├── sdk/
│   ├── python/
│   └── typescript/
├── skill/
│   └── SKILL.md
├── tests/
│   └── fixtures/
└── install.sh
```

## Build

```bash
cargo build
```

## Install (prebuilt, no toolchain)

macOS/Linux:

```bash
curl -fsSL https://github.com/NewLeaf-ai/agentic-dicom-suite/releases/latest/download/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://github.com/NewLeaf-ai/agentic-dicom-suite/releases/latest/download/install.ps1 | iex
```

Known-bad release:

- `aitrium-radiotherapy-v0.1.0-beta.1` is deprecated due MCP startup reliability defects. Use `beta.2+` or stable tags.

Installer flags:

- `--version <semver|latest>`
- `--channel stable|beta`
- `--agent codex|claude|both|none`
- `--no-skill`
- `--no-mcp`
- `--bin-dir <path>`
- `--repo <owner/repo>`
- `--skip-self-test` (dev-only)
- `--self-test-only`
- `--verify-mcp-only`

Release assets include per-target archives, checksums, installers, skill package, and `manifest.json`.

## Fast Dev Refresh

Use this during development to quickly rebuild and refresh skills + MCP config for both Codex and Claude:

```bash
cd /Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy
./scripts/dev-refresh.sh
```

Notes:

- Default uses `target/debug/aitrium-radiotherapy-server` for faster iteration.
- Use `./scripts/dev-refresh.sh --release` to point MCP at release builds.
- Use `./scripts/dev-refresh.sh --copy-local` if you also want `~/.local/bin/aitrium-radiotherapy-server` updated.

## Run

```bash
cargo run
```

The server supports stdio JSON-RPC in both:

- newline-delimited JSON mode
- framed mode (`Content-Length` headers; header order agnostic)

## Runtime diagnostics

- `aitrium-radiotherapy-server --build-info --json`
- `aitrium-radiotherapy-server self-test --json`

`self-test` validates version, newline initialize, framed initialize (header order variants), and `tools/list`.

## Agent Integration Note

`rt_inspect`, `rt_dvh`, and `rt_dvh_metrics` are MCP tool names, not shell commands.

- Correct: call tool `aitrium-radiotherapy/rt_inspect`, `aitrium-radiotherapy/rt_dvh`, or `aitrium-radiotherapy/rt_dvh_metrics` through an MCP-capable client.
- Incorrect: running `which rt_inspect` or executing `rt_inspect` in shell.

### Minimal Protocol Examples

Initialize:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
```

List tools:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

Call `rt_inspect`:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rt_inspect","arguments":{"path":"/path/to/dicom"}}}
```

Call `rt_dvh`:

```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rt_dvh","arguments":{"rtstruct_path":"/path/RS.dcm","rtdose_path":"/path/RD.dcm","structures":["PTV_60","Heart"],"interpolation":true,"z_segments":1,"include_curves":false}}}
```

Call `rt_dvh_metrics`:

```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rt_dvh_metrics","arguments":{"rtstruct_path":"/path/RS.dcm","rtdose_path":"/path/RD.dcm","structures":["PTV_60"],"metrics":[{"id":"d95","type":"dose_at_volume","volume_percent":95},{"id":"v60","type":"volume_at_dose","dose_gy":60,"volume_unit":"percent"},{"id":"mean","type":"stat","stat":"mean_gy"}]}}}
```

## Contract Schemas

Canonical schemas live in `/Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy/schemas`:

- `rt_inspect.input.schema.json`
- `rt_inspect.output.schema.json`
- `rt_dvh.input.schema.json`
- `rt_dvh.output.schema.json`
- `rt_dvh_metrics.input.schema.json`
- `rt_dvh_metrics.output.schema.json`
- `error.schema.json`

All success responses include `schema_version` (currently `1.0.0`).

## Python SDK

Path: `/Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy/sdk/python`

```bash
cd /Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy/sdk/python
pip install -e .
```

Example:

```python
from aitrium_radiotherapy_client import AitriumRadiotherapyClient

with AitriumRadiotherapyClient() as client:
    tools = client.list_tools()
    inspect = client.inspect("/path/to/dicom")
```

## TypeScript SDK

Path: `/Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy/sdk/typescript`

```bash
cd /Users/spencerjohnson/projects/aitrium/aitrium-radiotherapy/sdk/typescript
npm install
npm run build
```

Example:

```ts
import { AitriumRadiotherapyClient } from "@aitrium-radiotherapy/client";

const client = new AitriumRadiotherapyClient(["aitrium-radiotherapy-server"]);
const tools = await client.listTools();
```

## Quality Gates

- Rust: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`
- Schemas: `python3 scripts/check_schemas.py`
- Python SDK tests: `cd sdk/python && pytest`
- TypeScript SDK tests: `cd sdk/typescript && npm test`

## Notes

- M1 requires explicit `rtstruct_path` + `rtdose_path` for DVH tools to avoid heuristic mismatches.
- `rt_dvh` defaults to compact output (`include_curves=false`) and only returns full arrays when explicitly requested.
