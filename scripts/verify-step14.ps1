[CmdletBinding()]
param(
    [string]$LintToolchain = "stable-x86_64-pc-windows-msvc",
    [string]$BuildToolchain = "stable-x86_64-pc-windows-gnu",
    [string]$GnuToolWrapper,
    [string]$RustLld
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SummaryDir = Join-Path $RepoRoot "target\verification"
$SummaryPath = Join-Path $SummaryDir "step14-summary.json"
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

function Test-Component([string]$Toolchain, [string]$Name) {
    $Lines = @(& $script:Rustup component list --toolchain $Toolchain 2>$null)
    return [bool]($Lines | Where-Object { $_ -like "$Name-*" -and $_ -match "\(installed\)" })
}

function Toolchain-Info([string]$Toolchain) {
    $Lines = @(& $script:Rustup run $Toolchain rustc -vV 2>$null)
    $HostLine = $Lines | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    return [ordered]@{
        toolchain = $Toolchain
        host = if ($HostLine) { ($HostLine -split ":", 2)[1].Trim() } else { "" }
        rustc = ($Lines -join "`n")
        clippy = ""
        rustfmt = ""
    }
}

function Invoke-Gate {
    param([string]$Name, [string]$Toolchain, [string[]]$CargoArgs, [bool]$IsTest = $false)
    $Args = @("run", $Toolchain, "cargo") + $CargoArgs
    Write-Host "`n== $Name ($Toolchain) ==" -ForegroundColor Cyan
    Push-Location $RepoRoot
    try { $Lines = & $script:Rustup @Args 2>&1; $ProcessExit = $LASTEXITCODE }
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
    if ($IsTest -and ($Matches.Count -eq 0 -or $Discovered -eq 0 -or $Ignored -ne 0 -or $Filtered -ne 0)) {
        $Exit = 1
        Write-Host "test discovery gate failed" -ForegroundColor Red
    }
    $Commands.Add([ordered]@{
        name = $Name
        toolchain = $Toolchain
        command = ((@($script:Rustup) + $Args) -join " ")
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

function Write-Summary([int]$Exit, [string]$RootSha, [string]$Expected, [string]$Actual, $LintInfo, $BuildInfo) {
    [ordered]@{
        schema = 1
        gate = "step14-changeset-synchronic-seal"
        started_at_utc = $StartedAt
        finished_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        exit_code = $Exit
        lint_toolchain = $LintInfo
        build_toolchain = $BuildInfo
        repository_sha = $RootSha
        submodule_expected_sha = $Expected
        submodule_actual_sha = $Actual
        tool_gaps = @($ToolGaps)
        commands = @($Commands)
    } | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 $SummaryPath
    Write-Host "summary: $SummaryPath"
}

$Git = Resolve-Tool "git"
$Cargo = Resolve-Tool "cargo"
$script:Rustup = Resolve-Tool "rustup"
$RootSha = ""; $Expected = ""; $Actual = ""
$LintInfo = $null; $BuildInfo = $null

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

if ($script:Rustup) {
    $LintInfo = Toolchain-Info $LintToolchain
    $BuildInfo = Toolchain-Info $BuildToolchain
    if (-not $LintInfo.host) { Add-Gap "lint toolchain unavailable: $LintToolchain" }
    else {
        if (-not (Test-Component $LintToolchain "rustfmt")) { Add-Gap "rustfmt missing for lint toolchain $LintToolchain" }
        if (-not (Test-Component $LintToolchain "clippy")) { Add-Gap "Clippy missing for lint toolchain $LintToolchain" }
        if (Test-Component $LintToolchain "rustfmt") {
            $LintInfo.rustfmt = (@(& $script:Rustup run $LintToolchain rustfmt --version 2>$null) -join "`n")
        }
        if (Test-Component $LintToolchain "clippy") {
            $LintInfo.clippy = (@(& $script:Rustup run $LintToolchain clippy-driver --version 2>$null) -join "`n")
        }
        if ($LintInfo.host -like "*-msvc") {
            $LintLink = Get-Command link.exe -ErrorAction SilentlyContinue
            if ($null -eq $LintLink -or -not $env:LIB -or $LintLink.Source -match "anaconda") {
                Add-Gap "MSVC linker/library environment is not initialized for lint toolchain (Clippy is installed)"
            }
        }
    }
    if (-not $BuildInfo.host) { Add-Gap "build toolchain unavailable: $BuildToolchain" }
    else {
        $Targets = @(& $script:Rustup target list --installed --toolchain $BuildToolchain 2>$null)
        if ($Targets -notcontains "wasm32-unknown-unknown") { Add-Gap "WASM target missing for $BuildToolchain" }
        if ($BuildInfo.host -like "*-gnu") {
            $Sysroot = (& $script:Rustup run $BuildToolchain rustc --print sysroot).Trim()
            $env:Path = (Join-Path $Sysroot "bin") + ";" + $env:Path
            if (-not $GnuToolWrapper) {
                $BundledWrapper = Join-Path $RepoRoot "target\gnu\tool-wrapper"
                if (Test-Path $BundledWrapper) { $GnuToolWrapper = $BundledWrapper }
            }
            if ($GnuToolWrapper) {
                if (Test-Path $GnuToolWrapper) { $env:Path = (Resolve-Path $GnuToolWrapper).Path + ";" + $env:Path }
                else { Add-Gap "GNU tool-wrapper not found: $GnuToolWrapper" }
            }
            $Lld = if ($RustLld) { $RustLld } else { Join-Path $Sysroot "lib\rustlib\$($BuildInfo.host)\bin\rust-lld.exe" }
            if (Test-Path $Lld) { $env:RUST_LLD = (Resolve-Path $Lld).Path }
            else { Add-Gap "rust-lld missing for build toolchain: $Lld" }
        } elseif ($BuildInfo.host -like "*-msvc") {
            $Link = Get-Command link.exe -ErrorAction SilentlyContinue
            if ($null -eq $Link -or -not $env:LIB -or $Link.Source -match "anaconda") {
                Add-Gap "MSVC linker/library environment is not initialized for build toolchain"
            }
        }
    }
}

if ($ToolGaps.Count -gt 0) {
    Write-Summary 2 $RootSha $Expected $Actual $LintInfo $BuildInfo
    exit 2
}

$FailedGate = $false
$Gates = @(
    @{ Name = "rustfmt"; Toolchain = $LintToolchain; Args = @("fmt", "--all", "--", "--check"); Test = $false },
    @{ Name = "clippy"; Toolchain = $LintToolchain; Args = @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings"); Test = $false },
    @{ Name = "context-occurrence"; Toolchain = $BuildToolchain; Args = @("test", "-p", "conlang-language", "--test", "slot_feature_bindings"); Test = $true },
    @{ Name = "identity-v2"; Toolchain = $BuildToolchain; Args = @("test", "-p", "conlang-language", "--test", "identity_sidecar"); Test = $true },
    @{ Name = "changeset-interpreter"; Toolchain = $BuildToolchain; Args = @("test", "-p", "conlang-changeset", "--test", "step14_interpreter"); Test = $true },
    @{ Name = "tutorial-examples"; Toolchain = $BuildToolchain; Args = @("test", "-p", "conlang-language", "--test", "tutorial_examples"); Test = $true },
    @{ Name = "workspace-full"; Toolchain = $BuildToolchain; Args = @("test", "--workspace"); Test = $true },
    @{ Name = "tshiatun-full"; Toolchain = $BuildToolchain; Args = @("test", "--manifest-path", "tshiatun/Cargo.toml"); Test = $true },
    @{ Name = "tshiatun-core-wasm"; Toolchain = $BuildToolchain; Args = @("build", "--manifest-path", "tshiatun/Cargo.toml", "-p", "tshiatun-core", "--target", "wasm32-unknown-unknown"); Test = $false }
)
foreach ($Gate in $Gates) {
    if ((Invoke-Gate $Gate.Name $Gate.Toolchain $Gate.Args $Gate.Test) -ne 0) { $FailedGate = $true }
}
$Exit = if ($FailedGate) { 1 } else { 0 }
Write-Summary $Exit $RootSha $Expected $Actual $LintInfo $BuildInfo
exit $Exit
