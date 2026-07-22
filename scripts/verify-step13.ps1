[CmdletBinding()]
param(
    [string]$Toolchain = "stable",
    [string]$GnuToolWrapper,
    [string]$RustLld
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SummaryDir = Join-Path $RepoRoot "target\verification"
$SummaryPath = Join-Path $SummaryDir "step13-summary.json"
$StartedAt = (Get-Date).ToUniversalTime().ToString("o")
$Commands = [System.Collections.Generic.List[object]]::new()
$ToolGaps = [System.Collections.Generic.List[string]]::new()
New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
if (Test-Path Env:CARGO_TARGET_DIR) { Remove-Item Env:CARGO_TARGET_DIR }

function Add-Gap([string]$Message) {
    $ToolGaps.Add($Message)
    Write-Host "[preflight] $Message" -ForegroundColor Yellow
}

function Resolve-Tool([string]$Name) {
    $Tool = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $Tool) { Add-Gap "required tool not found: $Name"; return $null }
    return $Tool.Source
}

function Test-Installed([string[]]$Lines, [string]$Prefix) {
    return [bool]($Lines | Where-Object { $_ -like "$Prefix*" -and $_ -match "\(installed\)" })
}

function Invoke-Gate {
    param([string]$Name, [string]$Exe, [string[]]$Args)
    Write-Host "`n== $Name ==" -ForegroundColor Cyan
    Push-Location $RepoRoot
    try { $Lines = & $Exe @Args 2>&1; $ProcessExit = $LASTEXITCODE }
    finally { Pop-Location }
    $Output = ($Lines | ForEach-Object { $_.ToString() }) -join "`n"
    if ($Output) { Write-Host $Output }
    $Matches = [regex]::Matches(
        $Output,
        "test result: (?:ok|FAILED)\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;[^;]*;\s+(\d+) filtered out"
    )
    $Passed = 0; $Failed = 0; $Ignored = 0; $Filtered = 0
    foreach ($Match in $Matches) {
        $Passed += [int]$Match.Groups[1].Value
        $Failed += [int]$Match.Groups[2].Value
        $Ignored += [int]$Match.Groups[3].Value
        $Filtered += [int]$Match.Groups[4].Value
    }
    $Discovered = $Passed + $Failed + $Ignored
    $Exit = $ProcessExit
    $IsTest = $Name -in @(
        "fp-targeted", "runtime-targeted", "identity-targeted", "primitive-targeted",
        "workspace-full", "tshiatun-full"
    )
    if ($IsTest -and ($Matches.Count -eq 0 -or $Discovered -eq 0 -or $Ignored -ne 0 -or $Filtered -ne 0)) {
        $Exit = 1
        Write-Host "test discovery gate failed" -ForegroundColor Red
    }
    $Commands.Add([ordered]@{
        name = $Name
        command = ((@($Exe) + $Args) -join " ")
        exit_code = $Exit
        process_exit_code = $ProcessExit
        result_lines = $Matches.Count
        discovered = $Discovered
        passed = $Passed
        failed = $Failed
        ignored = $Ignored
        filtered = $Filtered
    })
    return $Exit
}

function Write-Summary([int]$Exit, [string]$RootSha, [string]$Expected, [string]$Actual) {
    [ordered]@{
        schema = 1
        gate = "step13-primitive-edit"
        started_at_utc = $StartedAt
        finished_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        exit_code = $Exit
        toolchain = $Toolchain
        repository_sha = $RootSha
        submodule_expected_sha = $Expected
        submodule_actual_sha = $Actual
        tool_gaps = @($ToolGaps)
        commands = @($Commands)
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $SummaryPath
    Write-Host "summary: $SummaryPath"
}

$Git = Resolve-Tool "git"
$Cargo = Resolve-Tool "cargo"
$Rustup = Resolve-Tool "rustup"
$Rustc = Resolve-Tool "rustc"
$RootSha = ""; $Expected = ""; $Actual = ""

if ($Git) {
    $SafeRoot = $RepoRoot.Replace("\", "/")
    $RootSha = (& $Git -c "safe.directory=$SafeRoot" -C $RepoRoot rev-parse HEAD 2>$null).Trim()
    $TreeLine = (& $Git -c "safe.directory=$SafeRoot" -C $RepoRoot ls-tree HEAD tshiatun 2>$null)
    if ($TreeLine -match "^160000 commit ([0-9a-f]{40})") { $Expected = $Matches[1] }
    else { Add-Gap "tshiatun gitlink is missing from HEAD" }
    $Submodule = Join-Path $RepoRoot "tshiatun"
    if (-not (Test-Path (Join-Path $Submodule "Cargo.toml"))) {
        Add-Gap "tshiatun submodule is not initialized"
    } else {
        $SafeSubmodule = $Submodule.Replace("\", "/")
        $Actual = (& $Git -c "safe.directory=$SafeSubmodule" -C $Submodule rev-parse HEAD 2>$null).Trim()
        if ($Expected -and $Actual -ne $Expected) { Add-Gap "tshiatun HEAD differs from gitlink" }
        if (@(& $Git -c "safe.directory=$SafeSubmodule" -C $Submodule status --porcelain).Count -gt 0) {
            Add-Gap "tshiatun worktree has changes"
        }
    }
}

if ($Rustup) {
    $Components = @(& $Rustup component list --toolchain $Toolchain 2>$null)
    if ($LASTEXITCODE -ne 0) { Add-Gap "Rust toolchain unavailable: $Toolchain" }
    else {
        if (-not (Test-Installed $Components "rustfmt-")) { Add-Gap "rustfmt missing for $Toolchain" }
        if (-not (Test-Installed $Components "clippy-")) { Add-Gap "selected toolchain has no Clippy component: $Toolchain" }
        $Targets = @(& $Rustup target list --installed --toolchain $Toolchain 2>$null)
        if ($Targets -notcontains "wasm32-unknown-unknown") { Add-Gap "WASM target missing" }
    }
}

if ($Rustc) {
    $Info = @(& $Rustc "+$Toolchain" -vV 2>$null)
    $HostLine = $Info | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    $HostTriple = if ($HostLine) { ($HostLine -split ":", 2)[1].Trim() } else { "" }
    if ($HostTriple -like "*-gnu") {
        $Sysroot = (& $Rustc "+$Toolchain" --print sysroot).Trim()
        if ($GnuToolWrapper -and (Test-Path $GnuToolWrapper)) {
            $env:Path = (Resolve-Path $GnuToolWrapper).Path + ";" + $env:Path
        } elseif ($GnuToolWrapper) { Add-Gap "GNU tool-wrapper not found: $GnuToolWrapper" }
        $Lld = if ($RustLld) { $RustLld } else {
            Join-Path $Sysroot "lib\rustlib\$HostTriple\bin\rust-lld.exe"
        }
        if (Test-Path $Lld) { $env:RUST_LLD = (Resolve-Path $Lld).Path }
        else { Add-Gap "rust-lld missing: $Lld" }
        if ($null -eq (Get-Command dlltool.exe -ErrorAction SilentlyContinue)) {
            Add-Gap "dlltool.exe missing"
        }
    } elseif ($HostTriple -like "*-msvc") {
        $Link = Get-Command link.exe -ErrorAction SilentlyContinue
        if ($null -eq $Link -or -not $env:LIB -or $Link.Source -match "anaconda") {
            Add-Gap "MSVC linker/library environment is not initialized"
        }
    } else { Add-Gap "could not determine Rust host" }
}

if ($ToolGaps.Count -gt 0) { Write-Summary 2 $RootSha $Expected $Actual; exit 2 }

$Prefix = @("+$Toolchain")
$FailedGate = $false
$Gates = @(
    @{ Name = "rustfmt"; Args = $Prefix + @("fmt", "--all", "--", "--check") },
    @{ Name = "clippy"; Args = $Prefix + @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings") },
    @{ Name = "fp-targeted"; Args = $Prefix + @("test", "-p", "conlang-language", "--test", "fp_v2") },
    @{ Name = "runtime-targeted"; Args = $Prefix + @("test", "-p", "conlang-language", "--test", "runtime_sealing") },
    @{ Name = "identity-targeted"; Args = $Prefix + @("test", "-p", "conlang-language", "--test", "identity_sidecar") },
    @{ Name = "primitive-targeted"; Args = $Prefix + @("test", "-p", "conlang-changeset", "--test", "primitive_edits") },
    @{ Name = "workspace-full"; Args = $Prefix + @("test", "--workspace") },
    @{ Name = "tshiatun-full"; Args = $Prefix + @("test", "--manifest-path", "tshiatun/Cargo.toml") },
    @{ Name = "tshiatun-core-wasm"; Args = $Prefix + @("build", "--manifest-path", "tshiatun/Cargo.toml", "-p", "tshiatun-core", "--target", "wasm32-unknown-unknown") }
)
foreach ($Gate in $Gates) {
    if ((Invoke-Gate $Gate.Name $Cargo $Gate.Args) -ne 0) { $FailedGate = $true }
}
$Exit = if ($FailedGate) { 1 } else { 0 }
Write-Summary $Exit $RootSha $Expected $Actual
exit $Exit
