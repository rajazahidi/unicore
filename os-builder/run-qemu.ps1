<#
.SYNOPSIS
    Boot UniCore OS inside QEMU on Windows 11 across 2005-present hardware profiles.
.DESCRIPTION
    Supports -HwProfile legacy2005 (512 MB RAM, 1 Core, no SSE4.2/AVX, legacy VGA),
    legacy2010 (2 GB RAM, 2 Cores, Nehalem), modern (4 GB RAM, 4 Cores), or auto.
    Supports -BootIso to test the Hybrid BIOS+UEFI bootable ISO image.
#>
[CmdletBinding()]
param(
    [string]$Arch = "x86_64",
    [ValidateSet("auto", "legacy2005", "legacy2010", "modern")]
    [string]$HwProfile = "auto",
    [int]$MemoryMB = 2048,
    [int]$Cpus = 2,
    [switch]$BootIso,
    [switch]$Headless,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$OutDir = Join-Path $ScriptDir "out"
$SquashImg = Join-Path $OutDir "unicore-rootfs-$Arch.squashfs"
$KernelImg = Join-Path $OutDir "vmlinuz-$Arch"
$InitrdImg = Join-Path $OutDir "initramfs-$Arch.img"
$DataQcow = Join-Path $OutDir "data.qcow2"
$IsoImg = Join-Path $OutDir "unicore-os-$Arch.iso"

$QemuBin = "qemu-system-$Arch"
$QemuCmd = Get-Command $QemuBin -ErrorAction SilentlyContinue
if (-not $QemuCmd) {
    $defaultQemuDir = "C:\Program Files\qemu"
    if (Test-Path $defaultQemuDir) {
        $env:PATH = "$defaultQemuDir;$env:PATH"
        $QemuCmd = Get-Command $QemuBin -ErrorAction SilentlyContinue
    }
}

$HypervisorPresent = (Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction SilentlyContinue).HypervisorPresent
$Accel = if (($Arch -eq "x86_64") -and $HypervisorPresent) { "whpx,kernel-irqchip=off" } else { "tcg" }

$CpuModel = $null
$MachineArgs = @()
$ConsoleDev = if ($Arch -eq "aarch64") { "ttyAMA0" } else { "ttyS0" }
$VgaDevice = if ($Arch -eq "aarch64") { @("-device", "virtio-gpu-pci") } else { @("-device", "virtio-vga") }
if ($Arch -eq "aarch64") {
    $MachineArgs = @("-M", "virt")
    $CpuModel = "cortex-a57"
}
switch ($HwProfile) {
    "legacy2005" {
        $MemoryMB = 512
        $Cpus = 1
        if ($Arch -ne "aarch64") {
            $CpuModel = "qemu64,-sse4.2,-avx,-avx2"
            $VgaDevice = @("-vga", "std")
        }
    }
    "legacy2010" {
        $MemoryMB = 2048
        $Cpus = 2
        if ($Arch -ne "aarch64") {
            $CpuModel = "Nehalem"
        }
    }
    "modern" {
        $MemoryMB = 4096
        $Cpus = 4
    }
}

$QemuArgs = @() + $MachineArgs + @(
    "-m", "$MemoryMB",
    "-smp", "$Cpus",
    "-accel", $Accel
)
if ($CpuModel) {
    $QemuArgs += @("-cpu", $CpuModel)
}
$QemuArgs += @(
    "-netdev", "user,id=net0",
    "-device", "virtio-net-pci,netdev=net0"
)

if ($Headless) {
    if ($HwProfile -eq "legacy2005") {
        $QemuArgs += @("-vga", "std", "-display", "none", "-serial", "stdio")
    } else {
        $QemuArgs += @("-nographic")
    }
} else {
    $QemuArgs += $VgaDevice + @("-display", "sdl", "-serial", "mon:stdio")
}

if ($BootIso) {
    if (-not (Test-Path $IsoImg)) {
        Write-Warning "Hybrid ISO not found at '$IsoImg'. Build via .\os-builder\bootstrap-via-qemu.ps1"
        if (-not $DryRun) { exit 1 }
    }
    if ($Arch -eq "aarch64") {
        $Aarch64Bios = "C:\Program Files\qemu\share\edk2-aarch64-code.fd"
        if (Test-Path $Aarch64Bios) {
            $QemuArgs += @("-bios", $Aarch64Bios)
        }
    }
    $QemuArgs += @("-cdrom", $IsoImg, "-boot", "d")
    if (Test-Path $DataQcow) {
        $QemuArgs += @("-drive", "file=$DataQcow,if=virtio,format=qcow2")
    }
} elseif ((Test-Path $KernelImg) -and (Test-Path $InitrdImg) -and (Test-Path $SquashImg)) {
    $append = "console=tty1 console=$ConsoleDev quiet"
    $QemuArgs += @(
        "-kernel", $KernelImg,
        "-initrd", $InitrdImg,
        "-drive", "file=$SquashImg,if=virtio,format=raw,readonly=on",
        "-append", $append
    )
    if (Test-Path $DataQcow) {
        $QemuArgs += @("-drive", "file=$DataQcow,if=virtio,format=qcow2")
    }
} elseif (Test-Path $IsoImg) {
    $QemuArgs += @("-cdrom", $IsoImg, "-boot", "d")
} else {
    Write-Warning "No bootable UniCore OS image found in '$OutDir'."
    Write-Host "Build the image first via:"
    Write-Host "  .\os-builder\bootstrap-via-qemu.ps1"
    if (-not $DryRun) {
        exit 1
    }
}

if (-not $QemuCmd) {
    Write-Warning "'$QemuBin' was not found on PATH."
    Write-Host "Install QEMU on Windows via winget:"
    Write-Host "  winget install SoftwareFreedomConservancy.QEMU"
    Write-Host ""
    Write-Host "Planned QEMU command:"
    Write-Host "  $QemuBin $($QemuArgs -join ' ')"
    exit $(if ($DryRun) { 0 } else { 1 })
}

if ($DryRun) {
    Write-Host "[DryRun] $($QemuCmd.Source) $($QemuArgs -join ' ')"
    exit 0
}

& $QemuCmd.Source @QemuArgs
