#!/usr/bin/env bash
set -euo pipefail

SERVER_NAME="aitrium-radiotherapy-server"
DEFAULT_REPO="${AITRIUM_RADIOTHERAPY_GITHUB_REPO:-NewLeaf-ai/agentic-dicom-suite}"
CHANNEL="stable"
VERSION="latest"
AGENT="both"
NO_SKILL=0
NO_MCP=0
SKIP_SELF_TEST=0
SELF_TEST_ONLY=0
VERIFY_MCP_ONLY=0
BIN_DIR="${HOME}/.local/bin"
INSTALL_REPO="${DEFAULT_REPO}"
RELEASE_BASE_URL="${AITRIUM_RADIOTHERAPY_RELEASE_BASE_URL:-}"
MANIFEST_URL="${AITRIUM_RADIOTHERAPY_MANIFEST_URL:-}"
GITHUB_AUTH_TOKEN="${AITRIUM_GITHUB_TOKEN:-${GITHUB_TOKEN:-${GH_TOKEN:-}}}"

CLAUDE_SKILL_STATUS="skipped"
CODEX_SKILL_STATUS="skipped"
CLAUDE_MCP_STATUS="skipped"
CODEX_MCP_STATUS="skipped"
CLAUDE_VERIFY_STATUS="skipped"
CODEX_VERIFY_STATUS="skipped"
RUNTIME_SELF_TEST_STATUS="skipped"

usage() {
  cat <<'EOF'
Install aitrium-radiotherapy from prebuilt release assets.

Usage:
  install.sh [options]

Options:
  --version <semver|latest>      Version to install (default: latest)
  --channel <stable|beta>        Release channel (default: stable)
  --agent <codex|claude|both|none>
                                 Agent integration target (default: both)
  --no-skill                     Skip skill installation
  --no-mcp                       Skip MCP auto-registration
  --bin-dir <path>               Install binary to this directory (default: ~/.local/bin)
  --repo <owner/repo>            GitHub repository source
  --release-base-url <url>       Base URL containing manifest/assets (e.g. public GCS path)
  --manifest-url <url>           Full URL to manifest.json (overrides tag/base-url resolution)
  --skip-self-test               Skip runtime self-test (dev-only)
  --self-test-only               Run self-test on installed binary and exit
  --verify-mcp-only              Verify configured MCP integrations and exit
  -h, --help                     Show this help

Environment:
  AITRIUM_RADIOTHERAPY_GITHUB_REPO        GitHub repo in owner/repo form
                                 (default: NewLeaf-ai/agentic-dicom-suite)
  AITRIUM_RADIOTHERAPY_RELEASE_BASE_URL   Override release base URL
  AITRIUM_RADIOTHERAPY_MANIFEST_URL       Override manifest URL
  AITRIUM_GITHUB_TOKEN / GITHUB_TOKEN / GH_TOKEN
                                 Optional token for private GitHub release assets
EOF
}

log() {
  printf '[aitrium-radiotherapy] %s\n' "$*"
}

warn() {
  printf '[aitrium-radiotherapy] warning: %s\n' "$*" >&2
}

die() {
  printf '[aitrium-radiotherapy] error: %s\n' "$*" >&2
  exit 1
}

to_lower() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

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

  set +e
  wait "${cmd_pid}"
  local cmd_status=$?
  set -e

  kill "${watchdog_pid}" >/dev/null 2>&1 || true
  wait "${watchdog_pid}" >/dev/null 2>&1 || true
  return "${cmd_status}"
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
    return
  fi
  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $NF}'
    return
  fi
  die "No SHA256 tool found (sha256sum, shasum, or openssl required)."
}

curl_with_auth() {
  if [[ -n "$GITHUB_AUTH_TOKEN" ]]; then
    curl -H "Authorization: Bearer ${GITHUB_AUTH_TOKEN}" "$@"
    return
  fi
  curl "$@"
}

download_to() {
  local url="$1"
  local out="$2"
  curl_with_auth -fL --retry 3 --retry-delay 1 "$url" -o "$out"
}

detect_target() {
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="darwin" ;;
    Linux) os="linux" ;;
    MINGW*|MSYS*|CYGWIN*) os="windows" ;;
    *) die "Unsupported OS: $os" ;;
  esac

  case "$arch" in
    arm64|aarch64) arch="aarch64" ;;
    x86_64|amd64) arch="x86_64" ;;
    *) die "Unsupported architecture: $arch" ;;
  esac

  if [[ "$os" == "linux" && "$arch" != "x86_64" ]]; then
    die "Linux builds currently ship only for x86_64."
  fi
  if [[ "$os" == "windows" ]]; then
    die "Use install.ps1 on Windows."
  fi

  printf '%s-%s\n' "$os" "$arch"
}

resolve_latest_tag() {
  local repo="$1"
  local channel="$2"
  local tag=""

  if [[ "$channel" == "stable" ]]; then
    local json
    json="$(curl_with_auth -fsSL "https://api.github.com/repos/${repo}/releases/latest")"
    tag="$(printf '%s\n' "$json" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  else
    local json current_tag=""
    json="$(curl_with_auth -fsSL "https://api.github.com/repos/${repo}/releases?per_page=50")"
    while IFS= read -r line; do
      if [[ "$line" =~ \"tag_name\"[[:space:]]*:[[:space:]]*\"([^\"]+)\" ]]; then
        current_tag="${BASH_REMATCH[1]}"
      fi
      if [[ "$line" =~ \"prerelease\"[[:space:]]*:[[:space:]]*true ]]; then
        if [[ -n "$current_tag" ]]; then
          tag="$current_tag"
          break
        fi
      fi
    done <<< "$json"
  fi

  [[ -n "$tag" ]] || die "Unable to resolve latest ${channel} release tag from GitHub."
  printf '%s\n' "$tag"
}

parse_manifest() {
  local manifest_path="$1"
  local target="$2"
  local output_path="$3"

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$manifest_path" "$target" > "$output_path" <<'PY'
import json
import sys

manifest_path = sys.argv[1]
target = sys.argv[2]
manifest = json.load(open(manifest_path, "r", encoding="utf-8"))
entry = next((item for item in manifest["targets"] if item["target"] == target), None)
if entry is None:
    raise SystemExit(f"No target entry for {target}")
print(f'TARGET_ARCHIVE={entry["archive"]}')
print(f'TARGET_URL={entry["url"]}')
print(f'TARGET_CHECKSUM={entry["checksum"]}')
print(f'SKILL_URL={manifest["skill"]["url"]}')
print(f'SKILL_CHECKSUM={manifest["skill"]["checksum"]}')
PY
    return
  fi

  if command -v node >/dev/null 2>&1; then
    node - "$manifest_path" "$target" > "$output_path" <<'NODE'
const fs = require("fs");

const manifestPath = process.argv[2];
const target = process.argv[3];
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const entry = manifest.targets.find((item) => item.target === target);
if (!entry) {
  throw new Error(`No target entry for ${target}`);
}
console.log(`TARGET_ARCHIVE=${entry.archive}`);
console.log(`TARGET_URL=${entry.url}`);
console.log(`TARGET_CHECKSUM=${entry.checksum}`);
console.log(`SKILL_URL=${manifest.skill.url}`);
console.log(`SKILL_CHECKSUM=${manifest.skill.checksum}`);
NODE
    return
  fi

  die "Unable to parse manifest.json. Install python3 or node."
}

should_use_claude() {
  [[ "$AGENT" == "claude" || "$AGENT" == "both" ]]
}

should_use_codex() {
  [[ "$AGENT" == "codex" || "$AGENT" == "both" ]]
}

create_wrapper() {
  local wrapper_path="$1"
  local real_basename="$2"
  cat > "$wrapper_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
exec "\${SCRIPT_DIR}/${real_basename}" "\$@"
EOF
  chmod +x "$wrapper_path"
}

run_binary_self_test() {
  local binary_path="$1"
  local label="$2"
  local version_out="$TMP_DIR/self-test-${label}-version.txt"
  local report_out="$TMP_DIR/self-test-${label}.json"
  local stderr_out="$TMP_DIR/self-test-${label}.stderr"

  if ! run_with_timeout 6 "$binary_path" --version >"$version_out" 2>"$stderr_out"; then
    warn "Version check failed for ${binary_path}:"
    cat "$stderr_out" >&2 || true
    return 1
  fi
  if [[ -z "$(tr -d '[:space:]' < "$version_out")" ]]; then
    warn "Version check produced empty output for ${binary_path}"
    return 1
  fi

  if [[ "$SKIP_SELF_TEST" -eq 1 ]]; then
    return 0
  fi

  if ! run_with_timeout 20 "$binary_path" self-test --json >"$report_out" 2>"$stderr_out"; then
    warn "Self-test failed for ${binary_path}:"
    cat "$stderr_out" >&2 || true
    return 1
  fi

  if ! grep -qi '"passed"[[:space:]]*:[[:space:]]*true' "$report_out"; then
    warn "Self-test reported failure for ${binary_path}:"
    cat "$report_out" >&2 || true
    return 1
  fi
  return 0
}

verify_claude_mcp() {
  local output_file="$TMP_DIR/claude-mcp-get.txt"
  if ! run_with_timeout 25 claude mcp get aitrium-radiotherapy >"$output_file" 2>&1; then
    CLAUDE_VERIFY_STATUS="failed"
    warn "Claude MCP verification command failed."
    return 1
  fi

  if grep -qi "failed to connect" "$output_file"; then
    CLAUDE_VERIFY_STATUS="failed"
    warn "Claude reports aitrium-radiotherapy MCP failed to connect."
    return 1
  fi

  CLAUDE_VERIFY_STATUS="verified"
  return 0
}

verify_codex_mcp() {
  local output_file="$TMP_DIR/codex-mcp-get.json"
  if ! run_with_timeout 20 codex mcp get aitrium-radiotherapy --json >"$output_file" 2>&1; then
    CODEX_VERIFY_STATUS="failed"
    warn "Codex MCP verification command failed."
    return 1
  fi

  if ! grep -q '"name"[[:space:]]*:[[:space:]]*"aitrium-radiotherapy"' "$output_file"; then
    CODEX_VERIFY_STATUS="failed"
    warn "Codex MCP configuration not found for aitrium-radiotherapy."
    return 1
  fi

  CODEX_VERIFY_STATUS="verified"
  return 0
}

verify_runtime_and_mcp() {
  local wrapper_bin="$1"

  if [[ "$SKIP_SELF_TEST" -eq 1 ]]; then
    RUNTIME_SELF_TEST_STATUS="skipped (--skip-self-test)"
  else
    if run_binary_self_test "$wrapper_bin" "installed"; then
      RUNTIME_SELF_TEST_STATUS="passed"
    else
      RUNTIME_SELF_TEST_STATUS="failed"
      return 1
    fi
  fi

  if [[ "$NO_MCP" -eq 1 ]]; then
    return 0
  fi

  if should_use_claude && [[ "$CLAUDE_MCP_STATUS" == "configured" ]]; then
    if ! verify_claude_mcp; then
      return 1
    fi
  fi

  if should_use_codex && [[ "$CODEX_MCP_STATUS" == "configured" ]]; then
    if ! verify_codex_mcp; then
      return 1
    fi
  fi

  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      [[ -n "$VERSION" ]] || die "Missing value for --version"
      shift 2
      ;;
    --channel)
      CHANNEL="${2:-}"
      [[ "$CHANNEL" == "stable" || "$CHANNEL" == "beta" ]] || die "Invalid channel: $CHANNEL"
      shift 2
      ;;
    --agent)
      AGENT="${2:-}"
      [[ "$AGENT" == "codex" || "$AGENT" == "claude" || "$AGENT" == "both" || "$AGENT" == "none" ]] || die "Invalid --agent value: $AGENT"
      shift 2
      ;;
    --no-skill)
      NO_SKILL=1
      shift
      ;;
    --no-mcp)
      NO_MCP=1
      shift
      ;;
    --skip-self-test)
      SKIP_SELF_TEST=1
      shift
      ;;
    --self-test-only)
      SELF_TEST_ONLY=1
      shift
      ;;
    --verify-mcp-only)
      VERIFY_MCP_ONLY=1
      shift
      ;;
    --bin-dir)
      BIN_DIR="${2:-}"
      [[ -n "$BIN_DIR" ]] || die "Missing value for --bin-dir"
      shift 2
      ;;
    --repo)
      INSTALL_REPO="${2:-}"
      [[ -n "$INSTALL_REPO" ]] || die "Missing value for --repo"
      shift 2
      ;;
    --release-base-url)
      RELEASE_BASE_URL="${2:-}"
      [[ -n "$RELEASE_BASE_URL" ]] || die "Missing value for --release-base-url"
      shift 2
      ;;
    --manifest-url)
      MANIFEST_URL="${2:-}"
      [[ -n "$MANIFEST_URL" ]] || die "Missing value for --manifest-url"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1"
      ;;
  esac
done

if [[ "$SELF_TEST_ONLY" -eq 1 && "$VERIFY_MCP_ONLY" -eq 1 ]]; then
  die "--self-test-only and --verify-mcp-only cannot be used together."
fi
if [[ "$SELF_TEST_ONLY" -eq 1 && "$SKIP_SELF_TEST" -eq 1 ]]; then
  die "--self-test-only cannot be used with --skip-self-test."
fi

TARGET="$(detect_target)"
TARGET_WRAPPER="$BIN_DIR/$SERVER_NAME"
TARGET_REAL="$BIN_DIR/${SERVER_NAME}.bin"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ "$SELF_TEST_ONLY" -eq 1 ]]; then
  [[ -x "$TARGET_WRAPPER" ]] || die "No installed binary found at ${TARGET_WRAPPER}"
  if run_binary_self_test "$TARGET_WRAPPER" "local-only"; then
    log "Self-test passed for installed binary: ${TARGET_WRAPPER}"
    exit 0
  fi
  die "Self-test failed for installed binary: ${TARGET_WRAPPER}"
fi

if [[ "$VERIFY_MCP_ONLY" -eq 1 ]]; then
  local_ok=1
  if [[ -x "$TARGET_WRAPPER" ]]; then
    if ! run_binary_self_test "$TARGET_WRAPPER" "verify-only"; then
      local_ok=0
    fi
  else
    warn "Installed wrapper not found at ${TARGET_WRAPPER}; skipping local runtime check."
  fi

  verify_ok=1
  if should_use_claude; then
    if command -v claude >/dev/null 2>&1; then
      verify_claude_mcp || verify_ok=0
    else
      warn "claude CLI not found; cannot verify Claude MCP."
      verify_ok=0
    fi
  fi
  if should_use_codex; then
    if command -v codex >/dev/null 2>&1; then
      verify_codex_mcp || verify_ok=0
    else
      warn "codex CLI not found; cannot verify Codex MCP."
      verify_ok=0
    fi
  fi

  if [[ "$local_ok" -eq 1 && "$verify_ok" -eq 1 ]]; then
    log "MCP verification passed."
    exit 0
  fi
  die "MCP verification failed."
fi

TAG=""
if [[ -z "$MANIFEST_URL" && -z "$RELEASE_BASE_URL" ]]; then
  if [[ "$VERSION" == "latest" ]]; then
    TAG="$(resolve_latest_tag "$INSTALL_REPO" "$CHANNEL")"
  else
    if [[ "$VERSION" == aitrium-radiotherapy-v* ]]; then
      TAG="$VERSION"
    else
      TAG="aitrium-radiotherapy-v${VERSION}"
    fi
  fi
fi

MANIFEST_PATH="$TMP_DIR/manifest.json"
if [[ -n "$MANIFEST_URL" ]]; then
  log "Downloading manifest: ${MANIFEST_URL}"
  download_to "$MANIFEST_URL" "$MANIFEST_PATH"
else
  if [[ -z "$RELEASE_BASE_URL" ]]; then
    RELEASE_BASE_URL="https://github.com/${INSTALL_REPO}/releases/download/${TAG}"
  fi
  RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
  log "Downloading manifest: ${RELEASE_BASE_URL}/manifest.json"
  download_to "${RELEASE_BASE_URL}/manifest.json" "$MANIFEST_PATH"
fi

MANIFEST_ENV="$TMP_DIR/manifest.env"
parse_manifest "$MANIFEST_PATH" "$TARGET" "$MANIFEST_ENV"
source "$MANIFEST_ENV"

if [[ -z "$TAG" ]]; then
  if [[ "$VERSION" == "latest" || -z "$VERSION" ]]; then
    TAG="custom"
  elif [[ "$VERSION" == aitrium-radiotherapy-v* ]]; then
    TAG="$VERSION"
  else
    TAG="aitrium-radiotherapy-v${VERSION}"
  fi
fi

[[ -n "${TARGET_URL:-}" ]] || die "Manifest parse failed (missing TARGET_URL)."
[[ -n "${TARGET_CHECKSUM:-}" ]] || die "Manifest parse failed (missing TARGET_CHECKSUM)."

ARCHIVE_PATH="$TMP_DIR/${TARGET_ARCHIVE}"
log "Downloading binary archive for ${TARGET}: ${TARGET_URL}"
download_to "$TARGET_URL" "$ARCHIVE_PATH"

ARCHIVE_SHA="$(sha256_file "$ARCHIVE_PATH")"
if [[ "$(to_lower "$ARCHIVE_SHA")" != "$(to_lower "$TARGET_CHECKSUM")" ]]; then
  die "Checksum mismatch for ${TARGET_ARCHIVE}. Expected ${TARGET_CHECKSUM}, got ${ARCHIVE_SHA}."
fi

EXTRACT_DIR="$TMP_DIR/extract"
mkdir -p "$EXTRACT_DIR"
case "$TARGET_ARCHIVE" in
  *.tar.gz)
    tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT_DIR"
    ;;
  *.zip)
    if ! command -v unzip >/dev/null 2>&1; then
      die "unzip is required to install ${TARGET_ARCHIVE}."
    fi
    unzip -q "$ARCHIVE_PATH" -d "$EXTRACT_DIR"
    ;;
  *)
    die "Unsupported archive format: $TARGET_ARCHIVE"
    ;;
esac

SOURCE_BIN="$EXTRACT_DIR/$SERVER_NAME"
[[ -f "$SOURCE_BIN" ]] || die "Installed archive missing expected binary."

mkdir -p "$BIN_DIR"
STAGED_BIN="$TMP_DIR/${SERVER_NAME}.staged"
cp "$SOURCE_BIN" "$STAGED_BIN"
chmod +x "$STAGED_BIN"

if [[ "$SKIP_SELF_TEST" -eq 1 ]]; then
  RUNTIME_SELF_TEST_STATUS="skipped (--skip-self-test)"
else
  if ! run_binary_self_test "$STAGED_BIN" "staged"; then
    die "Downloaded binary failed self-test. Installation aborted."
  fi
  RUNTIME_SELF_TEST_STATUS="passed (staged)"
fi

BACKUP_WRAPPER=""
BACKUP_REAL=""
if [[ -f "$TARGET_WRAPPER" ]]; then
  BACKUP_WRAPPER="$TMP_DIR/backup-wrapper"
  cp "$TARGET_WRAPPER" "$BACKUP_WRAPPER"
fi
if [[ -f "$TARGET_REAL" ]]; then
  BACKUP_REAL="$TMP_DIR/backup-real"
  cp "$TARGET_REAL" "$BACKUP_REAL"
fi

rollback_binaries() {
  if [[ -n "$BACKUP_REAL" && -f "$BACKUP_REAL" ]]; then
    cp "$BACKUP_REAL" "$TARGET_REAL"
    chmod +x "$TARGET_REAL"
  else
    rm -f "$TARGET_REAL"
  fi

  if [[ -n "$BACKUP_WRAPPER" && -f "$BACKUP_WRAPPER" ]]; then
    cp "$BACKUP_WRAPPER" "$TARGET_WRAPPER"
    chmod +x "$TARGET_WRAPPER"
  else
    rm -f "$TARGET_WRAPPER"
  fi
}

REAL_TMP="$TMP_DIR/${SERVER_NAME}.bin.tmp"
WRAPPER_TMP="$TMP_DIR/${SERVER_NAME}.wrapper.tmp"
cp "$STAGED_BIN" "$REAL_TMP"
chmod +x "$REAL_TMP"
create_wrapper "$WRAPPER_TMP" "$(basename "$TARGET_REAL")"

if ! mv "$REAL_TMP" "$TARGET_REAL"; then
  rollback_binaries
  die "Failed to install runtime binary to ${TARGET_REAL}"
fi
if ! mv "$WRAPPER_TMP" "$TARGET_WRAPPER"; then
  rollback_binaries
  die "Failed to install wrapper to ${TARGET_WRAPPER}"
fi
chmod +x "$TARGET_REAL" "$TARGET_WRAPPER"

if [[ "$NO_SKILL" -eq 0 ]]; then
  SKILL_PATH="$TMP_DIR/aitrium-radiotherapy-skill.tar.gz"
  log "Downloading skill package: ${SKILL_URL}"
  download_to "$SKILL_URL" "$SKILL_PATH"
  SKILL_SHA="$(sha256_file "$SKILL_PATH")"
  if [[ "$(to_lower "$SKILL_SHA")" != "$(to_lower "$SKILL_CHECKSUM")" ]]; then
    rollback_binaries
    die "Checksum mismatch for aitrium-radiotherapy-skill.tar.gz. Expected ${SKILL_CHECKSUM}, got ${SKILL_SHA}."
  fi

  if should_use_claude; then
    CLAUDE_SKILL_DIR="${HOME}/.claude/skills/aitrium-radiotherapy"
    mkdir -p "$CLAUDE_SKILL_DIR"
    tar -xzf "$SKILL_PATH" -C "$CLAUDE_SKILL_DIR" "SKILL.md"
    CLAUDE_SKILL_STATUS="installed"
  fi
  if should_use_codex; then
    CODEX_HOME_DIR="${CODEX_HOME:-${HOME}/.codex}"
    CODEX_SKILL_DIR="${CODEX_HOME_DIR}/skills/aitrium-radiotherapy"
    mkdir -p "$CODEX_SKILL_DIR"
    tar -xzf "$SKILL_PATH" -C "$CODEX_SKILL_DIR" "SKILL.md"
    CODEX_SKILL_STATUS="installed"
  fi
fi

if [[ "$NO_MCP" -eq 0 ]]; then
  if should_use_claude; then
    if command -v claude >/dev/null 2>&1; then
      run_with_timeout 10 claude mcp remove aitrium-radiotherapy -s user >/dev/null 2>&1 || true
      if run_with_timeout 20 claude mcp add --scope user aitrium-radiotherapy "$TARGET_WRAPPER" >/dev/null 2>&1; then
        CLAUDE_MCP_STATUS="configured"
      else
        CLAUDE_MCP_STATUS="failed (run manually)"
      fi
    else
      CLAUDE_MCP_STATUS="claude CLI not found"
    fi
  fi

  if should_use_codex; then
    if command -v codex >/dev/null 2>&1; then
      run_with_timeout 10 codex mcp remove aitrium-radiotherapy >/dev/null 2>&1 || true
      if run_with_timeout 20 codex mcp add aitrium-radiotherapy -- "$TARGET_WRAPPER" >/dev/null 2>&1; then
        CODEX_MCP_STATUS="configured"
      else
        CODEX_MCP_STATUS="failed (run manually)"
      fi
    else
      CODEX_MCP_STATUS="codex CLI not found"
    fi
  fi
fi

if ! verify_runtime_and_mcp "$TARGET_WRAPPER"; then
  rollback_binaries
  if should_use_claude && command -v claude >/dev/null 2>&1; then
    run_with_timeout 10 claude mcp remove aitrium-radiotherapy -s user >/dev/null 2>&1 || true
  fi
  if should_use_codex && command -v codex >/dev/null 2>&1; then
    run_with_timeout 10 codex mcp remove aitrium-radiotherapy >/dev/null 2>&1 || true
  fi
  die "Install verification failed; previous binary restored."
fi

log "Install summary:"
log "  tag: ${TAG}"
log "  target: ${TARGET}"
log "  wrapper: ${TARGET_WRAPPER}"
log "  runtime: ${TARGET_REAL}"
log "  runtime self-test: ${RUNTIME_SELF_TEST_STATUS}"
log "  claude skill: ${CLAUDE_SKILL_STATUS}"
log "  codex skill: ${CODEX_SKILL_STATUS}"
log "  claude mcp: ${CLAUDE_MCP_STATUS}"
log "  codex mcp: ${CODEX_MCP_STATUS}"
log "  claude verify: ${CLAUDE_VERIFY_STATUS}"
log "  codex verify: ${CODEX_VERIFY_STATUS}"
log "Ensure ${BIN_DIR} is on PATH."

if should_use_claude && [[ "${CLAUDE_MCP_STATUS}" != "configured" ]]; then
  warn "Claude manual MCP command:"
  warn "  claude mcp add --scope user aitrium-radiotherapy ${TARGET_WRAPPER}"
fi
if should_use_codex && [[ "${CODEX_MCP_STATUS}" != "configured" ]]; then
  warn "Codex manual MCP command:"
  warn "  codex mcp add aitrium-radiotherapy -- ${TARGET_WRAPPER}"
fi
