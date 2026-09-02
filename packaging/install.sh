#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

INSTALL_BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
DATA_DIR="${HOME}/.local/share/edvige"
PIXMAPS_DIR="${HOME}/.local/share/pixmaps"
HICOLOR_DIR="${HOME}/.local/share/icons/hicolor"

show_help() {
    cat <<EOF
Edvige Mail Installation & Management Script

Usage:
  ./packaging/install.sh [OPTIONS]

Options:
  --enable           Automatically enable and start edvige.service after install
  --remove           Uninstall Edvige Mail (binaries, desktop entry, icons, and systemd service)
  --purge            Used with --remove to also delete user data (~/.local/share/edvige)
  --no-build         Skip cargo compilation and install existing release binaries
  -h, --help         Show this help message

Examples:
  ./packaging/install.sh            # Standard installation
  ./packaging/install.sh --enable   # Install and enable background sync daemon via systemd
  ./packaging/install.sh --remove   # Uninstall application files
EOF
}

do_install() {
    echo "=== Installing Edvige Mail ==="

    # 1. Build release binaries
    if [ "${NO_BUILD}" = true ]; then
        echo "-> Skipping build (--no-build specified)..."
        if [ ! -f "${ROOT_DIR}/target/release/edvige-daemon" ] || [ ! -f "${ROOT_DIR}/target/release/edvige-gui" ]; then
            echo "Error: Binaries not found in target/release/. Run without --no-build first." >&2
            exit 1
        fi
    else
        echo "-> Building release binaries with Cargo..."
        cd "${ROOT_DIR}"
        cargo build --release -p edvige-daemon -p edvige-gui
    fi

    # 2. Install binaries directly to ~/.local/bin
    mkdir -p "${INSTALL_BIN_DIR}"
    echo "-> Installing binaries directly to ${INSTALL_BIN_DIR}..."

    # If previous symlinks exist, remove them first so install writes direct binaries
    [ -L "${INSTALL_BIN_DIR}/edvige" ] && rm -f "${INSTALL_BIN_DIR}/edvige"
    [ -L "${INSTALL_BIN_DIR}/edvige-daemon" ] && rm -f "${INSTALL_BIN_DIR}/edvige-daemon"

    install -m 755 "${ROOT_DIR}/target/release/edvige-daemon" "${INSTALL_BIN_DIR}/edvige-daemon"
    install -m 755 "${ROOT_DIR}/target/release/edvige-gui" "${INSTALL_BIN_DIR}/edvige"

    # Clean up any leftover copies in ~/.cargo/bin
    rm -f "${HOME}/.cargo/bin/edvige-daemon" "${HOME}/.cargo/bin/edvige-gui" 2>/dev/null || true

    # 3. Install application icon to multiple standard locations for desktop environment compatibility
    echo "-> Installing application icons..."
    mkdir -p "${PIXMAPS_DIR}"
    cp -f "${SCRIPT_DIR}/edvige.png" "${PIXMAPS_DIR}/edvige.png"

    # Install to hicolor icon theme directories with resized variants if convert is available
    for size in 512 256 128 64 48 32; do
        target_dir="${HICOLOR_DIR}/${size}x${size}/apps"
        mkdir -p "${target_dir}"
        if command -v convert >/dev/null 2>&1; then
            convert "${SCRIPT_DIR}/edvige.png" -resize "${size}x${size}" "${target_dir}/edvige.png" 2>/dev/null || cp -f "${SCRIPT_DIR}/edvige.png" "${target_dir}/edvige.png"
        else
            cp -f "${SCRIPT_DIR}/edvige.png" "${target_dir}/edvige.png"
        fi
    done

    mkdir -p "${HICOLOR_DIR}/scalable/apps"
    cp -f "${SCRIPT_DIR}/edvige.png" "${HICOLOR_DIR}/scalable/apps/edvige.png"

    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${HICOLOR_DIR}" >/dev/null 2>&1 || true
    fi

    # 4. Install desktop entry (expand ~ to absolute path so XDG launchers can find it)
    mkdir -p "${DESKTOP_DIR}"
    echo "-> Installing desktop entry to ${DESKTOP_DIR}..."
    sed "s|~|${HOME}|g" "${SCRIPT_DIR}/edvige.desktop" > "${DESKTOP_DIR}/edvige.desktop"
    chmod 644 "${DESKTOP_DIR}/edvige.desktop"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${DESKTOP_DIR}" >/dev/null 2>&1 || true
    fi

    # 5. Install systemd user service: edvige.service
    mkdir -p "${SYSTEMD_USER_DIR}"
    echo "-> Installing systemd user unit: ${SYSTEMD_USER_DIR}/edvige.service..."
    install -m 644 "${SCRIPT_DIR}/edvige.service" "${SYSTEMD_USER_DIR}/edvige.service"

    # Clean up obsolete edvige-daemon.service if it exists
    if [ -f "${SYSTEMD_USER_DIR}/edvige-daemon.service" ]; then
        if command -v systemctl >/dev/null 2>&1; then
            systemctl --user stop edvige-daemon.service 2>/dev/null || true
            systemctl --user disable edvige-daemon.service 2>/dev/null || true
        fi
        rm -f "${SYSTEMD_USER_DIR}/edvige-daemon.service"
    fi

    # 6. Service lifecycle: reload and restart if requested or currently running
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user daemon-reload
        if [ "${ENABLE_SERVICE}" = true ]; then
            echo "-> Enabling and starting edvige.service via systemctl --user..."
            systemctl --user enable edvige.service
            systemctl --user restart edvige.service
            echo "-> edvige.service is active!"
        elif systemctl --user is-active --quiet edvige.service 2>/dev/null; then
            echo "-> Detected active edvige.service; restarting with updated binary..."
            systemctl --user restart edvige.service
            echo "-> edvige.service restarted with latest build!"
        fi
    fi

    echo ""
    echo "=== Installation / Update Complete! ==="
    echo "• Executables installed to: ${INSTALL_BIN_DIR}/edvige and ${INSTALL_BIN_DIR}/edvige-daemon"
    echo "• Service: edvige.service"
    echo "• Application icon: installed in pixmaps and hicolor theme directories"
    echo ""
    echo "You can launch Edvige from your desktop application launcher or run '${INSTALL_BIN_DIR}/edvige'."
    if [ "${ENABLE_SERVICE}" = false ] && command -v systemctl >/dev/null 2>&1 && ! systemctl --user is-active --quiet edvige.service 2>/dev/null; then
        echo ""
        echo "To enable the background sync daemon to start automatically on login:"
        echo "  systemctl --user daemon-reload"
        echo "  systemctl --user enable --now edvige.service"
    fi
}

do_remove() {
    echo "=== Uninstalling Edvige Mail ==="

    # 1. Stop and disable systemd services if active
    if command -v systemctl >/dev/null 2>&1; then
        for s in edvige.service edvige-daemon.service; do
            if systemctl --user is-active --quiet "${s}" 2>/dev/null; then
                echo "-> Stopping ${s}..."
                systemctl --user stop "${s}" || true
            fi
            if systemctl --user is-enabled --quiet "${s}" 2>/dev/null; then
                echo "-> Disabling ${s}..."
                systemctl --user disable "${s}" || true
            fi
        done
    fi

    # 2. Remove systemd service unit
    for s in edvige.service edvige-daemon.service; do
        if [ -f "${SYSTEMD_USER_DIR}/${s}" ]; then
            echo "-> Removing ${SYSTEMD_USER_DIR}/${s}..."
            rm -f "${SYSTEMD_USER_DIR}/${s}"
        fi
    done

    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user daemon-reload || true
    fi

    # 3. Remove desktop entry
    if [ -f "${DESKTOP_DIR}/edvige.desktop" ]; then
        echo "-> Removing desktop entry..."
        rm -f "${DESKTOP_DIR}/edvige.desktop"
        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "${DESKTOP_DIR}" >/dev/null 2>&1 || true
        fi
    fi

    # 4. Remove application icons
    echo "-> Removing application icons..."
    rm -f "${PIXMAPS_DIR}/edvige.png"
    for size in 512 256 128 64 48 32 scalable; do
        rm -f "${HICOLOR_DIR}/${size}/apps/edvige.png"
        rm -f "${HICOLOR_DIR}/${size}x${size}/apps/edvige.png"
    done

    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${HICOLOR_DIR}" >/dev/null 2>&1 || true
    fi

    # 5. Remove binaries
    echo "-> Removing binaries in ${INSTALL_BIN_DIR}..."
    rm -f "${INSTALL_BIN_DIR}/edvige" "${INSTALL_BIN_DIR}/edvige-daemon" "${INSTALL_BIN_DIR}/edvige-gui"
    rm -f "${HOME}/.cargo/bin/edvige-daemon" "${HOME}/.cargo/bin/edvige-gui" 2>/dev/null || true

    # 6. Optional purge of data directory
    if [ "${PURGE_DATA}" = true ]; then
        if [ -d "${DATA_DIR}" ]; then
            echo "-> Purging user data directory (${DATA_DIR})..."
            rm -rf "${DATA_DIR}"
        fi
    else
        if [ -d "${DATA_DIR}" ]; then
            echo "Note: User data preserved at ${DATA_DIR}."
            echo "      To completely wipe data, run: ./packaging/install.sh --remove --purge"
        fi
    fi

    echo ""
    echo "=== Uninstallation Complete ==="
}

# --- CLI Argument Parsing ---
ACTION="install"
ENABLE_SERVICE=false
NO_BUILD=false
PURGE_DATA=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --enable)
            ENABLE_SERVICE=true
            shift
            ;;
        --remove|--uninstall)
            ACTION="remove"
            shift
            ;;
        --purge)
            PURGE_DATA=true
            shift
            ;;
        --no-build)
            NO_BUILD=true
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Error: Unknown argument '$1'" >&2
            echo "Run '$0 --help' for usage." >&2
            exit 1
            ;;
    esac
done

if [ "${ACTION}" = "remove" ]; then
    do_remove
else
    do_install
fi
