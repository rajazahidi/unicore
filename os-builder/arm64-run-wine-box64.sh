#!/bin/sh
# Executes x86_64/x86 Windows .exe via Box64 + Wine WoW64 inside an isolated ARM64 glibc runtime.
set -eu

RT="${UNICORE_ARM64_WIN32_DIR:-/usr/share/unicore/arm64-win32}"
export BOX64_LD_LIBRARY_PATH="${RT}/opt/wine/lib/wine/x86_64-unix:${RT}/sysroot/usr/lib/x86_64-linux-gnu"
export BOX64_DYNAREC=1
export BOX64_NOBANNER=1
export WINELOADER="${RT}/opt/wine/bin/wine"
export WINESERVER="${RT}/opt/wine/bin/wineserver"

GLIBC_PATH="${RT}/sysroot/lib/aarch64-linux-gnu:${RT}/sysroot/usr/lib/aarch64-linux-gnu"
exec "${RT}/sysroot/lib/ld-linux-aarch64.so.1" \
    --library-path "${GLIBC_PATH}" \
    "${RT}/bin/box64" "${RT}/opt/wine/bin/wine" "$@"
