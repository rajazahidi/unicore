#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$ManifestPath = "os-builder\out\unicore-manifest.json",
    [string]$KeyDir = "os-builder\keys",
    [switch]$GenerateKey,
    [switch]$KeygenOnly,
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

function Test-PythonEd25519 {
    $py = Get-Command python -ErrorAction SilentlyContinue
    if (-not $py) { return $false }
    & $py.Source -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey" 2>$null
    return ($LASTEXITCODE -eq 0)
}

try {
    if (-not $KeygenOnly -and -not (Test-Path $ManifestPath)) {
        throw "Manifest not found: $ManifestPath. Run .\os-builder\bootstrap-via-qemu.ps1 first."
    }

    $OpenSsl = Resolve-OpenSsl
    $UsePythonFallback = $false
    if (-not $OpenSsl) {
        if (Test-PythonEd25519) {
            $UsePythonFallback = $true
        } else {
            throw "Neither OpenSSL CLI (openssl.exe) nor Python 'cryptography' package was found. Install OpenSSL or run 'pip install cryptography'."
        }
    }

    New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
    $PrivKey = Join-Path $KeyDir "release.key"
    $PubKey = Join-Path $KeyDir "update.pub"
    $SigFile = if ([string]::IsNullOrWhiteSpace($OutputSig)) { "$ManifestPath.sig" } else { $OutputSig }

    if (-not $UsePythonFallback) {
        $env:PATH = "$(Split-Path -Parent $OpenSsl);$env:PATH"
        if (-not (Test-Path $PrivKey) -or -not (Test-Path $PubKey)) {
            if (-not $GenerateKey) {
                throw "Release Ed25519 keys missing in $KeyDir. Re-run with -GenerateKey to create release.key and update.pub."
            }
            Write-Host "==> Generating Ed25519 signing keypair in $KeyDir (OpenSSL)"
            & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update keygen --private $PrivKey --public $PubKey
            if ($LASTEXITCODE -ne 0) { throw "uniload update keygen failed ($LASTEXITCODE)" }
        }
        if ($KeygenOnly) {
            Write-Host "Ed25519 keypair ready:"
            Write-Host "  PrivKey  : $PrivKey (KEEP PRIVATE - never commit!)"
            Write-Host "  PubKey   : $PubKey"
            return
        }

        Write-Host "==> Signing manifest $ManifestPath"
        & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update sign-manifest $ManifestPath --key $PrivKey --output $SigFile
        if ($LASTEXITCODE -ne 0) { throw "uniload update sign-manifest failed ($LASTEXITCODE)" }

        Write-Host "==> Verifying signature strictly (--require-signature)"
        & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update verify-manifest $ManifestPath --pubkey $PubKey --sig $SigFile --require-signature
        if ($LASTEXITCODE -ne 0) { throw "uniload update verify-manifest failed ($LASTEXITCODE)" }
    } else {
        if (-not (Test-Path $PrivKey) -or -not (Test-Path $PubKey)) {
            if (-not $GenerateKey) {
                throw "Release Ed25519 keys missing in $KeyDir. Re-run with -GenerateKey to create release.key and update.pub."
            }
            Write-Host "==> Generating Ed25519 signing keypair in $KeyDir (RFC 8410 PEM via Python cryptography)"
            @'
import os, sys
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

priv_path, pub_path = sys.argv[1], sys.argv[2]
priv = Ed25519PrivateKey.generate()
priv_pem = priv.private_bytes(
    encoding=serialization.Encoding.PEM,
    format=serialization.PrivateFormat.PKCS8,
    encryption_algorithm=serialization.NoEncryption(),
)
pub_pem = priv.public_key().public_bytes(
    encoding=serialization.Encoding.PEM,
    format=serialization.PublicFormat.SubjectPublicKeyInfo,
)
with open(priv_path, "wb") as f:
    f.write(priv_pem)
with open(pub_path, "wb") as f:
    f.write(pub_pem)
'@ | python - $PrivKey $PubKey
            if ($LASTEXITCODE -ne 0) { throw "Python Ed25519 keygen failed ($LASTEXITCODE)" }
        }
        if ($KeygenOnly) {
            Write-Host "Ed25519 keypair ready:"
            Write-Host "  PrivKey  : $PrivKey (KEEP PRIVATE - never commit!)"
            Write-Host "  PubKey   : $PubKey"
            return
        }

        Write-Host "==> Signing & strictly verifying manifest $ManifestPath (RFC 8410 raw Ed25519)"
        @'
import pathlib, sys
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey

manifest_path, priv_path, pub_path, sig_path = sys.argv[1:5]
data = pathlib.Path(manifest_path).read_bytes()
priv = serialization.load_pem_private_key(pathlib.Path(priv_path).read_bytes(), password=None)
if not isinstance(priv, Ed25519PrivateKey):
    raise TypeError("Private key is not Ed25519")
sig = priv.sign(data)
if len(sig) != 64:
    raise RuntimeError(f"Expected 64-byte raw Ed25519 signature, got {len(sig)}")
pathlib.Path(sig_path).write_bytes(sig)

pub = serialization.load_pem_public_key(pathlib.Path(pub_path).read_bytes())
if not isinstance(pub, Ed25519PublicKey):
    raise TypeError("Public key is not Ed25519")
pub.verify(sig, data)
print("Ed25519 signature generated and strictly verified (64-byte raw OpenSSL -rawin compatible).")
'@ | python - $ManifestPath $PrivKey $PubKey $SigFile
        if ($LASTEXITCODE -ne 0) { throw "Python Ed25519 sign/verify failed ($LASTEXITCODE)" }

        Write-Host "==> Validating manifest schema via uniload"
        & "$PSScriptRoot\cargo-dev.ps1" run -q -p uniload -- update verify-manifest $ManifestPath --allow-unsigned
        if ($LASTEXITCODE -ne 0) { throw "uniload update verify-manifest schema check failed ($LASTEXITCODE)" }
    }

    Write-Host ""
    Write-Host "Release manifest signed and verified:"
    Write-Host "  Manifest : $ManifestPath"
    Write-Host "  Sig      : $SigFile"
    Write-Host "  PubKey   : $PubKey"
} finally {
    Pop-Location
}
