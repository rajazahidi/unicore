#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$IsoPath = "os-builder\out\unicore-os-x86_64.iso",
    [string]$Sha256Path = "",
    [switch]$VerifyOnly,
    [switch]$ListUsb
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

try {
    $ResolvedIso = Join-Path $RepoRoot $IsoPath
    if (-not (Test-Path $IsoPath)) {
        throw "ISO image not found: $IsoPath. Build it first with .\os-builder\bootstrap-via-qemu.ps1."
    }
    $IsoFull = (Resolve-Path $IsoPath).Path
    $ShaFile = if ([string]::IsNullOrWhiteSpace($Sha256Path)) { "$IsoFull.sha256" } else { (Resolve-Path $Sha256Path).Path }

    Write-Host "==> Checking ISO SHA-256 integrity..."
    $ActualHash = (Get-FileHash -Algorithm SHA256 -Path $IsoFull).Hash.ToLowerInvariant()
    if (Test-Path $ShaFile) {
        $FirstToken = ((Get-Content $ShaFile -TotalCount 1).Trim() -split '\s+')[0].ToLowerInvariant()
        if ($ActualHash -ne $FirstToken) {
            throw "ISO SHA-256 mismatch!`n  Expected: $FirstToken`n  Actual  : $ActualHash"
        }
        Write-Host "    [OK] SHA-256 verified: $ActualHash"
    } else {
        Write-Warning "No .sha256 companion file found at $ShaFile; computed SHA-256: $ActualHash"
    }

    $IsoSizeMb = [math]::Round((Get-Item $IsoFull).Length / 1MB, 1)
    Write-Host "    [OK] ISO size: $IsoSizeMb MB ($IsoFull)"

    if (-not $VerifyOnly -or $ListUsb) {
        Write-Host ""
        Write-Host "==> Removable USB Drives Detected (Internal SATA/NVMe disks excluded):"
        $UsbDisks = @(Get-Disk -ErrorAction SilentlyContinue | Where-Object { $_.BusType -eq 'USB' })
        if ($UsbDisks.Count -eq 0) {
            Write-Host "    (No USB flash drive detected right now)"
        } else {
            foreach ($d in $UsbDisks) {
                $gb = [math]::Round($d.Size / 1GB, 1)
                Write-Host ("    Disk {0}: {1} ({2} GB, PartitionStyle={3}, Serial={4})" -f $d.Number, $d.FriendlyName, $gb, $d.PartitionStyle, $d.SerialNumber)
            }
        }
    }

    Write-Host ""
    Write-Host "==> Safe USB Flashing Guide (Hybrid BIOS + UEFI ISO):"
    Write-Host "  Option 1 (Rufus on Windows - Recommended):"
    Write-Host "    1. Select your USB drive above (double-check Disk Number & GB size)."
    Write-Host "    2. Select Boot Image: $IsoFull"
    Write-Host "    3. Click START -> choose 'Write in DD Image mode' (recommended for Hybrid ISO/USB) or 'ISO Image mode'."
    Write-Host ""
    Write-Host "  Option 2 (Ventoy Multiboot USB):"
    Write-Host "    Copy unicore-os-x86_64.iso directly onto your Ventoy USB drive."
    Write-Host ""
    Write-Host "  Option 3 (Linux / macOS terminal):"
    Write-Host "    sudo dd if=unicore-os-x86_64.iso of=/dev/sdX bs=4M status=progress oflag=sync"
    Write-Host ""
    Write-Host "  Optional Persistence Setup (inside UniCore OS after booting from USB):"
    Write-Host "    Open App Center -> System Tab -> Disk Installer (with optional LUKS2 encryption), OR:"
    Write-Host "    sudo unicore-setup-data --list"
} finally {
    Pop-Location
}
