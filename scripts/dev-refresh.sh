#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/dev-refresh.sh [options]

Fast developer refresh for aitrium-radiotherapy:
  1) Build server binary (debug by default)
  2) Update skill files for Claude + Codex
  3) Re-register MCP server for Claude + Codex

Options:
  --debug         Build debug binary (default)
  --release       Build release binary
  --no-build      Skip build step
  --no-skills     Skip skill file updates
  --no-mcp        Skip MCP registration
  --copy-local    Also copy binary to ~/.local/bin
  -h, --help      Show this help
EOF
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_NAME="aitrium-radiotherapy-server"
MCP_SERVER_NAME="aitrium-radiotherapy"
PROFILE="debug"
DO_BUILD=1
DO_SKILLS=1
DO_MCP=1
COPY_LOCAL=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)
      PROFILE="debug"
      ;;
    --release)
      PROFILE="release"
      ;;
    --no-build)
      DO_BUILD=0
      ;;
    --no-skills)
      DO_SKILLS=0
      ;;
    --no-mcp)
      DO_MCP=0
      ;;
    --copy-local)
      COPY_LOCAL=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
  shift
done

run_with_timeout() {
  local timeout_seconds="$1"
  shift

  "$@" &
  local cmd_pid=$!

  (
    sleep "${timeout_seconds}"
    if kill -0 "${cmd_pid}" >/dev/null 2>&1; then
      kill -TERM "${cmd_pid}" >/dev/null 2>&1 || true
    fi
  ) &
  local watchdog_pid=$!

  wait "${cmd_pid}"
  local cmd_status=$?

  kill "${watchdog_pid}" >/dev/null 2>&1 || true
  wait "${watchdog_pid}" >/dev/null 2>&1 || true

  return "${cmd_status}"
}

if [[ "${DO_BUILD}" -eq 1 ]]; then
  echo "Building ${SERVER_NAME} (${PROFILE})..."
  if [[ "${PROFILE}" == "release" ]]; then
    cargo build --release --manifest-path "${ROOT_DIR}/Cargo.toml"
  else
    cargo build --manifest-path "${ROOT_DIR}/Cargo.toml"
  fi
fi

BIN_PATH="${ROOT_DIR}/target/${PROFILE}/${SERVER_NAME}"
if [[ -f "${BIN_PATH}.exe" ]]; then
  BIN_PATH="${BIN_PATH}.exe"
fi

if [[ ! -f "${BIN_PATH}" ]]; then
  echo "Binary not found: ${BIN_PATH}"
  exit 1
fi

if [[ "${COPY_LOCAL}" -eq 1 ]]; then
  LOCAL_BIN_DIR="${HOME}/.local/bin"
  mkdir -p "${LOCAL_BIN_DIR}"
  LOCAL_BIN_PATH="${LOCAL_BIN_DIR}/${SERVER_NAME}"
  if [[ "${BIN_PATH}" == *.exe ]]; then
    LOCAL_BIN_PATH="${LOCAL_BIN_PATH}.exe"
  fi

  cp "${BIN_PATH}" "${LOCAL_BIN_PATH}"
  if [[ "${LOCAL_BIN_PATH}" != *.exe ]]; then
    chmod +x "${LOCAL_BIN_PATH}"
  fi
  echo "Copied binary: ${LOCAL_BIN_PATH}"
fi

if [[ "${DO_SKILLS}" -eq 1 ]]; then
  echo "Updating skill files..."
  SKILL_SOURCE="${ROOT_DIR}/skill/SKILL.md"
  CLAUDE_SKILL_DIR="${HOME}/.claude/skills/${MCP_SERVER_NAME}"
  CODEX_HOME_DIR="${CODEX_HOME:-${HOME}/.codex}"
  CODEX_SKILL_DIR="${CODEX_HOME_DIR}/skills/${MCP_SERVER_NAME}"

  mkdir -p "${CLAUDE_SKILL_DIR}"
  cp "${SKILL_SOURCE}" "${CLAUDE_SKILL_DIR}/SKILL.md"

  mkdir -p "${CODEX_SKILL_DIR}"
  cp "${SKILL_SOURCE}" "${CODEX_SKILL_DIR}/SKILL.md"
fi

CLAUDE_STATUS="skipped"
CODEX_STATUS="skipped"

if [[ "${DO_MCP}" -eq 1 ]]; then
  if command -v claude >/dev/null 2>&1; then
    echo "Refreshing Claude MCP server..."
    run_with_timeout 10 claude mcp remove "${MCP_SERVER_NAME}" -s user >/dev/null 2>&1 || true
    if run_with_timeout 15 claude mcp add --scope user "${MCP_SERVER_NAME}" "${BIN_PATH}" >/dev/null 2>&1; then
      CLAUDE_STATUS="configured"
    else
      CLAUDE_STATUS="failed or timed out"
    fi
  else
    CLAUDE_STATUS="claude CLI not found"
  fi

  if command -v codex >/dev/null 2>&1; then
    echo "Refreshing Codex MCP server..."
    run_with_timeout 10 codex mcp remove "${MCP_SERVER_NAME}" >/dev/null 2>&1 || true
    if run_with_timeout 15 codex mcp add "${MCP_SERVER_NAME}" -- "${BIN_PATH}" >/dev/null 2>&1; then
      CODEX_STATUS="configured"
    else
      CODEX_STATUS="failed or timed out"
    fi
  else
    CODEX_STATUS="codex CLI not found"
  fi
fi

echo "Dev refresh summary:"
echo "  binary: ${BIN_PATH}"
echo "  claude mcp: ${CLAUDE_STATUS}"
echo "  codex mcp: ${CODEX_STATUS}"
echo "Recommended: restart active Claude/Codex sessions before retesting tools."
