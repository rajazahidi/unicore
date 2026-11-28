<#
.SYNOPSIS
    Runs Cargo on Windows with auto-detected Rust, MSVC linker, and Windows SDK (ucrt + um) paths.
.EXAMPLE
    .\scripts\cargo-dev.ps1 test
    .\scripts\cargo-dev.ps1 run -- inspect C:\Windows\System32\notepad.exe
#>
$ErrorActionPreference = "Stop"
$CargoArgs = $args

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = "$cargoBin;$env:PATH"
}

if (-not (Get-Command "link.exe" -ErrorAction SilentlyContinue)) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
        if ($vsPath) {
            $msvcRoot = Join-Path $vsPath "VC\Tools\MSVC"
            $msvcVer = Get-ChildItem $msvcRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
            if ($msvcVer) {
                $vcBin = Join-Path $msvcVer.FullName "bin\Hostx64\x64"
                $vcLib = Join-Path $msvcVer.FullName "lib\x64"
                $vcInc = Join-Path $msvcVer.FullName "include"
                if (Test-Path $vcBin) { $env:PATH = "$vcBin;$env:PATH" }
                if (Test-Path $vcLib) { $env:LIB = "$vcLib;$env:LIB" }
                if ((Test-Path $vcInc) -and ($env:INCLUDE -notlike "*$vcInc*")) {
                    $env:INCLUDE = if ($env:INCLUDE) { "$vcInc;$env:INCLUDE" } else { $vcInc }
                }
            }
        }
    }
}

$sdkCandidates = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\Lib",
    "C:\winsdk\Windows Kits\10\Lib"
)
foreach ($sdkRoot in $sdkCandidates) {
    if (Test-Path $sdkRoot) {
        $latestVer = Get-ChildItem $sdkRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
        if ($latestVer) {
            foreach ($sub in @("ucrt\x64", "um\x64")) {
                $libDir = Join-Path $latestVer.FullName $sub
                if ((Test-Path $libDir) -and ($env:LIB -notlike "*$libDir*")) {
                    $env:LIB = if ($env:LIB) { "$env:LIB;$libDir" } else { $libDir }
                }
            }
        }
    }
}

$sdkIncCandidates = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\Include",
    "C:\winsdk\Windows Kits\10\Include"
)
foreach ($sdkIncRoot in $sdkIncCandidates) {
    if (Test-Path $sdkIncRoot) {
        $latestVer = Get-ChildItem $sdkIncRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
        if ($latestVer) {
            foreach ($sub in @("ucrt", "um", "shared")) {
                $incDir = Join-Path $latestVer.FullName $sub
                if ((Test-Path $incDir) -and ($env:INCLUDE -notlike "*$incDir*")) {
                    $env:INCLUDE = if ($env:INCLUDE) { "$env:INCLUDE;$incDir" } else { $incDir }
                }
            }
        }
    }
}

if (-not $CargoArgs -or $CargoArgs.Count -eq 0) {
    $CargoArgs = @("--help")
}

$argJoined = $CargoArgs -join " "
if ($argJoined -match "(x86_64|aarch64)-unknown-linux-musl") {
    $sysroot = (& rustc --print sysroot).Trim()
    $rustLld = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"
    if (Test-Path $rustLld) {
        if ($argJoined -match "x86_64-unknown-linux-musl") {
            $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = $rustLld
            $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C link-self-contained=yes"
        }
        if ($argJoined -match "aarch64-unknown-linux-musl") {
            $env:CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = $rustLld
            $env:CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C link-self-contained=yes"
        }
    }
}

& cargo @CargoArgs
exit $LASTEXITCODE
