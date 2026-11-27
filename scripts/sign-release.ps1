#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$ManifestPath = "os-builder\out\unicore-manifest.json",
    [string]$KeyDir = "os-builder\keys",
    [switch]$GenerateKey,
    [string]$OutputSig = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

function Resolve-OpenSsl {
    $cmd = Get-Command openssl -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $candidates = @(
        "C:\Program Files\Git\usr\bin\openssl.exe",
        "C:\Program Files\Git\mingw64\bin\openssl.exe",
        "$env:LOCALAPPDATA\Programs\Git\usr\bin\openssl.exe",
        "$env:LOCALAPPDATA\Programs\Git\mingw64\bin\openssl.exe",
        "C:\Program Files\OpenSSL-Win64\bin\openssl.exe",
        "C:\Program Files\OpenSSL-Win32\bin\openssl.exe"
    )
    foreach ($p in $candidates) {
        if (Test-Path $p) { return $p }
    }
    return $null
}

try {
    if (-not (Test-Path $ManifestPath)) {
        throw "Manifest not found: $ManifestPath. Run .\os-builder\bootstrap-via-qemu.ps1 first."
    }

    $OpenSsl = Resolve-OpenSsl
    if (-not $OpenSsl) {
        throw "OpenSSL CLI (openssl.exe) was not found in PATH or standard Windows install locations. Install OpenSSL or run 'uniload update keygen/sign-manifest' inside WSL or UniCore OS."
    }

    $env:PATH = "$(Split-Path -Parent $OpenSsl);$env:PATH"
    New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
    $PrivKey = Join-Path $KeyDir "release.key"
    $PubKey = Join-Path $KeyDir "update.pub"
    $SigFile = if ([string]::IsNullOrWhiteSpace($OutputSig)) { "$ManifestPath.sig" } else { $OutputSig }

    if (-not (Test-Path $PrivKey) -or -not (Test-Path $PubKey)) {
        if (-not $GenerateKey) {
            throw "Release Ed25519 keys missing in $KeyDir. Re-run with -GenerateKey to create release.key and update.pub."
        }
        Write-Host "==> Generating Ed25519 signing keypair in $KeyDir"
        & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update keygen --private $PrivKey --public $PubKey
        if ($LASTEXITCODE -ne 0) { throw "uniload update keygen failed ($LASTEXITCODE)" }
    }

    Write-Host "==> Signing manifest $ManifestPath"
    & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update sign-manifest $ManifestPath --key $PrivKey --output $SigFile
    if ($LASTEXITCODE -ne 0) { throw "uniload update sign-manifest failed ($LASTEXITCODE)" }

    Write-Host "==> Verifying signature strictly (--require-signature)"
    & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update verify-manifest $ManifestPath --pubkey $PubKey --sig $SigFile --require-signature
    if ($LASTEXITCODE -ne 0) { throw "uniload update verify-manifest failed ($LASTEXITCODE)" }

    Write-Host ""
    Write-Host "Release manifest signed and verified:"
    Write-Host "  Manifest : $ManifestPath"
    Write-Host "  Sig      : $SigFile"
    Write-Host "  PubKey   : $PubKey"
} finally {
    Pop-Location
}
