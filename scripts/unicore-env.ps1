#Requires -Version 5.1
<#
.SYNOPSIS
    Unified UniCore OS developer, build, QA, OTA, signing, and Ventoy deployment environment controller.
.DESCRIPTION
    Actions:
      doctor        Audit Windows host prerequisites, disk space, Git hooks/LF, QEMU/UEFI, keys, ISO & Ventoy USB.
      check         Run full local CI suite (LF check, clippy -D warnings, cargo test, musl cross-check, py_compile).
      build         Clean full ISO build (auto-includes Ed25519 keys if present in os-builder\keys).
      build-fast    Incremental ISO rebuild reusing existing base SquashFS.
      qa            Run full 5-gate release matrix + 3-stage A/B LUKS2 E2E test suite.
      deploy-ventoy Safely update unicore-os-x86_64.iso on connected Ventoy USB with SHA-256 verification.
      ota-serve     Start local LAN + QEMU HTTP OTA update server on port 8090.
      import-diag   Import unicore-diag-*.txt reports from connected USB drives into docs\hardware-reports\.
      clean         Reclaim disk space by removing disposable builder intermediates (preserves ISO & base SquashFS).
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet("doctor", "check", "build", "build-fast", "qa", "deploy-ventoy", "ota-serve", "import-diag", "clean")]
    [string]$Action = "doctor",
    [int]$OtaPort = 8090,
    [ValidateSet("slim", "full")]
    [string]$FirmwareProfile = "slim"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

function Resolve-Tool([string]$Name, [string[]]$Fallbacks = @()) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($p in $Fallbacks) {
        if (Test-Path $p) { return $p }
    }
    return $null
}

function Run-LfCheck {
    @'
import pathlib, sys
bad = []
for p in pathlib.Path("os-builder").rglob("*"):
    if not p.is_file():
        continue
    if p.suffix.lower() in (".png", ".jpg", ".jpeg", ".webp", ".ico", ".exe", ".apk", ".ps1", ".rs", ".toml", ".py") or "fixtures/target" in p.as_posix() or "os-builder/out" in p.as_posix():
        continue
    if b"\r" in p.read_bytes():
        bad.append(str(p))
if bad:
    print("CRLF detected in Linux scripts/configs:", bad, file=sys.stderr)
    sys.exit(1)
print("[OK] All os-builder Linux scripts/configs are clean LF.")
'@ | python -
    if ($LASTEXITCODE -ne 0) { throw "LF check failed." }
}

function Find-VentoyDrive {
    $vols = Get-Volume | Where-Object { $_.DriveLetter -and $_.DriveType -eq "Removable" }
    foreach ($v in $vols) {
        if ($v.FileSystemLabel -match "Ventoy") { return $v }
    }
    foreach ($v in $vols) {
        if (Test-Path "$($v.DriveLetter):\ventoy") { return $v }
    }
    return $null
}

try {
    switch ($Action) {
        "doctor" {
            Write-Host "============================================================"
            Write-Host "  UniCore OS Perfection Environment Doctor"
            Write-Host "============================================================"
            $checks = @(
                @{ Label = "Git"; Path = (Resolve-Tool "git") },
                @{ Label = "Rust cargo"; Path = (Resolve-Tool "cargo") },
                @{ Label = "Python 3"; Path = (Resolve-Tool "python") },
                @{ Label = "QEMU x86_64"; Path = (Resolve-Tool "qemu-system-x86_64" @("C:\Program Files\qemu\qemu-system-x86_64.exe")) },
                @{ Label = "UEFI OVMF Firmware"; Path = (Resolve-Tool "__none__" @("C:\Program Files\qemu\share\edk2-x86_64-code.fd")) },
                @{ Label = "OpenSSL (Optional Host Sign)"; Path = (Resolve-Tool "openssl" @("C:\Program Files\Git\usr\bin\openssl.exe", "$env:LOCALAPPDATA\Programs\Git\usr\bin\openssl.exe", "C:\Program Files\OpenSSL-Win64\bin\openssl.exe")) },
                @{ Label = "GitHub CLI (gh)"; Path = (Resolve-Tool "gh" @("C:\Program Files\GitHub CLI\gh.exe")) }
            )
            foreach ($c in $checks) {
                $status = if ($c.Path) { "[OK]    $($c.Path)" } else { "[WARN]  Not found" }
                Write-Host ("  {0,-28} {1}" -f $c.Label, $status)
            }

            $hooksPath = (git config --get core.hooksPath 2>$null)
            $attrOk = Test-Path (Join-Path $RepoRoot ".gitattributes")
            Write-Host ("  {0,-28} {1}" -f ".gitattributes (LF guard)", $(if ($attrOk) { "[OK]    Present" } else { "[WARN]  Missing" }))
            Write-Host ("  {0,-28} {1}" -f "Git pre-commit hooks", $(if ($hooksPath -eq ".githooks") { "[OK]    Active (.githooks)" } else { "[WARN]  Not active (run: git config core.hooksPath .githooks)" }))

            $volC = Get-Volume -DriveLetter C
            $freeGB = [math]::Round($volC.SizeRemaining / 1GB, 2)
            $diskTag = if ($freeGB -lt 10) { "[WARN]  $freeGB GB free (<10 GB; run 'unicore-env.ps1 clean' before full build)" } else { "[OK]    $freeGB GB free" }
            Write-Host ("  {0,-28} {1}" -f "Disk C: Free Space", $diskTag)

            $isoPath = Join-Path $RepoRoot "os-builder\out\unicore-os-x86_64.iso"
            if (Test-Path $isoPath) {
                $it = Get-Item $isoPath
                Write-Host ("  {0,-28} [OK]    {1:N1} MB ({2})" -f "Built Hybrid ISO", ($it.Length / 1MB), $it.LastWriteTime)
            } else {
                Write-Host ("  {0,-28} [INFO]  Not built yet" -f "Built Hybrid ISO")
            }

            $privKey = Join-Path $RepoRoot "os-builder\keys\release.key"
            $pubKey = Join-Path $RepoRoot "os-builder\keys\update.pub"
            $keyStatus = if ((Test-Path $privKey) -and (Test-Path $pubKey)) { "[OK]    Ed25519 keypair ready" } else { "[INFO]  No local keys yet (optional for local dev; use GitHub Secrets for prod OTA)" }
            Write-Host ("  {0,-28} {1}" -f "Ed25519 Release Keys", $keyStatus)

            $ventoy = Find-VentoyDrive
            if ($ventoy) {
                $vFree = [math]::Round($ventoy.SizeRemaining / 1GB, 2)
                Write-Host ("  {0,-28} [OK]    Drive $($ventoy.DriveLetter): ($($ventoy.FileSystem), $vFree GB free)" -f "Ventoy USB Drive")
            } else {
                Write-Host ("  {0,-28} [INFO]  Not connected" -f "Ventoy USB Drive")
            }
            Write-Host "============================================================"
        }

        "check" {
            Write-Host "==> [1/5] Checking LF line endings in os-builder/..."
            Run-LfCheck
            Write-Host "==> [2/5] Running Rust clippy (--all-targets -D warnings)..."
            & "$PSScriptRoot\cargo-dev.ps1" clippy --workspace --all-targets '--' -D warnings
            if ($LASTEXITCODE -ne 0) { throw "clippy failed" }
            Write-Host "==> [3/5] Running Rust workspace test suite..."
            & "$PSScriptRoot\cargo-dev.ps1" test --workspace
            if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
            Write-Host "==> [4/5] Checking Linux musl cross-compile target..."
            & "$PSScriptRoot\cargo-dev.ps1" check -p uniload --target x86_64-unknown-linux-musl
            if ($LASTEXITCODE -ne 0) { throw "musl check failed" }
            Write-Host "==> [5/5] Compiling Python QA & OTA scripts..."
            python -m py_compile scripts\test-release-matrix.py scripts\test-ab-install-luks.py scripts\ota-local-server.py
            if ($LASTEXITCODE -ne 0) { throw "py_compile failed" }
            Write-Host "[PASS] All local CI checks passed!"
        }

        "build" {
            Run-LfCheck
            $argsList = @("-FirmwareProfile", $FirmwareProfile)
            if ((Test-Path "os-builder\keys\update.pub") -and (Test-Path "os-builder\keys\release.key")) {
                $argsList += @("-UpdatePubKey", "os-builder\keys\update.pub", "-SignKey", "os-builder\keys\release.key")
            }
            & "$RepoRoot\os-builder\bootstrap-via-qemu.ps1" @argsList
        }

        "build-fast" {
            Run-LfCheck
            $argsList = @("-Incremental", "-FirmwareProfile", $FirmwareProfile)
            if ((Test-Path "os-builder\keys\update.pub") -and (Test-Path "os-builder\keys\release.key")) {
                $argsList += @("-UpdatePubKey", "os-builder\keys\update.pub", "-SignKey", "os-builder\keys\release.key")
            }
            & "$RepoRoot\os-builder\bootstrap-via-qemu.ps1" @argsList
        }

        "qa" {
            python scripts\test-release-matrix.py
            if ($LASTEXITCODE -ne 0) { throw "Release QA matrix failed" }
            python scripts\test-ab-install-luks.py
            if ($LASTEXITCODE -ne 0) { throw "A/B LUKS2 E2E test failed" }
        }

        "deploy-ventoy" {
            $isoSrc = Join-Path $RepoRoot "os-builder\out\unicore-os-x86_64.iso"
            if (-not (Test-Path $isoSrc)) { throw "Built ISO not found: $isoSrc" }
            $ventoy = Find-VentoyDrive
            if (-not $ventoy) { throw "No removable Ventoy USB drive detected. Plug in your Ventoy USB and retry." }
            $destDir = if (Test-Path "$($ventoy.DriveLetter):\ventoy") { "$($ventoy.DriveLetter):\ventoy" } else { "$($ventoy.DriveLetter):\" }
            $isoDst = Join-Path $destDir "unicore-os-x86_64.iso"
            if (Test-Path $isoDst) {
                Write-Host "==> Removing previous $isoDst..."
                Remove-Item -Force $isoDst
            }
            Write-Host "==> Copying $isoSrc -> $isoDst ..."
            Copy-Item -Path $isoSrc -Destination $isoDst -Force
            Write-Host "==> Verifying SHA-256 on USB..."
            $h1 = (Get-FileHash -Algorithm SHA256 -Path $isoSrc).Hash.ToLower()
            $h2 = (Get-FileHash -Algorithm SHA256 -Path $isoDst).Hash.ToLower()
            if ($h1 -ne $h2) { throw "SHA-256 mismatch after copy ($h1 != $h2)!" }
            Write-Host "[OK] Deployed & SHA-256 verified: $isoDst ($h2)"
            Write-Host "     Remember to Safely Eject drive $($ventoy.DriveLetter): before unplugging."
        }

        "ota-serve" {
            python scripts\ota-local-server.py --port $OtaPort
        }

        "import-diag" {
            $destRoot = Join-Path $RepoRoot "docs\hardware-reports"
            New-Item -ItemType Directory -Force -Path $destRoot | Out-Null
            $vols = Get-Volume | Where-Object { $_.DriveLetter -and $_.DriveType -eq "Removable" }
            $found = 0
            foreach ($v in $vols) {
                $reports = Get-ChildItem "$($v.DriveLetter):\" -Filter "unicore-diag-*.txt" -Recurse -Depth 2 -ErrorAction SilentlyContinue
                foreach ($r in $reports) {
                    $dst = Join-Path $destRoot $r.Name
                    Copy-Item -Path $r.FullName -Destination $dst -Force
                    Write-Host "[IMPORTED] $($r.FullName) -> $dst"
                    $found++
                }
            }
            if ($found -eq 0) {
                Write-Host "[INFO] No unicore-diag-*.txt reports found on removable drives."
                Write-Host "       Run 'unicore-diag --save-usb' inside UniCore OS on your target PC first."
            }
        }

        "clean" {
            $targets = @(
                "os-builder\out\builder-work.qcow2",
                "os-builder\out\stage.tar",
                "os-builder\out\artifacts.tar.raw",
                "os-builder\out\stage",
                "scripts\__pycache__"
            )
            foreach ($rel in $targets) {
                $full = Join-Path $RepoRoot $rel
                if (Test-Path $full) {
                    Remove-Item -Recurse -Force $full
                    Write-Host "[CLEANED] $rel"
                }
            }
            $volC = Get-Volume -DriveLetter C
            Write-Host ("[OK] Cleanup complete. Drive C: free space = {0:N2} GB" -f ($volC.SizeRemaining / 1GB))
        }
    }
} finally {
    Pop-Location
}
