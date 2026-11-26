"""Generate a minimal structurally valid Android APK test fixture (Pure-DEX)."""

import sys
import zipfile
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: build_hello_apk.py <output.apk>", file=sys.stderr)
        return 1

    out_path = Path(sys.argv[1])
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # Minimal valid DEX header magic + dummy binary AndroidManifest.xml structure for inspector & staging tests.
    dex_header = b"dex\n035\x00" + b"\x00" * 104
    manifest_stub = b"\x03\x00\x08\x00" + b"com.unicore.hello\x00"

    with zipfile.ZipFile(out_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("AndroidManifest.xml", manifest_stub)
        zf.writestr("classes.dex", dex_header)
        zf.writestr("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\nCreated-By: UniCore Builder\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
