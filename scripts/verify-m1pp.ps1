[CmdletBinding()]
param(
    [string]$Toolchain = "stable",
    [string]$GnuToolWrapper,
    [string]$RustLld
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SummaryDir = Join-Path $RepoRoot "target\verification"
$SummaryPath = Join-Path $SummaryDir "m1pp-summary.json"
$StartedAt = (Get-Date).ToUniversalTime().ToString("o")
$ToolGaps = [System.Collections.Generic.List[string]]::new()
$Commands = [System.Collections.Generic.List[object]]::new()

New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
if (Test-Path Env:CARGO_TARGET_DIR) {
    Remove-Item Env:CARGO_TARGET_DIR
}

function Add-Gap([string]$Message) {
    $ToolGaps.Add($Message)
    Write-Host "[preflight] $Message" -ForegroundColor Yellow
}

function Resolve-Tool([string]$Name) {
    $Tool = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $Tool) {
        Add-Gap "required tool not found: $Name"
        return $null
    }
    return $Tool.Source
}

function Test-InstalledLine([string[]]$Lines, [string]$Prefix) {
    return [bool]($Lines | Where-Object { $_ -like "$Prefix*" -and $_ -match "\(installed\)" })
}

function Invoke-GateCommand {
    param(
        [string]$Name,
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )
    Write-Host "`n== $Name ==" -ForegroundColor Cyan
    Write-Host ((@($Executable) + $Arguments) -join " ")
    Push-Location $WorkingDirectory
    try {
        $OutputLines = & $Executable @Arguments 2>&1
        $ExitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    $Output = ($OutputLines | ForEach-Object { $_.ToString() }) -join "`n"
    if ($Output.Length -gt 0) {
        Write-Host $Output
    }
    $Passed = 0
    $Failed = 0
    $Ignored = 0
    $Filtered = 0
    $ResultMatches = [regex]::Matches(
        $Output,
        "test result: (?:ok|FAILED)\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;[^;]*;\s+(\d+) filtered out"
    )
    foreach ($Match in $ResultMatches) {
        $Passed += [int]$Match.Groups[1].Value
        $Failed += [int]$Match.Groups[2].Value
        $Ignored += [int]$Match.Groups[3].Value
        $Filtered += [int]$Match.Groups[4].Value
    }
    $Discovered = $Passed + $Failed + $Ignored
    $ProcessExitCode = $ExitCode
    $IsTestGate = $Name -in @("m1pp-targeted", "language-full", "tshiatun-full")
    if ($IsTestGate -and ($ResultMatches.Count -eq 0 -or $Discovered -eq 0 -or $Ignored -ne 0 -or $Filtered -ne 0)) {
        Write-Host "test discovery gate failed (results=$($ResultMatches.Count), discovered=$Discovered, ignored=$Ignored, filtered=$Filtered)" -ForegroundColor Red
        $ExitCode = 1
    }
    $Commands.Add([ordered]@{
        name = $Name
        command = ((@($Executable) + $Arguments) -join " ")
        working_directory = $WorkingDirectory
        exit_code = $ExitCode
        process_exit_code = $ProcessExitCode
        test_results_found = $ResultMatches.Count
        discovered = $Discovered
        passed = $Passed
        failed = $Failed
        ignored = $Ignored
        filtered = $Filtered
    })
    return $ExitCode
}

function Write-Summary([int]$ExitCode, [string]$RootSha, [string]$ExpectedSubmoduleSha, [string]$ActualSubmoduleSha) {
    $Summary = [ordered]@{
        schema = 1
        gate = "m1pp-p38-p44"
        started_at_utc = $StartedAt
        finished_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        exit_code = $ExitCode
        toolchain = $Toolchain
        repository_sha = $RootSha
        submodule_expected_sha = $ExpectedSubmoduleSha
        submodule_actual_sha = $ActualSubmoduleSha
        tool_gaps = @($ToolGaps)
        commands = @($Commands)
    }
    $Summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $SummaryPath
    Write-Host "`nsummary: $SummaryPath"
}

$Git = Resolve-Tool "git"
$Cargo = Resolve-Tool "cargo"
$Rustup = Resolve-Tool "rustup"
$Rustc = Resolve-Tool "rustc"

$RootSha = ""
$ExpectedSubmoduleSha = ""
$ActualSubmoduleSha = ""
if ($null -ne $Git) {
    $SafeRoot = $RepoRoot.Replace("\", "/")
    $RootSha = (& $Git -c "safe.directory=$SafeRoot" -C $RepoRoot rev-parse HEAD 2>$null).Trim()
    $TreeLine = (& $Git -c "safe.directory=$SafeRoot" -C $RepoRoot ls-tree HEAD tshiatun 2>$null)
    if ($LASTEXITCODE -ne 0 -or $TreeLine -notmatch "^160000 commit ([0-9a-f]{40})") {
        Add-Gap "tshiatun gitlink is missing from HEAD"
    }
    else {
        $ExpectedSubmoduleSha = $Matches[1]
    }
    $SubmodulePath = Join-Path $RepoRoot "tshiatun"
    if (-not (Test-Path (Join-Path $SubmodulePath "Cargo.toml"))) {
        Add-Gap "tshiatun submodule is not initialized"
    }
    else {
        $SafeSubmodule = $SubmodulePath.Replace("\", "/")
        $ActualSubmoduleSha = (& $Git -c "safe.directory=$SafeSubmodule" -C $SubmodulePath rev-parse HEAD 2>$null).Trim()
        if ($ExpectedSubmoduleSha -and $ActualSubmoduleSha -ne $ExpectedSubmoduleSha) {
            Add-Gap "tshiatun HEAD does not match the superproject gitlink"
        }
        $SubmoduleStatus = @(& $Git -c "safe.directory=$SafeSubmodule" -C $SubmodulePath status --porcelain)
        if ($SubmoduleStatus.Count -gt 0) {
            Add-Gap "tshiatun worktree has tracked or untracked changes"
        }
    }
}

if ($null -ne $Rustup) {
    $Components = @(& $Rustup component list --toolchain $Toolchain 2>$null)
    if ($LASTEXITCODE -ne 0) {
        Add-Gap "Rust toolchain is unavailable: $Toolchain"
    }
    else {
        if (-not (Test-InstalledLine $Components "rustfmt-")) {
            Add-Gap "rustfmt is not installed for $Toolchain"
        }
        if (-not (Test-InstalledLine $Components "clippy-")) {
            Add-Gap "Clippy is not installed for $Toolchain"
        }
        $Targets = @(& $Rustup target list --installed --toolchain $Toolchain 2>$null)
        if ($Targets -notcontains "wasm32-unknown-unknown") {
            Add-Gap "wasm32-unknown-unknown is not installed for $Toolchain"
        }
    }
}

if ($null -ne $Rustc) {
    $RustInfo = @(& $Rustc "+$Toolchain" -vV 2>$null)
    $HostLine = $RustInfo | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    $RustHostTriple = if ($HostLine) { ($HostLine -split ":", 2)[1].Trim() } else { "" }
    if ($RustHostTriple -like "*-gnu") {
        $Sysroot = (& $Rustc "+$Toolchain" --print sysroot 2>$null).Trim()
        if ($Sysroot) {
            $env:Path = (Join-Path $Sysroot "bin") + ";" + $env:Path
        }
        if ($GnuToolWrapper) {
            if (Test-Path $GnuToolWrapper) {
                $env:Path = (Resolve-Path $GnuToolWrapper).Path + ";" + $env:Path
            }
            else {
                Add-Gap "GNU tool-wrapper path does not exist: $GnuToolWrapper"
            }
        }
        if ($RustLld) {
            if (Test-Path $RustLld) {
                $env:RUST_LLD = (Resolve-Path $RustLld).Path
            }
            else {
                Add-Gap "rust-lld path does not exist: $RustLld"
            }
        }
        else {
            $BundledRustLld = Join-Path $Sysroot "lib\rustlib\$RustHostTriple\bin\rust-lld.exe"
            if (Test-Path $BundledRustLld) {
                $env:RUST_LLD = $BundledRustLld
            }
            else {
                Add-Gap "GNU linker preflight failed: rust-lld.exe not found"
            }
        }
        if ($null -eq (Get-Command "dlltool.exe" -ErrorAction SilentlyContinue)) {
            Add-Gap "GNU linker preflight failed: dlltool.exe not found"
        }
    }
    elseif ($RustHostTriple -like "*-msvc") {
        if ($null -eq (Get-Command "link.exe" -ErrorAction SilentlyContinue) -or -not $env:LIB) {
            Add-Gap "MSVC linker/library environment is not initialized"
        }
    }
    elseif (-not $RustHostTriple) {
        Add-Gap "could not determine Rust host for $Toolchain"
    }
}

if ($ToolGaps.Count -gt 0) {
    Write-Summary 2 $RootSha $ExpectedSubmoduleSha $ActualSubmoduleSha
    exit 2
}

$CargoPrefix = @("+$Toolchain")
$FailedGate = $false
$GateCommands = @(
    @{ Name = "rustfmt"; Args = $CargoPrefix + @("fmt", "-p", "conlang-language", "--", "--check") },
    @{ Name = "clippy"; Args = $CargoPrefix + @("clippy", "-p", "conlang-language", "--all-targets", "--", "-D", "warnings") },
    @{ Name = "m1pp-targeted"; Args = $CargoPrefix + @("test", "-p", "conlang-language", "--test", "m1pp_counterexamples", "--test", "m1pp_system", "--test", "patch_interface", "--test", "codegen", "--test", "library_cxg_english", "--test", "sem_roles_realization", "--test", "slot_feature_bindings") },
    @{ Name = "language-full"; Args = $CargoPrefix + @("test", "-p", "conlang-language") },
    @{ Name = "tshiatun-full"; Args = $CargoPrefix + @("test", "--manifest-path", "tshiatun/Cargo.toml") },
    @{ Name = "tshiatun-core-wasm"; Args = $CargoPrefix + @("build", "--manifest-path", "tshiatun/Cargo.toml", "-p", "tshiatun-core", "--target", "wasm32-unknown-unknown") }
)
foreach ($Gate in $GateCommands) {
    $Result = Invoke-GateCommand $Gate.Name $Cargo $Gate.Args $RepoRoot
    if ($Result -ne 0) {
        $FailedGate = $true
    }
}

$ExitCode = if ($FailedGate) { 1 } else { 0 }
Write-Summary $ExitCode $RootSha $ExpectedSubmoduleSha $ActualSubmoduleSha
exit $ExitCode
