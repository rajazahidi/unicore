#!/usr/bin/env bash
set -euo pipefail

# UniCore OS Bootable Image Builder (run inside Alpine Linux VM or WSL2/Linux).
# Installs minimal Alpine + labwc Wayland + XWayland + lightweight Win32 core,
# injects native Rust `uniload` + binfmt_misc + UniCore overlay,
# and outputs vmlinuz-x86_64, initramfs-x86_64.img, and unicore-rootfs-x86_64.squashfs.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT_OVERRIDE:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
OUT_DIR="${OUT_DIR_OVERRIDE:-${SCRIPT_DIR}/out}"
WORK_DIR="${WORK_DIR_OVERRIDE:-${OUT_DIR}/work}"
ROOTFS_DIR="${WORK_DIR}/rootfs"
INITRD_DIR="${WORK_DIR}/initrd"
ALPINE_VERSION="${ALPINE_VERSION:-3.20}"
ARCH="${TARGET_ARCH:-x86_64}"
RUST_TARGET="${RUST_TARGET:-${ARCH}-unknown-linux-musl}"

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "[ERROR] Required build tool missing: ${cmd}" >&2
        exit 1
    fi
}

echo "==> Checking build environment (${ARCH})"
for tool in tar mksquashfs gzip find depmod; do
    require_cmd "$tool"
done
if ! command -v cpio >/dev/null 2>&1 && ! busybox cpio --help >/dev/null 2>&1; then
    echo "[ERROR] Required build tool missing: cpio (or busybox cpio)" >&2
    exit 1
fi

# Locate or build static Rust uniload binary
UNILOAD_BIN="${UNILOAD_BIN_OVERRIDE:-}"
if [[ -z "${UNILOAD_BIN}" ]]; then
    require_cmd cargo
    if command -v rustup >/dev/null 2>&1 && rustup target list --installed | grep -qx "${RUST_TARGET}"; then
        echo "==> Building static Rust uniload (${RUST_TARGET})"
        cargo build --release --manifest-path "${REPO_ROOT}/Cargo.toml" -p uniload --target "${RUST_TARGET}"
        UNILOAD_BIN="${REPO_ROOT}/target/${RUST_TARGET}/release/uniload"
    else
        echo "==> Building host Linux Rust uniload"
        cargo build --release --manifest-path "${REPO_ROOT}/Cargo.toml" -p uniload
        UNILOAD_BIN="${REPO_ROOT}/target/release/uniload"
    fi
fi

if [[ ! -f "${UNILOAD_BIN}" ]]; then
    echo "[ERROR] uniload binary not found at: ${UNILOAD_BIN}" >&2
    exit 1
fi

UNICORE_CENTER_BIN="${UNICORE_CENTER_BIN_OVERRIDE:-}"
if [[ -z "${UNICORE_CENTER_BIN}" ]]; then
    require_cmd cargo
    echo "==> Building UniCore App & Security Center GUI"
    cargo build --release --manifest-path "${REPO_ROOT}/Cargo.toml" -p unicore-center
    UNICORE_CENTER_BIN="${REPO_ROOT}/target/release/unicore-center"
fi

if [[ ! -f "${UNICORE_CENTER_BIN}" ]]; then
    echo "[ERROR] unicore-center binary not found at: ${UNICORE_CENTER_BIN}" >&2
    exit 1
fi

mkdir -p "${OUT_DIR}" "${WORK_DIR}"
rm -rf "${ROOTFS_DIR}" "${INITRD_DIR}"
mkdir -p "${ROOTFS_DIR}" "${INITRD_DIR}"

APK_BIN="${APK_BIN_OVERRIDE:-apk}"
if ! command -v "${APK_BIN}" >/dev/null 2>&1; then
    echo "[ERROR] Alpine 'apk' tool is required (run inside Alpine Linux or pass APK_BIN_OVERRIDE)." >&2
    exit 1
fi

MIRROR_MAIN="http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION}/main"
MIRROR_COMM="http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION}/community"

DESKTOP_PKGS=(
    fuzzel
    pcmanfm
    grim
    shared-mime-info
    desktop-file-utils
    adwaita-icon-theme
    doas
    networkmanager
    networkmanager-wifi
    networkmanager-cli
    networkmanager-openrc
    wpa_supplicant
    bluez
    bluez-openrc
    wlr-randr
    brightnessctl
    pulseaudio-utils
    wireplumber
    epiphany
    gnome-keyring
    linux-firmware
    sof-firmware
    alsa-ucm-conf
    grub
    grub-efi
    parted
    dosfstools
    efibootmgr
    lsblk
    curl
    openssl
    bubblewrap
    cryptsetup
    tzdata
    unzip
    swaybg
    mako
    libnotify
    swaylock
    wl-clipboard
    slurp
    wlrctl
    udisks2
    gvfs
    gvfs-fuse
    gvfs-mtp
    polkit
    polkit-openrc
    xdg-user-dirs
    mousepad
    imv
    xarchiver
    vulkan-loader
    vulkan-tools
    mesa-vulkan-ati
    mesa-vulkan-intel
    mesa-va-gallium
    mpv
    celluloid
    ffmpeg
    gst-plugins-good
    gst-plugins-bad
    gst-plugins-ugly
    gst-libav
    mangohud
    tar
    zstd
    cabextract
    dosbox-staging
    cups
    cups-client
    cups-filters
    cups-openrc
    wlsunset
    ntfs-3g
    exfatprogs
    fuse3
    wireguard-tools
    openvpn
    networkmanager-openvpn
    abiword
    gnumeric
    evince
    sane
    sane-utils
    sane-udev
    sane-backend-escl
    sane-backend-hp
    sane-backend-pixma
    sane-backend-epson2
    sane-backend-net
    simple-scan
    swayidle
    smartmontools
    shadow
    git
    python3
    openssh-client
    jq
    podman
    crun
    slirp4netns
    fuse-overlayfs
)

if [[ "${ARCH}" == "x86_64" || "${ARCH}" == "x86" ]]; then
    DESKTOP_PKGS+=(grub-bios intel-media-driver libva-intel-driver)
fi

BASE_PKGS=(
    alpine-base
    openrc
    busybox-openrc
    linux-edge
    e2fsprogs
    util-linux
    kmod
    eudev
    seatd
    dbus
    labwc
    xwayland
    foot
    waybar
    mesa-dri-gallium
    mesa-egl
    mesa-gles
    mesa-vulkan-swrast
    pciutils
    pipewire
    pipewire-pulse
    alsa-lib
    font-dejavu
    waydroid
    waydroid-openrc
    waydroid-nftables
    nftables
    iproute2
    bash
)

if [[ "${ARCH}" == "x86_64" || "${ARCH}" == "x86" ]]; then
    BASE_PKGS+=(wine)
fi

run_apk_add() {
    local initdb="$1"
    shift
    local -a apk_flags=(
        --root "${ROOTFS_DIR}"
        --no-cache
        --arch "${ARCH}"
        --repository "${MIRROR_MAIN}"
        --repository "${MIRROR_COMM}"
    )
    if [[ "${initdb}" == "true" ]]; then
        apk_flags+=(--initdb)
    fi
    if [[ -d /etc/apk/keys ]]; then
        apk_flags+=(--keys-dir /etc/apk/keys)
    else
        apk_flags+=(--allow-untrusted)
    fi

    for attempt in 1 2 3; do
        if "${APK_BIN}" "${apk_flags[@]}" add "$@"; then
            return 0
        fi
        if [[ "${attempt}" -eq 3 ]]; then
            echo "[ERROR] apk add failed after 3 attempts" >&2
            exit 1
        fi
        echo "==> Retrying apk add (${attempt}/3)..."
        sleep 2
    done
}

if [[ -n "${BASE_SQUASHFS_OVERRIDE:-}" && ( -b "${BASE_SQUASHFS_OVERRIDE}" || -f "${BASE_SQUASHFS_OVERRIDE}" ) ]]; then
    echo "==> Hydrating rootfs from existing base SquashFS (${BASE_SQUASHFS_OVERRIDE})"
    require_cmd unsquashfs
    unsquashfs -f -d "${ROOTFS_DIR}" "${BASE_SQUASHFS_OVERRIDE}" >/dev/null
    mkdir -p "${ROOTFS_DIR}/dev"
    [ -e "${ROOTFS_DIR}/dev/null" ] || mknod -m 666 "${ROOTFS_DIR}/dev/null" c 1 3 2>/dev/null || true
    [ -e "${ROOTFS_DIR}/dev/zero" ] || mknod -m 666 "${ROOTFS_DIR}/dev/zero" c 1 5 2>/dev/null || true
    [ -e "${ROOTFS_DIR}/dev/urandom" ] || mknod -m 666 "${ROOTFS_DIR}/dev/urandom" c 1 9 2>/dev/null || true
    for sym in \
        "${ROOTFS_DIR}/etc/NetworkManager/system-connections" \
        "${ROOTFS_DIR}/var/lib/bluetooth" \
        "${ROOTFS_DIR}/etc/cups" \
        "${ROOTFS_DIR}/usr/share/applications/mimeapps.list"; do
        [ -L "${sym}" ] && rm -f "${sym}"
    done
    mkdir -p "${ROOTFS_DIR}/etc/NetworkManager/system-connections" "${ROOTFS_DIR}/var/lib/bluetooth" "${ROOTFS_DIR}/etc/cups"
    missing_pkgs=()
    for pkg in "${DESKTOP_PKGS[@]}"; do
        if ! "${APK_BIN}" --root "${ROOTFS_DIR}" info -e "${pkg}" >/dev/null 2>&1; then
            missing_pkgs+=("${pkg}")
        fi
    done
    if [[ "${#missing_pkgs[@]}" -gt 0 ]]; then
        echo "==> Installing missing incremental packages: ${missing_pkgs[*]}"
        run_apk_add false "${missing_pkgs[@]}"
    else
        echo "==> All incremental desktop packages already present in base SquashFS; skipping apk add"
    fi
else
    echo "==> Installing UniCore base OS (linux-edge), Wayland (labwc), Win32, and Android Waydroid core"
    run_apk_add true "${BASE_PKGS[@]}" "${DESKTOP_PKGS[@]}"
fi

echo "==> Configuring UniCore users, services, and overlay"
mkdir -p \
    "${ROOTFS_DIR}/etc/binfmt.d" \
    "${ROOTFS_DIR}/etc/uniload" \
    "${ROOTFS_DIR}/etc/unicore" \
    "${ROOTFS_DIR}/etc/doas.d" \
    "${ROOTFS_DIR}/etc/NetworkManager" \
    "${ROOTFS_DIR}/etc/dbus-1/system.d" \
    "${ROOTFS_DIR}/usr/bin" \
    "${ROOTFS_DIR}/usr/sbin" \
    "${ROOTFS_DIR}/usr/libexec" \
    "${ROOTFS_DIR}/mnt/unicore-media" \
    "${ROOTFS_DIR}/data/sandboxes" \
    "${ROOTFS_DIR}/data/network" \
    "${ROOTFS_DIR}/data/bluetooth" \
    "${ROOTFS_DIR}/data/cups" \
    "${ROOTFS_DIR}/var/lib" \
    "${ROOTFS_DIR}/usr/share/unicore/cups-defaults" \
    "${ROOTFS_DIR}/usr/share/unicore"

cp -a "${SCRIPT_DIR}/overlay/." "${ROOTFS_DIR}/"
rm -f "${ROOTFS_DIR}"/usr/share/applications/*dosbox*.desktop "${ROOTFS_DIR}"/usr/share/applications/*cups*.desktop
ln -snf /etc/xdg/mimeapps.list "${ROOTFS_DIR}/usr/share/applications/mimeapps.list"
rm -rf \
    "${ROOTFS_DIR}/home/unicore/.config/labwc" \
    "${ROOTFS_DIR}/home/unicore/.config/waybar" \
    "${ROOTFS_DIR}/home/unicore/.config/fuzzel"
rm -rf "${ROOTFS_DIR}/etc/NetworkManager/system-connections" "${ROOTFS_DIR}/var/lib/bluetooth"
ln -snf /data/network "${ROOTFS_DIR}/etc/NetworkManager/system-connections"
ln -snf /data/bluetooth "${ROOTFS_DIR}/var/lib/bluetooth"
if [[ -d "${ROOTFS_DIR}/etc/cups" && ! -L "${ROOTFS_DIR}/etc/cups" ]]; then
    cp -a "${ROOTFS_DIR}/etc/cups/." "${ROOTFS_DIR}/usr/share/unicore/cups-defaults/" 2>/dev/null || true
    rm -rf "${ROOTFS_DIR}/etc/cups"
fi
ln -snf /data/cups "${ROOTFS_DIR}/etc/cups"
cp "${SCRIPT_DIR}/binfmt-universal.conf" "${ROOTFS_DIR}/etc/binfmt.d/binfmt-universal.conf"
cp "${SCRIPT_DIR}/kernel.config.fragment" "${ROOTFS_DIR}/etc/uniload/kernel.config.fragment"
if [[ -n "${UPDATE_PUBKEY_OVERRIDE:-}" && -f "${UPDATE_PUBKEY_OVERRIDE}" ]]; then
    install -m 0644 "${UPDATE_PUBKEY_OVERRIDE}" "${ROOTFS_DIR}/etc/unicore/update.pub"
    if grep -q '^REQUIRE_SIGNATURE=' "${ROOTFS_DIR}/etc/unicore/update.conf" 2>/dev/null; then
        sed -i 's/^REQUIRE_SIGNATURE=.*/REQUIRE_SIGNATURE=1/' "${ROOTFS_DIR}/etc/unicore/update.conf"
    else
        printf 'REQUIRE_SIGNATURE=1\n' >> "${ROOTFS_DIR}/etc/unicore/update.conf"
    fi
fi

install -m 0755 "${UNILOAD_BIN}" "${ROOTFS_DIR}/usr/bin/uniload"
install -m 0755 "${UNICORE_CENTER_BIN}" "${ROOTFS_DIR}/usr/bin/unicore-center"
chmod 0755 \
    "${ROOTFS_DIR}/usr/bin/unicore-setup-data" \
    "${ROOTFS_DIR}/usr/bin/unicore-diag" \
    "${ROOTFS_DIR}/usr/sbin/unicore-install" \
    "${ROOTFS_DIR}/usr/sbin/unicore-update" \
    "${ROOTFS_DIR}/usr/libexec/uniload-win32" \
    "${ROOTFS_DIR}/usr/libexec/uniload-win32-assets" \
    "${ROOTFS_DIR}/usr/libexec/uniload-dos" \
    "${ROOTFS_DIR}/usr/libexec/uniload-art" \
    "${ROOTFS_DIR}/usr/libexec/uniload-native" \
    "${ROOTFS_DIR}/usr/libexec/uniload-sandbox-exec" \
    "${ROOTFS_DIR}/usr/libexec/unicore-app-runner" \
    "${ROOTFS_DIR}/usr/libexec/unicore-screenshot" \
    "${ROOTFS_DIR}/usr/libexec/unicore-lock" \
    "${ROOTFS_DIR}/usr/libexec/unicore-session" \
    "${ROOTFS_DIR}/usr/libexec/unicore-sys-helper" \
    "${ROOTFS_DIR}/usr/libexec/unicore-clip-store" \
    "${ROOTFS_DIR}/usr/libexec/unicore-clipboard" \
    "${ROOTFS_DIR}/usr/libexec/unicore-nightlight" \
    "${ROOTFS_DIR}/usr/libexec/unicore-firewall" \
    "${ROOTFS_DIR}/usr/libexec/unicore-backup" \
    "${ROOTFS_DIR}/usr/libexec/unicore-personalize" \
    "${ROOTFS_DIR}/usr/libexec/unicore-users" \
    "${ROOTFS_DIR}/usr/libexec/unicore-power" \
    "${ROOTFS_DIR}/usr/libexec/unicore-smart" \
    "${ROOTFS_DIR}/usr/libexec/unicore-devbox" \
    "${ROOTFS_DIR}/usr/libexec/unicore-ai" \
    "${ROOTFS_DIR}/etc/local.d/unicore-binfmt.start" \
    "${ROOTFS_DIR}/etc/local.d/unicore-android.start" \
    "${ROOTFS_DIR}/etc/local.d/unicore-hwprofile.start" \
    "${ROOTFS_DIR}/etc/local.d/unicore-connectivity.start" \
    "${ROOTFS_DIR}/etc/local.d/unicore-users.start" \
    "${ROOTFS_DIR}/etc/xdg/labwc/autostart"
chown 0:0 "${ROOTFS_DIR}/etc/doas.d/unicore.conf"
chmod 0600 "${ROOTFS_DIR}/etc/doas.d/unicore.conf"
cp "${ROOTFS_DIR}/etc/doas.d/unicore.conf" "${ROOTFS_DIR}/etc/doas.conf"
chown 0:0 "${ROOTFS_DIR}/etc/doas.conf"
chmod 0600 "${ROOTFS_DIR}/etc/doas.conf"
chmod 0700 "${ROOTFS_DIR}/data/network" "${ROOTFS_DIR}/data/bluetooth"
chmod 0755 "${ROOTFS_DIR}/data/cups"

cat <<'EOF' > "${ROOTFS_DIR}/usr/libexec/uniload-exec"
#!/bin/sh
set -eu
TARGET_BIN="$1"
shift || true
if [ "$#" -gt 0 ]; then
    exec /usr/bin/uniload --state-dir /data/uniload launch "${TARGET_BIN}" -- "$@"
fi
exec /usr/bin/uniload --state-dir /data/uniload launch "${TARGET_BIN}"
EOF
chmod +x "${ROOTFS_DIR}/usr/libexec/uniload-exec"

cat <<'EOF' > "${ROOTFS_DIR}/etc/uniload/subsystems.conf"
# Built-in on-demand subsystems (lazy loaded only when .exe, .com, .bat, or .apk runs)
WIN32_RUNTIME=/usr/libexec/uniload-win32
DOS_RUNTIME=/usr/libexec/uniload-dos
ANDROID_ART_RUNTIME=/usr/libexec/uniload-art
IDLE_HIBERNATE_SECONDS=30
EOF

if [[ -n "${HELLO_EXE_OVERRIDE:-}" && -f "${HELLO_EXE_OVERRIDE}" ]]; then
    install -m 0755 "${HELLO_EXE_OVERRIDE}" "${ROOTFS_DIR}/usr/share/unicore/unicore_hello.exe"
fi
if [[ -n "${HELLO_APK_OVERRIDE:-}" && -f "${HELLO_APK_OVERRIDE}" ]]; then
    install -m 0644 "${HELLO_APK_OVERRIDE}" "${ROOTFS_DIR}/usr/share/unicore/unicore_hello.apk"
fi

if [[ -n "${DXVK_ARCHIVE_OVERRIDE:-}" && -f "${DXVK_ARCHIVE_OVERRIDE}" ]]; then
    echo "==> Pre-bundling DXVK runtime into /usr/share/dxvk"
    rm -rf "${ROOTFS_DIR}/usr/share/dxvk"
    mkdir -p "${ROOTFS_DIR}/usr/share/dxvk"
    tar -xzf "${DXVK_ARCHIVE_OVERRIDE}" -C "${ROOTFS_DIR}/usr/share/dxvk" --strip-components=1
fi

if [[ -n "${VKD3D_ARCHIVE_OVERRIDE:-}" && -f "${VKD3D_ARCHIVE_OVERRIDE}" ]]; then
    echo "==> Pre-bundling VKD3D-Proton runtime into /usr/share/vkd3d-proton"
    rm -rf "${ROOTFS_DIR}/usr/share/vkd3d-proton"
    mkdir -p "${ROOTFS_DIR}/usr/share/vkd3d-proton"
    tar --zstd -xf "${VKD3D_ARCHIVE_OVERRIDE}" -C "${ROOTFS_DIR}/usr/share/vkd3d-proton" --strip-components=1
fi

if [[ -n "${CNCDDRAW_ARCHIVE_OVERRIDE:-}" && -f "${CNCDDRAW_ARCHIVE_OVERRIDE}" ]]; then
    echo "==> Pre-bundling cnc-ddraw runtime into /usr/share/cnc-ddraw"
    rm -rf "${ROOTFS_DIR}/usr/share/cnc-ddraw"
    mkdir -p "${ROOTFS_DIR}/usr/share/cnc-ddraw"
    unzip -q -o "${CNCDDRAW_ARCHIVE_OVERRIDE}" -d "${ROOTFS_DIR}/usr/share/cnc-ddraw"
fi

if [[ -n "${OTVDM_ARCHIVE_OVERRIDE:-}" && -f "${OTVDM_ARCHIVE_OVERRIDE}" ]]; then
    echo "==> Pre-bundling OTVDM (Win16 NE) runtime into /usr/share/unicore/otvdm"
    rm -rf "${ROOTFS_DIR}/usr/share/unicore/otvdm" "${WORK_DIR}/otvdm_stage"
    mkdir -p "${ROOTFS_DIR}/usr/share/unicore/otvdm" "${WORK_DIR}/otvdm_stage"
    unzip -q -o "${OTVDM_ARCHIVE_OVERRIDE}" -d "${WORK_DIR}/otvdm_stage"
    otvdm_exe="$(find "${WORK_DIR}/otvdm_stage" -maxdepth 3 -name 'otvdm.exe' | head -n1 || true)"
    if [[ -n "${otvdm_exe}" ]]; then
        cp -a "$(dirname "${otvdm_exe}")/." "${ROOTFS_DIR}/usr/share/unicore/otvdm/"
    else
        cp -a "${WORK_DIR}/otvdm_stage/." "${ROOTFS_DIR}/usr/share/unicore/otvdm/"
    fi
    rm -rf "${WORK_DIR}/otvdm_stage"
fi

if [[ -n "${WINETRICKS_SCRIPT_OVERRIDE:-}" && -f "${WINETRICKS_SCRIPT_OVERRIDE}" ]]; then
    install -m 0755 "${WINETRICKS_SCRIPT_OVERRIDE}" "${ROOTFS_DIR}/usr/bin/winetricks"
fi

# Create non-root desktop user `unicore` inside rootfs image
chroot "${ROOTFS_DIR}" /bin/sh -c '
    set -eu
    if ! id -u unicore >/dev/null 2>&1; then
        adduser -D -s /bin/sh unicore
    fi
    for grp in video input seat audio users plugdev netdev bluetooth lp lpadmin scanner; do
        addgroup "${grp}" 2>/dev/null || true
        addgroup unicore "${grp}" 2>/dev/null || true
    done
    grep -q '^unicore:' /etc/subuid 2>/dev/null || echo 'unicore:100000:65536' >> /etc/subuid
    grep -q '^unicore:' /etc/subgid 2>/dev/null || echo 'unicore:100000:65536' >> /etc/subgid
    chown -R unicore:unicore /home/unicore
    update-mime-database /usr/share/mime || true
    update-desktop-database /usr/share/applications || true
'

cat <<'EOF' > "${ROOTFS_DIR}/etc/inittab"
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default

tty1::respawn:/usr/libexec/unicore-session
ttyS0::respawn:/sbin/getty -n -l /bin/sh -L 115200 ttyS0 vt100

::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/openrc shutdown
EOF

# Enable essential OpenRC runlevels
mkdir -p \
    "${ROOTFS_DIR}/etc/runlevels/sysinit" \
    "${ROOTFS_DIR}/etc/runlevels/boot" \
    "${ROOTFS_DIR}/etc/runlevels/default"

for svc in devfs dmesg udev udev-trigger udev-settle; do
    [[ -e "${ROOTFS_DIR}/etc/init.d/${svc}" ]] && ln -sf "/etc/init.d/${svc}" "${ROOTFS_DIR}/etc/runlevels/sysinit/${svc}"
done
for svc in modules sysctl hostname bootmisc; do
    [[ -e "${ROOTFS_DIR}/etc/init.d/${svc}" ]] && ln -sf "/etc/init.d/${svc}" "${ROOTFS_DIR}/etc/runlevels/boot/${svc}"
done
for svc in seatd dbus polkit networkmanager local; do
    [[ -e "${ROOTFS_DIR}/etc/init.d/${svc}" ]] && ln -sf "/etc/init.d/${svc}" "${ROOTFS_DIR}/etc/runlevels/default/${svc}"
done

echo "==> Extracting kernel and building Stage-1 initramfs"
VMLINUZ_SRC="$(find "${ROOTFS_DIR}/boot" -maxdepth 1 -name 'vmlinuz-*' | head -n1)"
if [[ -z "${VMLINUZ_SRC}" ]]; then
    echo "[ERROR] No vmlinuz kernel found in ${ROOTFS_DIR}/boot" >&2
    exit 1
fi
cp "${VMLINUZ_SRC}" "${OUT_DIR}/vmlinuz-${ARCH}"

KVER="$(basename "$(find "${ROOTFS_DIR}/lib/modules" -mindepth 1 -maxdepth 1 -type d | head -n1)")"
mkdir -p "${INITRD_DIR}/bin" "${INITRD_DIR}/sbin" "${INITRD_DIR}/lib" "${INITRD_DIR}/usr/lib" "${INITRD_DIR}/lib/modules/${KVER}"

# Copy dynamically linked Stage-1 utilities + shared libs cleanly (dereferencing symlinks)
copy_bin_with_libs() {
    local bin_rel
    bin_rel="$(chroot "${ROOTFS_DIR}" /bin/sh -c "command -v '$1' 2>/dev/null || true")"
    [[ -n "${bin_rel}" && -e "${ROOTFS_DIR}${bin_rel}" ]] || return 0
    mkdir -p "$(dirname "${INITRD_DIR}${bin_rel}")"
    cp -L "${ROOTFS_DIR}${bin_rel}" "${INITRD_DIR}${bin_rel}"
    chmod 0755 "${INITRD_DIR}${bin_rel}"

    chroot "${ROOTFS_DIR}" /bin/sh -c "ldd '${bin_rel}' 2>/dev/null || true" | awk '/=>/ {print $3} /^\// {print $1}' | while read -r lib; do
        [[ -n "${lib}" && -e "${ROOTFS_DIR}${lib}" ]] || continue
        mkdir -p "$(dirname "${INITRD_DIR}${lib}")"
        cp -L "${ROOTFS_DIR}${lib}" "${INITRD_DIR}${lib}"
    done
}

copy_bin_with_libs /bin/busybox
copy_bin_with_libs mkfs.ext4
copy_bin_with_libs blkid
copy_bin_with_libs kmod
copy_bin_with_libs cryptsetup
copy_bin_with_libs dmsetup

for applet in sh ash grep head cat ls echo sleep mount umount mkdir rmdir rm cp ln chmod sync switch_root blkid modprobe losetup poweroff reboot uname basename dirname tr cut sort sed awk find dmesg lsmod printf; do
    if [[ ! -e "${INITRD_DIR}/bin/${applet}" && ! -e "${INITRD_DIR}/sbin/${applet}" ]]; then
        ln -sf /bin/busybox "${INITRD_DIR}/bin/${applet}"
    fi
done
if [[ -e "${INITRD_DIR}/bin/kmod" ]]; then
    ln -sf /bin/kmod "${INITRD_DIR}/sbin/modprobe"
elif [[ -e "${INITRD_DIR}/usr/bin/kmod" ]]; then
    ln -sf /usr/bin/kmod "${INITRD_DIR}/sbin/modprobe"
fi

# Copy essential 2005-2026 storage, PCI/VMD, MMC, USB, HID keyboard, CD, and fs kernel modules into initramfs
if [[ -d "${ROOTFS_DIR}/lib/modules/${KVER}" ]]; then
    (
        cd "${ROOTFS_DIR}"
        find "lib/modules/${KVER}" -type f \( \
            -path "*/kernel/drivers/block/*" -o \
            -path "*/kernel/drivers/virtio/*" -o \
            -path "*/kernel/drivers/ata/*" -o \
            -path "*/kernel/drivers/scsi/*" -o \
            -path "*/kernel/drivers/nvme/*" -o \
            -path "*/kernel/drivers/pci/controller/*" -o \
            -path "*/kernel/drivers/mmc/*" -o \
            -path "*/kernel/drivers/misc/cardreader/*" -o \
            -path "*/kernel/drivers/cdrom/*" -o \
            -path "*/kernel/drivers/phy/*" -o \
            -path "*/kernel/drivers/usb/common/*" -o \
            -path "*/kernel/drivers/usb/core/*" -o \
            -path "*/kernel/drivers/usb/host/*" -o \
            -path "*/kernel/drivers/usb/phy/*" -o \
            -path "*/kernel/drivers/usb/storage/*" -o \
            -path "*/kernel/drivers/hid/hid.ko*" -o \
            -path "*/kernel/drivers/hid/hid-generic.ko*" -o \
            -path "*/kernel/drivers/hid/usbhid/*" -o \
            -path "*/kernel/drivers/hid/i2c-hid/*" -o \
            -path "*/kernel/drivers/input/serio/*" -o \
            -path "*/kernel/drivers/input/keyboard/atkbd*" -o \
            -path "*/kernel/drivers/md/*" -o \
            -path "*/kernel/fs/squashfs/*" -o \
            -path "*/kernel/fs/overlayfs/*" -o \
            -path "*/kernel/fs/ext4/*" -o \
            -path "*/kernel/fs/jbd2/*" -o \
            -path "*/kernel/fs/mbcache*" -o \
            -path "*/kernel/fs/isofs/*" -o \
            -path "*/kernel/fs/fat/*" -o \
            -path "*/kernel/fs/exfat/*" -o \
            -path "*/kernel/fs/ntfs3/*" -o \
            -path "*/kernel/fs/nls/*" -o \
            -path "*/kernel/fs/binfmt_misc*" -o \
            -path "*/kernel/fs/crypto/*" -o \
            -path "*/kernel/lib/*" -o \
            -path "*/kernel/arch/*/crypto/*" -o \
            -path "*/kernel/crypto/*" \
        \) | while IFS= read -r modrel; do
            mkdir -p "${INITRD_DIR}/$(dirname "${modrel}")"
            cp -a "${modrel}" "${INITRD_DIR}/${modrel}"
        done
        # Copy all transitive dependencies listed in the full rootfs modules.dep
        # (pulls in encrypted-keys, trusted, tpm, tee, hwmon, mfd, etc. for dm-crypt, nvme, cardreaders & hid)
        if [[ -f "lib/modules/${KVER}/modules.dep" ]]; then
            for _dep_pass in 1 2 3 4 5; do
                (cd "${INITRD_DIR}/lib/modules/${KVER}" && find kernel -type f) > "${WORK_DIR}/initrd_mods.list"
                new_deps="$(awk -F: '
                    NR==FNR { have[$0]=1; next }
                    {
                        mod=$1; deps=$2
                        sub(/^[[:space:]]+/, "", deps)
                        if (mod in have && deps != "") {
                            n = split(deps, arr, /[[:space:]]+/)
                            for (i = 1; i <= n; i++) {
                                if (arr[i] != "" && !(arr[i] in have)) {
                                    print arr[i]
                                    have[arr[i]] = 1
                                }
                            }
                        }
                    }
                ' "${WORK_DIR}/initrd_mods.list" "lib/modules/${KVER}/modules.dep")"
                [[ -n "${new_deps}" ]] || break
                printf '%s\n' "${new_deps}" | while IFS= read -r dep_rel; do
                    src="lib/modules/${KVER}/${dep_rel}"
                    if [[ -f "${src}" && ! -e "${INITRD_DIR}/${src}" ]]; then
                        mkdir -p "${INITRD_DIR}/$(dirname "${src}")"
                        cp -a "${src}" "${INITRD_DIR}/${src}"
                    fi
                done
            done
            rm -f "${WORK_DIR}/initrd_mods.list"
        fi
        for modmeta in "lib/modules/${KVER}"/modules.*; do
            [[ -e "${modmeta}" ]] || continue
            mkdir -p "${INITRD_DIR}/$(dirname "${modmeta}")"
            cp -a "${modmeta}" "${INITRD_DIR}/${modmeta}"
        done
    )
    depmod -b "${INITRD_DIR}" "${KVER}"
fi

install -m 0755 "${SCRIPT_DIR}/initramfs-init" "${INITRD_DIR}/init"

CPIO_BIN="cpio"
command -v cpio >/dev/null 2>&1 || CPIO_BIN="busybox cpio"
(
    cd "${INITRD_DIR}"
    find . -print0 | ${CPIO_BIN} --null -ov --format=newc 2>/dev/null | gzip -9 > "${OUT_DIR}/initramfs-${ARCH}.img"
)

FIRMWARE_PROFILE="${FIRMWARE_PROFILE_OVERRIDE:-slim}"
if [[ "${FIRMWARE_PROFILE}" == "slim" ]]; then
    echo "==> Applying slim image profile (pruning docs, apk cache & server/mobile-SoC firmware)"
    rm -rf "${ROOTFS_DIR}/usr/share/doc"/* "${ROOTFS_DIR}/usr/share/man"/* "${ROOTFS_DIR}/usr/share/info"/* "${ROOTFS_DIR}/var/cache/apk"/* 2>/dev/null || true
    if [[ -d "${ROOTFS_DIR}/lib/firmware" ]]; then
        fw_before="$(du -sh "${ROOTFS_DIR}/lib/firmware" 2>/dev/null | awk '{print $1}')"
        (
            cd "${ROOTFS_DIR}/lib/firmware"
            rm -rf \
                netronome mellanox liquidio qed dpaa2 cavium cxgb4 cxgb3 bnx2x bnx2 \
                qcom qca mrvl nxp cnm amphion rockchip meson imx cypress \
                ti-connectivity ti s5p* vpu* ctefx* ueagle-atm vxge \
                myri10ge* phanfw* tigon libertas rsi carl9170fw* 2>/dev/null || true
        )
        fw_after="$(du -sh "${ROOTFS_DIR}/lib/firmware" 2>/dev/null | awk '{print $1}')"
        echo "==> Firmware size: ${fw_before} -> ${fw_after}"
    fi
fi

SQUASH_IMG="${OUT_DIR}/unicore-rootfs-${ARCH}.squashfs"
echo "==> Packing immutable SquashFS root image: ${SQUASH_IMG}"
mksquashfs "${ROOTFS_DIR}" "${SQUASH_IMG}" -comp zstd -noappend

OS_VERSION="0.2.1"
if [[ -r "${ROOTFS_DIR}/etc/os-release" ]]; then
    parsed_ver="$(awk -F= '$1=="VERSION_ID"{gsub(/["'\'']/, "", $2); print $2; exit}' "${ROOTFS_DIR}/etc/os-release" 2>/dev/null || true)"
    [[ -n "${parsed_ver}" ]] && OS_VERSION="${parsed_ver}"
fi
MANIFEST_IMG="${OUT_DIR}/unicore-manifest.json"
echo "==> Generating signed-ready OTA manifest: ${MANIFEST_IMG}"
"${UNILOAD_BIN}" update generate-manifest \
    --version "${OS_VERSION}" \
    --channel stable \
    --arch "${ARCH}" \
    --vmlinuz "${OUT_DIR}/vmlinuz-${ARCH}" \
    --initramfs "${OUT_DIR}/initramfs-${ARCH}.img" \
    --squashfs "${SQUASH_IMG}" \
    --output "${MANIFEST_IMG}" >/dev/null

if [[ -n "${UPDATE_SIGN_KEY_OVERRIDE:-}" && -f "${UPDATE_SIGN_KEY_OVERRIDE}" ]]; then
    echo "==> Signing OTA manifest with Ed25519 release key"
    "${UNILOAD_BIN}" update sign-manifest "${MANIFEST_IMG}" --key "${UPDATE_SIGN_KEY_OVERRIDE}" --output "${MANIFEST_IMG}.sig" >/dev/null
fi

if command -v grub-mkrescue >/dev/null 2>&1 && command -v xorriso >/dev/null 2>&1; then
    ISO_DIR="${WORK_DIR}/iso_stage"
    ISO_IMG="${OUT_DIR}/unicore-os-${ARCH}.iso"
    echo "==> Building Hybrid BIOS+UEFI Bootable ISO (2005-present PCs): ${ISO_IMG}"
    rm -rf "${ISO_DIR}"
    mkdir -p "${ISO_DIR}/boot/grub" "${ISO_DIR}/unicore"
    cp "${OUT_DIR}/vmlinuz-${ARCH}" "${ISO_DIR}/boot/vmlinuz-${ARCH}"
    cp "${OUT_DIR}/initramfs-${ARCH}.img" "${ISO_DIR}/boot/initramfs-${ARCH}.img"
    ln -f "${SQUASH_IMG}" "${ISO_DIR}/unicore/unicore-rootfs-${ARCH}.squashfs" 2>/dev/null || cp "${SQUASH_IMG}" "${ISO_DIR}/unicore/unicore-rootfs-${ARCH}.squashfs"
    cp "${MANIFEST_IMG}" "${ISO_DIR}/unicore/unicore-manifest.json"
    if [[ -f "${MANIFEST_IMG}.sig" ]]; then
        cp "${MANIFEST_IMG}.sig" "${ISO_DIR}/unicore/unicore-manifest.json.sig"
    fi
    cat <<EOF > "${ISO_DIR}/boot/grub/grub.cfg"
set timeout=3
set default=0

menuentry "UniCore OS ${OS_VERSION} (Universal Desktop - 2005 to Present)" {
    linux /boot/vmlinuz-${ARCH} console=tty1 quiet
    initrd /boot/initramfs-${ARCH}.img
}

menuentry "UniCore OS ${OS_VERSION} (Safe / Legacy VESA & Serial Debug Mode)" {
    linux /boot/vmlinuz-${ARCH} console=tty1 console=ttyS0 nomodeset
    initrd /boot/initramfs-${ARCH}.img
}
EOF
    grub-mkrescue -o "${ISO_IMG}" "${ISO_DIR}" -- -volid UNICORE_OS >/dev/null
    (cd "${OUT_DIR}" && sha256sum "$(basename "${ISO_IMG}")" > "${ISO_IMG}.sha256")
    ls -lh "${ISO_IMG}" "${ISO_IMG}.sha256"
fi

echo "==> Build complete:"
ls -lh "${OUT_DIR}/vmlinuz-${ARCH}" "${OUT_DIR}/initramfs-${ARCH}.img" "${SQUASH_IMG}"
