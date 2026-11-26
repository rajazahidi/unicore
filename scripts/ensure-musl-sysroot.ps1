<#
.SYNOPSIS
    Ensures Alpine 3.20 musl + libgcc shared-library sysroot files exist in .cache/alpine-musl-sysroot
    for dynamically linked x86_64-unknown-linux-musl GUI binaries (dlopen support).
#>
[CmdletBinding()]
param(
    [string]$AlpineVersion = "3.20",
    [string]$Arch = "x86_64"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$OutDir = if ($Arch -eq "x86_64") {
    Join-Path $RepoRoot ".cache\alpine-musl-sysroot"
} else {
    Join-Path $RepoRoot ".cache\alpine-musl-sysroot-$Arch"
}

$ldSo = "ld-musl-$Arch.so.1"
$required = @("libc.so", "libgcc_s.so", "libdl.a", $ldSo, "Scrt1.o", "crti.o", "crtn.o")
$allPresent = ($required | ForEach-Object { Test-Path (Join-Path $OutDir $_) }) -notcontains $false
if ($allPresent) {
    Write-Host "==> Alpine musl sysroot ready: $OutDir"
    exit 0
}

Write-Host "==> Provisioning Alpine v$AlpineVersion ($Arch) musl dynamic sysroot ($OutDir)"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$pyScript = @'
import urllib.request, re, tarfile, io, os, sys

out_dir = sys.argv[1]
ver = sys.argv[2]
arch = sys.argv[3]
ld_so = f"ld-musl-{arch}.so.1"
base = f"https://dl-cdn.alpinelinux.org/alpine/v{ver}/main/{arch}/"
html = urllib.request.urlopen(base).read().decode("utf-8", "replace")
links = re.findall(r'href="([^"]+\.apk)"', html)

targets = {
    f"lib/{ld_so}": [ld_so, "libc.so"],
    "usr/lib/Scrt1.o": ["Scrt1.o"],
    "usr/lib/crti.o": ["crti.o"],
    "usr/lib/crtn.o": ["crtn.o"],
    "usr/lib/libdl.a": ["libdl.a"],
    "usr/lib/libgcc_s.so.1": ["libgcc_s.so.1", "libgcc_s.so"],
}

for pkg_prefix in ("musl-1.", "musl-dev-", "libgcc-"):
    apk_name = next((m for m in links if m.startswith(pkg_prefix)), None)
    if not apk_name:
        raise RuntimeError(f"Could not find {pkg_prefix}*.apk at {base}")
    data = urllib.request.urlopen(base + apk_name).read()
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tf:
        for member in tf.getmembers():
            if member.isfile() and member.name in targets:
                raw = tf.extractfile(member).read()
                for dst_name in targets[member.name]:
                    with open(os.path.join(out_dir, dst_name), "wb") as f:
                        f.write(raw)
print("Extracted Alpine musl sysroot successfully.")
'@

$pyScript | & python - $OutDir $AlpineVersion $Arch
if ($LASTEXITCODE -ne 0) {
    throw "Failed to setup Alpine musl sysroot"
}
