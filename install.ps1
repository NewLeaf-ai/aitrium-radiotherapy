param(
  [string]$Version = "latest",
  [ValidateSet("stable", "beta")][string]$Channel = "stable",
  [ValidateSet("codex", "claude", "both", "none")][string]$Agent = "both",
  [string]$Repo = "",
  [switch]$NoSkill,
  [switch]$NoMcp,
  [switch]$SkipSelfTest,
  [switch]$SelfTestOnly,
  [switch]$VerifyMcpOnly,
  [string]$BinDir
)

$ErrorActionPreference = "Stop"

function Write-Info($Message) {
  Write-Host "[aitrium-radiotherapy] $Message"
}

function Write-WarnLine($Message) {
  Write-Warning "[aitrium-radiotherapy] $Message"
}

function Resolve-Tag([string]$Repo, [string]$RequestedVersion, [string]$RequestedChannel) {
  if ($RequestedVersion -ne "latest") {
    if ($RequestedVersion.StartsWith("aitrium-radiotherapy-v")) {
      return $RequestedVersion
    }
    return "aitrium-radiotherapy-v$RequestedVersion"
  }

  if ($RequestedChannel -eq "stable") {
    $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    if (-not $latest.tag_name) {
      throw "Unable to resolve latest stable release tag."
    }
    return [string]$latest.tag_name
  }

  $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=50"
  $beta = $releases | Where-Object { $_.prerelease -eq $true } | Select-Object -First 1
  if (-not $beta -or -not $beta.tag_name) {
    throw "Unable to resolve latest beta release tag."
  }
  return [string]$beta.tag_name
}

function Resolve-TargetName() {
  $os =
    if ($IsMacOS) { "darwin" }
    elseif ($IsLinux) { "linux" }
    elseif ($IsWindows) { "windows" }
    else { throw "Unsupported OS." }

  $archName = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
  $arch =
    switch ($archName) {
      "arm64" { "aarch64" }
      "x64" { "x86_64" }
      "x86_64" { "x86_64" }
      default { throw "Unsupported architecture: $archName" }
    }

  if ($os -eq "linux" -and $arch -ne "x86_64") {
    throw "Linux builds currently ship only for x86_64."
  }
  if ($os -eq "windows" -and $arch -ne "x86_64") {
    throw "Windows builds currently ship only for x86_64."
  }
  return "$os-$arch"
}

function Should-UseClaude([string]$AgentTarget) {
  return $AgentTarget -eq "claude" -or $AgentTarget -eq "both"
}

function Should-UseCodex([string]$AgentTarget) {
  return $AgentTarget -eq "codex" -or $AgentTarget -eq "both"
}

function Invoke-ProcessWithTimeout {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @(),
    [string]$StdInText = "",
    [int]$TimeoutSeconds = 20
  )

  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.FileName = $FilePath
  foreach ($arg in $Arguments) {
    [void]$startInfo.ArgumentList.Add($arg)
  }
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardInput = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true

  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to start process: $FilePath"
  }

  if ($StdInText) {
    $process.StandardInput.Write($StdInText)
  }
  $process.StandardInput.Close()

  if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
    try { $process.Kill($true) } catch {}
    throw "Timed out after ${TimeoutSeconds}s: $FilePath $($Arguments -join ' ')"
  }

  $stdout = $process.StandardOutput.ReadToEnd()
  $stderr = $process.StandardError.ReadToEnd()
  return [pscustomobject]@{
    ExitCode = $process.ExitCode
    StdOut = $stdout
    StdErr = $stderr
  }
}

function Create-LauncherWrapper([string]$WrapperPath, [string]$RuntimeBinaryName) {
  $wrapper = @"
@echo off
setlocal
set SCRIPT_DIR=%~dp0
"%SCRIPT_DIR%$RuntimeBinaryName" %*
"@
  Set-Content -Path $WrapperPath -Value $wrapper -Encoding ascii
}

function Run-BinarySelfTest([string]$BinaryPath, [string]$Label, [bool]$SkipSelfTestFlag) {
  $versionResult = Invoke-ProcessWithTimeout -FilePath $BinaryPath -Arguments @("--version") -TimeoutSeconds 6
  if ($versionResult.ExitCode -ne 0) {
    throw "Version check failed for '$Label': $($versionResult.StdErr)"
  }
  if (-not $versionResult.StdOut.Trim()) {
    throw "Version check returned empty output for '$Label'."
  }

  if ($SkipSelfTestFlag) {
    return
  }

  $selfTestResult = Invoke-ProcessWithTimeout -FilePath $BinaryPath -Arguments @("self-test", "--json") -TimeoutSeconds 20
  if ($selfTestResult.ExitCode -ne 0) {
    throw "Self-test command failed for '$Label': $($selfTestResult.StdErr)"
  }
  $selfTest = $selfTestResult.StdOut | ConvertFrom-Json
  if (-not $selfTest.passed) {
    throw "Self-test reported failure for '$Label'."
  }
}

function Verify-ClaudeMcp {
  $output = & claude mcp get aitrium-radiotherapy 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) {
    throw "Claude MCP verification command failed."
  }
  if ($output -match "Failed to connect") {
    throw "Claude reports aitrium-radiotherapy MCP failed to connect."
  }
}

function Verify-CodexMcp {
  $output = & codex mcp get aitrium-radiotherapy --json 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) {
    throw "Codex MCP verification command failed."
  }
  if ($output -notmatch '"name"\s*:\s*"aitrium-radiotherapy"') {
    throw "Codex MCP configuration for aitrium-radiotherapy was not found."
  }
}

if ($SelfTestOnly -and $VerifyMcpOnly) {
  throw "--self-test-only and --verify-mcp-only cannot be used together."
}
if ($SelfTestOnly -and $SkipSelfTest) {
  throw "--self-test-only cannot be used with --skip-self-test."
}

if (-not $BinDir) {
  if ($IsWindows) {
    $BinDir = Join-Path $HOME "AppData\Local\aitrium-radiotherapy\bin"
  } else {
    $BinDir = Join-Path $HOME ".local/bin"
  }
}

$wrapperPath = Join-Path $BinDir "aitrium-radiotherapy-server.cmd"
$runtimeBinaryPath = Join-Path $BinDir "aitrium-radiotherapy-server.bin.exe"
$legacyBinaryPath = Join-Path $BinDir "aitrium-radiotherapy-server.exe"

if ($SelfTestOnly) {
  $selfTestBinary = if (Test-Path $runtimeBinaryPath) { $runtimeBinaryPath } elseif (Test-Path $legacyBinaryPath) { $legacyBinaryPath } else { $null }
  if (-not $selfTestBinary) {
    throw "No installed runtime binary found in $BinDir."
  }
  Run-BinarySelfTest -BinaryPath $selfTestBinary -Label "installed" -SkipSelfTestFlag:$false
  Write-Info "Self-test passed for $selfTestBinary"
  exit 0
}

if ($VerifyMcpOnly) {
  if (Test-Path $runtimeBinaryPath) {
    Run-BinarySelfTest -BinaryPath $runtimeBinaryPath -Label "verify-only" -SkipSelfTestFlag:$SkipSelfTest.IsPresent
  } else {
    Write-WarnLine "Runtime binary not found at $runtimeBinaryPath; skipping local runtime check."
  }

  if (Should-UseClaude $Agent) {
    if (Get-Command claude -ErrorAction SilentlyContinue) {
      Verify-ClaudeMcp
    } else {
      throw "claude CLI not found for MCP verification."
    }
  }
  if (Should-UseCodex $Agent) {
    if (Get-Command codex -ErrorAction SilentlyContinue) {
      Verify-CodexMcp
    } else {
      throw "codex CLI not found for MCP verification."
    }
  }
  Write-Info "MCP verification passed."
  exit 0
}

$repo =
  if ($Repo) { $Repo }
  elseif ($env:AITRIUM_RADIOTHERAPY_GITHUB_REPO) { $env:AITRIUM_RADIOTHERAPY_GITHUB_REPO }
  else { "NewLeaf-ai/aitrium-radiotherapy" }
$targetName = Resolve-TargetName
$tag = Resolve-Tag -Repo $repo -RequestedVersion $Version -RequestedChannel $Channel
$releaseBase = "https://github.com/$repo/releases/download/$tag"

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("aitrium-radiotherapy-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -Path $tempRoot -ItemType Directory -Force | Out-Null

$claudeSkillStatus = "skipped"
$codexSkillStatus = "skipped"
$claudeMcpStatus = "skipped"
$codexMcpStatus = "skipped"
$claudeVerifyStatus = "skipped"
$codexVerifyStatus = "skipped"
$runtimeSelfTestStatus = "skipped"

$backupWrapper = $null
$backupRuntime = $null
$backupLegacy = $null

function Restore-Binaries {
  if ($backupRuntime -and (Test-Path $backupRuntime)) {
    Copy-Item -Path $backupRuntime -Destination $runtimeBinaryPath -Force
  } else {
    Remove-Item -Path $runtimeBinaryPath -Force -ErrorAction SilentlyContinue
  }

  if ($backupWrapper -and (Test-Path $backupWrapper)) {
    Copy-Item -Path $backupWrapper -Destination $wrapperPath -Force
  } else {
    Remove-Item -Path $wrapperPath -Force -ErrorAction SilentlyContinue
  }

  if ($backupLegacy -and (Test-Path $backupLegacy)) {
    Copy-Item -Path $backupLegacy -Destination $legacyBinaryPath -Force
  }
}

try {
  $manifestPath = Join-Path $tempRoot "manifest.json"
  Write-Info "Downloading manifest: $releaseBase/manifest.json"
  Invoke-WebRequest -Uri "$releaseBase/manifest.json" -OutFile $manifestPath
  $manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json

  $targetEntry = $manifest.targets | Where-Object { $_.target -eq $targetName } | Select-Object -First 1
  if (-not $targetEntry) {
    throw "No manifest target entry for '$targetName'."
  }

  $archiveName = [string]$targetEntry.archive
  $archiveUrl = [string]$targetEntry.url
  $archiveChecksum = [string]$targetEntry.checksum
  $archivePath = Join-Path $tempRoot $archiveName

  Write-Info "Downloading binary archive: $archiveUrl"
  Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath
  $actualArchiveChecksum = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualArchiveChecksum -ne $archiveChecksum.ToLowerInvariant()) {
    throw "Checksum mismatch for $archiveName. Expected $archiveChecksum, got $actualArchiveChecksum."
  }

  $extractDir = Join-Path $tempRoot "extract"
  New-Item -Path $extractDir -ItemType Directory -Force | Out-Null
  if ($archiveName.EndsWith(".zip")) {
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force
  } elseif ($archiveName.EndsWith(".tar.gz")) {
    tar -xzf $archivePath -C $extractDir
  } else {
    throw "Unsupported archive format: $archiveName"
  }

  $sourceBinary = Join-Path $extractDir "aitrium-radiotherapy-server.exe"
  if (-not (Test-Path $sourceBinary)) {
    $sourceBinary = Join-Path $extractDir "aitrium-radiotherapy-server"
  }
  if (-not (Test-Path $sourceBinary)) {
    throw "Expected binary was not found after archive extraction."
  }

  New-Item -Path $BinDir -ItemType Directory -Force | Out-Null
  if (Test-Path $wrapperPath) {
    $backupWrapper = Join-Path $tempRoot "backup-wrapper.cmd"
    Copy-Item -Path $wrapperPath -Destination $backupWrapper -Force
  }
  if (Test-Path $runtimeBinaryPath) {
    $backupRuntime = Join-Path $tempRoot "backup-runtime.exe"
    Copy-Item -Path $runtimeBinaryPath -Destination $backupRuntime -Force
  }
  if (Test-Path $legacyBinaryPath) {
    $backupLegacy = Join-Path $tempRoot "backup-legacy.exe"
    Copy-Item -Path $legacyBinaryPath -Destination $backupLegacy -Force
  }

  $stagedBinary = Join-Path $tempRoot "aitrium-radiotherapy-server.staged.exe"
  Copy-Item -Path $sourceBinary -Destination $stagedBinary -Force
  Run-BinarySelfTest -BinaryPath $stagedBinary -Label "staged" -SkipSelfTestFlag:$SkipSelfTest.IsPresent
  $runtimeSelfTestStatus = if ($SkipSelfTest) { "skipped (--skip-self-test)" } else { "passed (staged)" }

  $runtimeTmp = Join-Path $tempRoot "runtime.tmp.exe"
  Copy-Item -Path $stagedBinary -Destination $runtimeTmp -Force
  Move-Item -Path $runtimeTmp -Destination $runtimeBinaryPath -Force

  $wrapperTmp = Join-Path $tempRoot "aitrium-radiotherapy-server.tmp.cmd"
  Create-LauncherWrapper -WrapperPath $wrapperTmp -RuntimeBinaryName "aitrium-radiotherapy-server.bin.exe"
  Move-Item -Path $wrapperTmp -Destination $wrapperPath -Force

  if (-not $NoSkill) {
    $skillUrl = [string]$manifest.skill.url
    $skillChecksum = [string]$manifest.skill.checksum
    $skillArchive = Join-Path $tempRoot "aitrium-radiotherapy-skill.tar.gz"

    Write-Info "Downloading skill archive: $skillUrl"
    Invoke-WebRequest -Uri $skillUrl -OutFile $skillArchive
    $actualSkillChecksum = (Get-FileHash -Path $skillArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSkillChecksum -ne $skillChecksum.ToLowerInvariant()) {
      throw "Checksum mismatch for aitrium-radiotherapy-skill.tar.gz. Expected $skillChecksum, got $actualSkillChecksum."
    }

    if (Should-UseClaude $Agent) {
      $claudeSkillDir = Join-Path $HOME ".claude/skills/aitrium-radiotherapy"
      New-Item -Path $claudeSkillDir -ItemType Directory -Force | Out-Null
      tar -xzf $skillArchive -C $claudeSkillDir "SKILL.md"
      $claudeSkillStatus = "installed"
    }
    if (Should-UseCodex $Agent) {
      $codexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $HOME ".codex" }
      $codexSkillDir = Join-Path $codexHome "skills/aitrium-radiotherapy"
      New-Item -Path $codexSkillDir -ItemType Directory -Force | Out-Null
      tar -xzf $skillArchive -C $codexSkillDir "SKILL.md"
      $codexSkillStatus = "installed"
    }
  }

  if (-not $NoMcp) {
    if (Should-UseClaude $Agent) {
      if (Get-Command claude -ErrorAction SilentlyContinue) {
        try { & claude mcp remove aitrium-radiotherapy -s user | Out-Null } catch {}
        try {
          & claude mcp add --scope user aitrium-radiotherapy cmd /c $wrapperPath | Out-Null
          $claudeMcpStatus = "configured"
        } catch {
          $claudeMcpStatus = "failed (run manually)"
        }
      } else {
        $claudeMcpStatus = "claude CLI not found"
      }
    }

    if (Should-UseCodex $Agent) {
      if (Get-Command codex -ErrorAction SilentlyContinue) {
        try { & codex mcp remove aitrium-radiotherapy | Out-Null } catch {}
        try {
          & codex mcp add aitrium-radiotherapy -- cmd /c $wrapperPath | Out-Null
          $codexMcpStatus = "configured"
        } catch {
          $codexMcpStatus = "failed (run manually)"
        }
      } else {
        $codexMcpStatus = "codex CLI not found"
      }
    }
  }

  Run-BinarySelfTest -BinaryPath $runtimeBinaryPath -Label "installed" -SkipSelfTestFlag:$SkipSelfTest.IsPresent
  $runtimeSelfTestStatus = if ($SkipSelfTest) { "skipped (--skip-self-test)" } else { "passed" }

  if (-not $NoMcp) {
    if ((Should-UseClaude $Agent) -and $claudeMcpStatus -eq "configured") {
      try {
        Verify-ClaudeMcp
        $claudeVerifyStatus = "verified"
      } catch {
        $claudeVerifyStatus = "failed"
        throw
      }
    }
    if ((Should-UseCodex $Agent) -and $codexMcpStatus -eq "configured") {
      try {
        Verify-CodexMcp
        $codexVerifyStatus = "verified"
      } catch {
        $codexVerifyStatus = "failed"
        throw
      }
    }
  }

  Write-Info "Install summary:"
  Write-Info "  tag: $tag"
  Write-Info "  target: $targetName"
  Write-Info "  wrapper: $wrapperPath"
  Write-Info "  runtime: $runtimeBinaryPath"
  Write-Info "  runtime self-test: $runtimeSelfTestStatus"
  Write-Info "  claude skill: $claudeSkillStatus"
  Write-Info "  codex skill: $codexSkillStatus"
  Write-Info "  claude mcp: $claudeMcpStatus"
  Write-Info "  codex mcp: $codexMcpStatus"
  Write-Info "  claude verify: $claudeVerifyStatus"
  Write-Info "  codex verify: $codexVerifyStatus"

  if ((Should-UseClaude $Agent) -and $claudeMcpStatus -ne "configured") {
    Write-WarnLine "Claude manual MCP command: claude mcp add --scope user aitrium-radiotherapy cmd /c $wrapperPath"
  }
  if ((Should-UseCodex $Agent) -and $codexMcpStatus -ne "configured") {
    Write-WarnLine "Codex manual MCP command: codex mcp add aitrium-radiotherapy -- cmd /c $wrapperPath"
  }
}
catch {
  Restore-Binaries
  if (Should-UseClaude $Agent) {
    try { & claude mcp remove aitrium-radiotherapy -s user | Out-Null } catch {}
  }
  if (Should-UseCodex $Agent) {
    try { & codex mcp remove aitrium-radiotherapy | Out-Null } catch {}
  }
  throw
}
finally {
  if (Test-Path $tempRoot) {
    Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
