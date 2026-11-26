<#
.SYNOPSIS
    Builds bootable UniCore OS artifacts on Windows without WSL2 or rebooting.
.DESCRIPTION
    1. Cross-compiles `uniload` to a static x86_64-unknown-linux-musl binary.
    2. Compiles a tiny static Windows PE fixture (`unicore_hello.exe`).
    3. Downloads official Alpine netboot kernel/initramfs (v3.20).
    4. Runs build-rootfs.sh inside an automated headless QEMU Alpine VM and
       extracts vmlinuz-x86_64, initramfs-x86_64.img, and unicore-rootfs-x86_64.squashfs.
    5. Creates persistent data.qcow2 (8G thin-provisioned) if absent.
#>
[CmdletBinding()]
param(
    [string]$Arch = "x86_64",
    [string]$AlpineVersion = "3.20",
    [ValidateSet("slim", "full")]
    [string]$FirmwareProfile = "slim",
    [string]$UpdatePubKey = "",
    [string]$SignKey = "",
    [switch]$Incremental
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$OutDir = Join-Path $ScriptDir "out"
$NetbootDir = Join-Path $OutDir "netboot\$Arch"
if (($Arch -eq "x86_64") -and (Test-Path (Join-Path $OutDir "netboot\vmlinuz-virt")) -and (-not (Test-Path (Join-Path $NetbootDir "vmlinuz-virt")))) {
    $NetbootDir = Join-Path $OutDir "netboot"
}
$StageDir = Join-Path $OutDir "stage"
$CargoDev = Join-Path $RepoRoot "scripts\cargo-dev.ps1"

$QemuSys = Get-Command "qemu-system-$Arch" -ErrorAction SilentlyContinue
$QemuImg = Get-Command "qemu-img" -ErrorAction SilentlyContinue
if (-not $QemuSys -or -not $QemuImg) {
    $defaultQemuDir = "C:\Program Files\qemu"
    if (Test-Path $defaultQemuDir) {
        $env:PATH = "$defaultQemuDir;$env:PATH"
        $QemuSys = Get-Command "qemu-system-$Arch" -ErrorAction SilentlyContinue
        $QemuImg = Get-Command "qemu-img" -ErrorAction SilentlyContinue
    }
}
if (-not $QemuSys -or -not $QemuImg) {
    throw "QEMU (qemu-system-$Arch and qemu-img) is required. Install via: winget install SoftwareFreedomConservancy.QEMU"
}

New-Item -ItemType Directory -Force -Path $OutDir, $NetbootDir | Out-Null
if (Test-Path $StageDir) {
    Remove-Item -Recurse -Force $StageDir
}
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

$RustTarget = "$Arch-unknown-linux-musl"
Write-Host "==> Cross-compiling static Rust uniload ($RustTarget)"
& $CargoDev build --release --target $RustTarget -p uniload
if ($LASTEXITCODE -ne 0) {
    throw "Failed to cross-compile uniload for $RustTarget"
}
$UniloadBin = Join-Path $RepoRoot "target\$RustTarget\release\uniload"
if (-not (Test-Path $UniloadBin)) {
    throw "Expected cross-compiled binary missing: $UniloadBin"
}

$EnsureSysroot = Join-Path $RepoRoot "scripts\ensure-musl-sysroot.ps1"
& $EnsureSysroot -AlpineVersion $AlpineVersion -Arch $Arch
if ($LASTEXITCODE -ne 0) {
    throw "Failed to prepare Alpine musl dynamic sysroot"
}
$MuslSysroot = if ($Arch -eq "x86_64") {
    Join-Path $RepoRoot ".cache\alpine-musl-sysroot"
} else {
    Join-Path $RepoRoot ".cache\alpine-musl-sysroot-$Arch"
}

Write-Host "==> Cross-compiling UniCore App Center GUI ($RustTarget dynamic musl)"
& $CargoDev rustc -p unicore-center --bin unicore-center --release --target $RustTarget '--' -C target-feature=-crt-static -L "native=$MuslSysroot" -C link-arg=--dynamic-linker=/lib/ld-musl-$Arch.so.1
if ($LASTEXITCODE -ne 0) {
    throw "Failed to cross-compile unicore-center for $RustTarget"
}
$CenterBin = Join-Path $RepoRoot "target\$RustTarget\release\unicore-center"
if (-not (Test-Path $CenterBin)) {
    throw "Expected cross-compiled GUI binary missing: $CenterBin"
}

Write-Host "==> Compiling tiny static Win32 test fixture (unicore_hello.exe)"
$FixtureManifest = Join-Path $ScriptDir "fixtures\Cargo.toml"
$prevRustFlags = $env:RUSTFLAGS
$env:RUSTFLAGS = "-C target-feature=+crt-static"
try {
    & $CargoDev build --release --manifest-path $FixtureManifest
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to compile unicore_hello.exe test fixture"
    }
} finally {
    $env:RUSTFLAGS = $prevRustFlags
}
$HelloBuilt = Join-Path $ScriptDir "fixtures\target\release\unicore_hello.exe"
$HelloExe = Join-Path $StageDir "unicore_hello.exe"
Copy-Item $HelloBuilt $HelloExe

Write-Host "==> Building Android APK test fixture (unicore_hello.apk)"
$ApkBuilder = Join-Path $ScriptDir "fixtures\build_hello_apk.py"
$HelloApk = Join-Path $StageDir "unicore_hello.apk"
& python $ApkBuilder $HelloApk
if ($LASTEXITCODE -ne 0) {
    throw "Failed to generate unicore_hello.apk test fixture"
}

Write-Host "==> Preparing Alpine netboot builder assets (v$AlpineVersion)"
$BaseUrlHttps = "https://dl-cdn.alpinelinux.org/alpine/v$AlpineVersion/releases/$Arch/netboot"
$BaseUrlHttp = "http://dl-cdn.alpinelinux.org/alpine/v$AlpineVersion/releases/$Arch/netboot"
foreach ($asset in @("vmlinuz-virt", "initramfs-virt")) {
    $dest = Join-Path $NetbootDir $asset
    if (-not (Test-Path $dest)) {
        Write-Host "    Downloading $asset..."
        Invoke-WebRequest -Uri "$BaseUrlHttps/$asset" -OutFile $dest
    }
}

Write-Host "==> Staging UniCore OS builder files"
Copy-Item $UniloadBin (Join-Path $StageDir "uniload")
Copy-Item $CenterBin (Join-Path $StageDir "unicore-center")
Copy-Item -Recurse (Join-Path $ScriptDir "overlay") (Join-Path $StageDir "overlay")
Copy-Item (Join-Path $ScriptDir "binfmt-universal.conf") (Join-Path $StageDir "binfmt-universal.conf")
Copy-Item (Join-Path $ScriptDir "kernel.config.fragment") (Join-Path $StageDir "kernel.config.fragment")
Copy-Item (Join-Path $ScriptDir "initramfs-init") (Join-Path $StageDir "initramfs-init")
Copy-Item (Join-Path $ScriptDir "build-rootfs.sh") (Join-Path $StageDir "build-rootfs.sh")
if ($UpdatePubKey -and (Test-Path $UpdatePubKey)) {
    Copy-Item $UpdatePubKey (Join-Path $StageDir "update.pub")
}
if ($SignKey -and (Test-Path $SignKey)) {
    Copy-Item $SignKey (Join-Path $StageDir "release.key")
}

if (($Arch -eq "x86_64") -or ($Arch -eq "x86")) {
    $Win32Cache = Join-Path $RepoRoot ".cache\win32-assets"
    New-Item -ItemType Directory -Force -Path $Win32Cache | Out-Null
    $DxvkUrl = "https://github.com/doitsujin/dxvk/releases/download/v2.4.1/dxvk-2.4.1.tar.gz"
    $Vkd3dUrl = "https://github.com/HansKristian-Work/vkd3d-proton/releases/download/v2.13/vkd3d-proton-2.13.tar.zst"
    $CncDdrawUrl = "https://github.com/FunkyFr3sh/cnc-ddraw/releases/download/v7.1.0.0/cnc-ddraw.zip"
    $OtvdmUrl = "https://github.com/otya128/winevdm/releases/download/v0.9.0/otvdm-v0.9.0.zip"
    $WinetricksUrl = "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks"
    $DxvkCache = Join-Path $Win32Cache "dxvk-2.4.1.tar.gz"
    $Vkd3dCache = Join-Path $Win32Cache "vkd3d-proton-2.13.tar.zst"
    $CncDdrawCache = Join-Path $Win32Cache "cnc-ddraw-v7.1.0.0.zip"
    $OtvdmCache = Join-Path $Win32Cache "otvdm-v0.9.0.zip"
    $WinetricksCache = Join-Path $Win32Cache "winetricks"
    if (-not (Test-Path $DxvkCache)) {
        Write-Host "==> Downloading DXVK v2.4.1 for offline DirectX 8-11..."
        Invoke-WebRequest -Uri $DxvkUrl -OutFile $DxvkCache
    }
    if (-not (Test-Path $Vkd3dCache)) {
        Write-Host "==> Downloading VKD3D-Proton v2.13 for offline DirectX 12..."
        Invoke-WebRequest -Uri $Vkd3dUrl -OutFile $Vkd3dCache
    }
    if (-not (Test-Path $CncDdrawCache)) {
        Write-Host "==> Downloading cnc-ddraw v7.1.0.0 for Win98/XP DirectDraw games..."
        Invoke-WebRequest -Uri $CncDdrawUrl -OutFile $CncDdrawCache
    }
    if (-not (Test-Path $OtvdmCache)) {
        Write-Host "==> Downloading OTVDM v0.9.0 for offline Win16 NE games..."
        Invoke-WebRequest -Uri $OtvdmUrl -OutFile $OtvdmCache
    }
    if (-not (Test-Path $WinetricksCache)) {
        Write-Host "==> Downloading winetricks for DX9/VC++ classic game runtimes..."
        Invoke-WebRequest -Uri $WinetricksUrl -OutFile $WinetricksCache
    }
    Copy-Item $DxvkCache (Join-Path $StageDir "dxvk-2.4.1.tar.gz")
    Copy-Item $Vkd3dCache (Join-Path $StageDir "vkd3d-proton-2.13.tar.zst")
    Copy-Item $CncDdrawCache (Join-Path $StageDir "cnc-ddraw-v7.1.0.0.zip")
    Copy-Item $OtvdmCache (Join-Path $StageDir "otvdm-v0.9.0.zip")
    Copy-Item $WinetricksCache (Join-Path $StageDir "winetricks")
}

$runBuildScript = (@'
#!/bin/sh
set -eu
on_err() {
    echo "=== UNICORE_BUILD_FAILED ==="
    poweroff -f
}
trap on_err EXIT

echo "=== UniCore QEMU Builder Starting ==="
cat <<REPOS > /etc/apk/repositories
http://dl-cdn.alpinelinux.org/alpine/v3.20/main
http://dl-cdn.alpinelinux.org/alpine/v3.20/community
REPOS

apk update
GRUB_ARCH_PKGS="grub grub-efi"
if [ "@@TARGET_ARCH@@" = "x86_64" ] || [ "@@TARGET_ARCH@@" = "x86" ]; then
    GRUB_ARCH_PKGS="grub grub-bios grub-efi"
fi
apk add --no-cache bash squashfs-tools tar zstd unzip gzip findutils kmod e2fsprogs util-linux ${GRUB_ARCH_PKGS} xorriso mtools dosfstools

mkfs.ext4 -F /dev/vda >/dev/null 2>&1
mkdir -p /work
mount /dev/vda /work

mkdir -p /work/src /work/out /work/build
cp -a /mnt/stage/. /work/src/
chmod +x /work/src/build-rootfs.sh

export TARGET_ARCH=@@TARGET_ARCH@@
export FIRMWARE_PROFILE_OVERRIDE=@@FIRMWARE_PROFILE@@
export OUT_DIR_OVERRIDE=/work/out
export WORK_DIR_OVERRIDE=/work/build
export UNILOAD_BIN_OVERRIDE=/work/src/uniload
export UNICORE_CENTER_BIN_OVERRIDE=/work/src/unicore-center
export HELLO_EXE_OVERRIDE=/work/src/unicore_hello.exe
export HELLO_APK_OVERRIDE=/work/src/unicore_hello.apk
export REPO_ROOT_OVERRIDE=/work/src
if [ -f /work/src/dxvk-2.4.1.tar.gz ]; then
    export DXVK_ARCHIVE_OVERRIDE=/work/src/dxvk-2.4.1.tar.gz
fi
if [ -f /work/src/vkd3d-proton-2.13.tar.zst ]; then
    export VKD3D_ARCHIVE_OVERRIDE=/work/src/vkd3d-proton-2.13.tar.zst
fi
if [ -f /work/src/cnc-ddraw-v7.1.0.0.zip ]; then
    export CNCDDRAW_ARCHIVE_OVERRIDE=/work/src/cnc-ddraw-v7.1.0.0.zip
fi
if [ -f /work/src/otvdm-v0.9.0.zip ]; then
    export OTVDM_ARCHIVE_OVERRIDE=/work/src/otvdm-v0.9.0.zip
fi
if [ -f /work/src/winetricks ]; then
    export WINETRICKS_SCRIPT_OVERRIDE=/work/src/winetricks
fi
if [ -f /work/src/update.pub ]; then
    export UPDATE_PUBKEY_OVERRIDE=/work/src/update.pub
fi
if [ -f /work/src/release.key ]; then
    export UPDATE_SIGN_KEY_OVERRIDE=/work/src/release.key
fi
if [ -b /dev/vdd ]; then
    export BASE_SQUASHFS_OVERRIDE=/dev/vdd
fi

bash /work/src/build-rootfs.sh

echo "=== Exporting built artifacts to /dev/vdc ==="
EXTRA_SIG=""
[ -f /work/out/unicore-manifest.json.sig ] && EXTRA_SIG="unicore-manifest.json.sig"
EXTRA_SHA=""
[ -f /work/out/unicore-os-@@TARGET_ARCH@@.iso.sha256 ] && EXTRA_SHA="unicore-os-@@TARGET_ARCH@@.iso.sha256"
tar -cf /dev/vdc -C /work/out vmlinuz-@@TARGET_ARCH@@ initramfs-@@TARGET_ARCH@@.img unicore-rootfs-@@TARGET_ARCH@@.squashfs unicore-os-@@TARGET_ARCH@@.iso unicore-manifest.json ${EXTRA_SIG} ${EXTRA_SHA}
sync
trap - EXIT
echo "=== UNICORE_BUILD_SUCCESS ==="
poweroff -f
'@).Replace('@@TARGET_ARCH@@', $Arch).Replace('@@FIRMWARE_PROFILE@@', $FirmwareProfile) -replace "`r`n", "`n"
[System.IO.File]::WriteAllText((Join-Path $StageDir "run-build.sh"), $runBuildScript, [System.Text.UTF8Encoding]::new($false))

$WorkQcow = Join-Path $OutDir "builder-work.qcow2"
$StageTar = Join-Path $OutDir "stage.tar"
$OutTarRaw = Join-Path $OutDir "artifacts.tar.raw"
$DataQcow = Join-Path $OutDir "data.qcow2"
$ExistingIso = Join-Path $OutDir "unicore-os-$Arch.iso"

if (Test-Path $WorkQcow) { Remove-Item -Force $WorkQcow }
if (Test-Path $StageTar) { Remove-Item -Force $StageTar }
if (Test-Path $OutTarRaw) { Remove-Item -Force $OutTarRaw }
if (Test-Path $ExistingIso) { Remove-Item -Force $ExistingIso }

& python -c @"
import os, tarfile
stage = r'$StageDir'
out_tar = r'$StageTar'
with tarfile.open(out_tar, 'w') as tf:
    for root, _, files in os.walk(stage):
        for fn in sorted(files):
            full = os.path.join(root, fn)
            rel = os.path.relpath(full, stage).replace('\\\\', '/')
            ti = tf.gettarinfo(full, arcname=rel)
            if rel in ('uniload', 'unicore-center', 'build-rootfs.sh', 'run-build.sh', 'initramfs-init') or '/usr/bin/' in rel or '/usr/sbin/' in rel or '/libexec/' in rel or '/local.d/' in rel or rel.endswith('autostart'):
                ti.mode = 0o755
            else:
                ti.mode = 0o644
            with open(full, 'rb') as f:
                tf.addfile(ti, f)
"@
if ($LASTEXITCODE -ne 0) {
    throw "Failed to pack stage.tar"
}

& $QemuImg.Source create -f qcow2 $WorkQcow 16G | Out-Null
# Pre-allocate 6 GiB sparse raw file for output tar archive (SquashFS + Hybrid ISO with full firmware)
$fs = [System.IO.File]::Create($OutTarRaw)
$fs.Close()
& fsutil sparse setflag $OutTarRaw | Out-Null
if ($LASTEXITCODE -ne 0) { throw "fsutil sparse setflag failed" }
& fsutil file seteof $OutTarRaw 6442450944 | Out-Null
if ($LASTEXITCODE -ne 0) { throw "fsutil file seteof failed" }

$ExistingSquashfs = Join-Path $OutDir "unicore-rootfs-$Arch.squashfs"
$UseIncremental = $Incremental -and (Test-Path $ExistingSquashfs)
if ($Incremental -and -not $UseIncremental) {
    Write-Host "==> -Incremental requested but no existing $ExistingSquashfs found; falling back to full build."
}

if (-not (Test-Path $DataQcow)) {
    Write-Host "==> Provisioning persistent /data disk ($DataQcow)"
    & $QemuImg.Source create -f qcow2 $DataQcow 8G | Out-Null
}

Write-Host "==> Booting headless QEMU Alpine builder (installs packages & builds SquashFS/initramfs)..."
$driverPy = @'
import subprocess, sys, time

def send_serial(proc, text_bytes):
    # Chunk writes below 16-byte 16550A UART FIFO size to prevent dropped characters.
    for i in range(0, len(text_bytes), 8):
        proc.stdin.write(text_bytes[i:i + 8])
        proc.stdin.flush()
        time.sleep(0.02)

qemu_cmd = sys.argv[1:]
proc = subprocess.Popen(
    qemu_cmd,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    bufsize=0,
)

buf = b""
logged_in = False
started_build = False
success = False

try:
    while True:
        chunk = proc.stdout.read(1)
        if not chunk:
            break
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()
        buf += chunk
        if len(buf) > 8192:
            buf = buf[-4096:]

        if not logged_in and b"localhost login:" in buf:
            logged_in = True
            send_serial(proc, b"root\n")
            buf = b""
        elif logged_in and not started_build and b"localhost:~#" in buf:
            started_build = True
            cmd = b"(mkdir -p /mnt/stage && cd /mnt/stage && tar -xf /dev/vdb && sh run-build.sh) || poweroff -f\n"
            send_serial(proc, cmd)
            buf = b""
        if b"UNICORE_BUILD_SUCCESS" in buf:
            success = True
finally:
    rc = proc.wait(timeout=120)

if not success or rc != 0:
    sys.exit(1)
'@

$BuilderConsole = if ($Arch -eq "aarch64") { "ttyAMA0" } else { "ttyS0" }
$qemuArgs = @($QemuSys.Source)
if ($Arch -eq "aarch64") {
    $qemuArgs += @("-M", "virt", "-cpu", "cortex-a57")
}
$qemuArgs += @(
    "-m", "2048",
    "-smp", "4",
    "-nographic",
    "-no-reboot",
    "-kernel", (Join-Path $NetbootDir "vmlinuz-virt"),
    "-initrd", (Join-Path $NetbootDir "initramfs-virt"),
    "-append", "console=$BuilderConsole ip=dhcp alpine_repo=http://dl-cdn.alpinelinux.org/alpine/v$AlpineVersion/main modloop=$BaseUrlHttp/modloop-virt",
    "-drive", "file=$WorkQcow,if=virtio,format=qcow2",
    "-drive", "file=$StageTar,format=raw,if=virtio,readonly=on",
    "-drive", "file=$OutTarRaw,if=virtio,format=raw"
)
if ($UseIncremental) {
    Write-Host "==> Incremental build enabled using existing base SquashFS ($ExistingSquashfs)"
    $qemuArgs += @("-drive", "file=$ExistingSquashfs,format=raw,if=virtio,readonly=on")
}
$qemuArgs += @(
    "-netdev", "user,id=net0",
    "-device", "virtio-net-pci,netdev=net0"
)

$driverPath = Join-Path $OutDir "qemu_build_driver.py"
[System.IO.File]::WriteAllText($driverPath, $driverPy, [System.Text.UTF8Encoding]::new($false))

& python $driverPath @qemuArgs
if ($LASTEXITCODE -ne 0) {
    throw "Automated QEMU rootfs build failed."
}

Remove-Item -Force $WorkQcow, $StageTar, $driverPath -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $StageDir -ErrorAction SilentlyContinue
Write-Host "==> Extracting built UniCore OS artifacts to $OutDir"
& python -c "import tarfile, sys; tf = tarfile.open(sys.argv[1], 'r:*'); tf.extractall(sys.argv[2], filter='data'); tf.close()" $OutTarRaw $OutDir
if ($LASTEXITCODE -ne 0) {
    throw "Failed to extract built artifacts from $OutTarRaw"
}

Remove-Item -Force $OutTarRaw -ErrorAction SilentlyContinue
Write-Host "==> UniCore OS build ready in $OutDir!"
Write-Host "    Boot desktop via: .\os-builder\run-qemu.ps1"
