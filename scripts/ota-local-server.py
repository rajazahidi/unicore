#!/usr/bin/env python3
"""UniCore OS Local LAN & QEMU OTA Update Test Server.

Serves `os-builder/out/` (signed `unicore-manifest.json`, `.sig`, `vmlinuz`,
`initramfs`, `unicore-rootfs-*.squashfs`) over HTTP so real PCs on the same LAN
(e.g., HP Compaq dx2355 over Wi-Fi) and QEMU VMs (`http://10.0.2.2:<port>/...`)
can test full A/B Ed25519-verified OTA updates locally.
"""

import argparse
import functools
import http.server
import os
import socket
import sys
import urllib.request

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_DIR = os.path.join(REPO_ROOT, "os-builder", "out")


def detect_lan_ips():
    ips = []
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        primary = s.getsockname()[0]
        s.close()
        if primary and not primary.startswith("127."):
            ips.append(primary)
    except OSError:
        pass
    try:
        for info in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            ip = info[4][0]
            if ip and not ip.startswith("127.") and ip not in ips:
                ips.append(ip)
    except OSError:
        pass
    return ips


class OtaHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def run_self_test(serve_dir):
    handler = functools.partial(OtaHandler, directory=serve_dir)
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    import threading

    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    try:
        url = f"http://127.0.0.1:{port}/unicore-manifest.json"
        if os.path.isfile(os.path.join(serve_dir, "unicore-manifest.json")):
            with urllib.request.urlopen(url, timeout=5) as resp:
                body = resp.read()
                if resp.status != 200 or b'"version"' not in body:
                    raise RuntimeError(f"Unexpected manifest response: status={resp.status}")
            print(f"[PASS] Local OTA server self-test OK ({url}, {len(body)} bytes)")
        else:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=5) as resp:
                if resp.status != 200:
                    raise RuntimeError(f"Unexpected index response: status={resp.status}")
            print(f"[PASS] Local OTA server self-test OK (directory index served on port {port})")
    finally:
        httpd.shutdown()
        httpd.server_close()


def main():
    parser = argparse.ArgumentParser(description="UniCore OS Local OTA Update Server")
    parser.add_argument("--dir", default=DEFAULT_DIR, help="Directory containing OTA artifacts")
    parser.add_argument("--bind", default="0.0.0.0", help="Bind address (default: 0.0.0.0)")
    parser.add_argument("--port", type=int, default=8090, help="HTTP port (default: 8090)")
    parser.add_argument("--self-test", action="store_true", help="Run loopback health test and exit")
    args = parser.parse_args()

    serve_dir = os.path.abspath(args.dir)
    if not os.path.isdir(serve_dir):
        print(f"[ERROR] Artifact directory not found: {serve_dir}", file=sys.stderr)
        sys.exit(1)

    if args.self_test:
        run_self_test(serve_dir)
        return

    handler = functools.partial(OtaHandler, directory=serve_dir)
    httpd = http.server.ThreadingHTTPServer((args.bind, args.port), handler)
    lan_ips = detect_lan_ips()
    manifest_exists = os.path.isfile(os.path.join(serve_dir, "unicore-manifest.json"))
    sig_exists = os.path.isfile(os.path.join(serve_dir, "unicore-manifest.json.sig"))

    print("============================================================")
    print("  UniCore OS Local OTA Update Server")
    print("============================================================")
    print(f"  Serving dir : {serve_dir}")
    print(f"  Manifest    : {'READY' if manifest_exists else 'MISSING (build ISO first)'}")
    print(f"  Ed25519 Sig : {'SIGNED (.sig present)' if sig_exists else 'UNSIGNED (use --allow-unsigned or sign first)'}")
    print(f"  QEMU VM URL : http://10.0.2.2:{args.port}/unicore-manifest.json")
    for ip in lan_ips:
        print(f"  LAN PC URL  : http://{ip}:{args.port}/unicore-manifest.json")
    print("  Press Ctrl+C to stop.")
    print("============================================================")

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping OTA server.")
    finally:
        httpd.server_close()


if __name__ == "__main__":
    main()
