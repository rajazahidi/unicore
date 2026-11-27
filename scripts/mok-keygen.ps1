#Requires -Version 5.1
<#
.SYNOPSIS
    Generates a UniCore OS UEFI Secure Boot Machine Owner Key (MOK) X.509 RSA-2048 certificate
    pair (mok.key, mok.crt, mok.der) for signing GRUB EFI and Linux kernels with sbsign.
#>
[CmdletBinding()]
param(
    [string]$KeyDir = "os-builder\keys",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

try {
    New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
    $MokKey = Join-Path $KeyDir "mok.key"
    $MokCrt = Join-Path $KeyDir "mok.crt"
    $MokDer = Join-Path $KeyDir "mok.der"

    if (-not $Force -and (Test-Path $MokKey) -and (Test-Path $MokCrt) -and (Test-Path $MokDer)) {
        Write-Host "UEFI Secure Boot MOK keypair already exists in $KeyDir (use -Force to regenerate):"
        Write-Host "  Key : $MokKey"
        Write-Host "  Cert: $MokCrt"
        Write-Host "  DER : $MokDer"
        return
    }

    Write-Host "==> Generating UniCore OS UEFI Secure Boot MOK X.509 keypair (RSA-2048 / SHA-256, 15y)"
    @'
import datetime, pathlib, sys
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

key_path, crt_path, der_path = [pathlib.Path(p) for p in sys.argv[1:4]]

priv = rsa.generate_private_key(public_exponent=65537, key_size=2048)
subject = issuer = x509.Name([
    x509.NameAttribute(NameOID.COMMON_NAME, "UniCore OS Secure Boot MOK"),
    x509.NameAttribute(NameOID.ORGANIZATION_NAME, "UniCore OS"),
])
now = datetime.datetime.now(datetime.timezone.utc)
cert = (
    x509.CertificateBuilder()
    .subject_name(subject)
    .issuer_name(issuer)
    .public_key(priv.public_key())
    .serial_number(x509.random_serial_number())
    .not_valid_before(now - datetime.timedelta(days=1))
    .not_valid_after(now + datetime.timedelta(days=365 * 15))
    .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
    .add_extension(
        x509.KeyUsage(
            digital_signature=True,
            content_commitment=False,
            key_encipherment=False,
            data_encipherment=False,
            key_agreement=False,
            key_cert_sign=False,
            crl_sign=False,
            encipher_only=False,
            decipher_only=False,
        ),
        critical=True,
    )
    .add_extension(
        x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CODE_SIGNING]),
        critical=False,
    )
    .sign(priv, hashes.SHA256())
)

key_path.write_bytes(
    priv.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )
)
crt_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
der_path.write_bytes(cert.public_bytes(serialization.Encoding.DER))
fp = cert.fingerprint(hashes.SHA256()).hex()
print(f"[OK] Generated MOK X.509 certificate (SHA-256: {fp})")
'@ | python - $MokKey $MokCrt $MokDer
    if ($LASTEXITCODE -ne 0) { throw "MOK keygen failed ($LASTEXITCODE)" }

    Write-Host ""
    Write-Host "UEFI Secure Boot MOK files ready (gitignored in $KeyDir):"
    Write-Host "  Private Key : $MokKey  (KEEP PRIVATE - upload to GitHub Secret UNICORE_MOK_KEY)"
    Write-Host "  PEM Cert    : $MokCrt  (upload to GitHub Secret UNICORE_MOK_CERT)"
    Write-Host "  DER Cert    : $MokDer  (embedded into ISO / EFI as ENROLL_UNICORE_MOK.cer)"
} finally {
    Pop-Location
}
