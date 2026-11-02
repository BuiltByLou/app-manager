#!/usr/bin/env bash
# Builds the .deb for App Manager. The package is assembled from the release
# binary plus the desktop entry and icon in data/, so nothing outside this
# repository is needed.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
name="app-manager"
stage="target/deb/${name}_${version}_${arch}"

echo "Building ${name} ${version} (${arch})"
cargo build --release -p appmgr

if [ ! -x target/release/appmgr ]; then
    echo "missing target/release/appmgr" >&2
    exit 1
fi

rm -rf "$stage"
install -Dm755 target/release/appmgr "$stage/usr/bin/appmgr"
# Debug symbols are useless in the package and several times the binary size.
strip --strip-unneeded "$stage/usr/bin/appmgr"
install -Dm644 data/io.github.builtbylou.app_manager.desktop \
    "$stage/usr/share/applications/io.github.builtbylou.app_manager.desktop"
install -Dm644 data/io.github.builtbylou.app_manager.svg \
    "$stage/usr/share/icons/hicolor/scalable/apps/io.github.builtbylou.app_manager.svg"

# Runtime library dependencies, resolved from the paths the binary actually
# links. Paths are followed through /lib → /usr/lib so dpkg finds them, and
# diversion notes ("diversion by libc6 from: ...") are not packages.
depends="$(ldd target/release/appmgr \
    | awk '$2 == "=>" {print $3} $1 ~ /^\// {print $1}' \
    | while read -r path; do
        dpkg-query -S "$(readlink -f "$path")" 2>/dev/null | grep -v '^diversion ' || true
    done \
    | sed 's/:.*//' \
    | sort -u \
    | paste -sd, -)"

mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<EOF
Package: ${name}
Version: ${version}
Architecture: ${arch}
Maintainer: Ed <ed@localhost>
Section: admin
Priority: optional
Depends: ${depends}
Recommends: apt, dpkg, curl, sudo, flatpak, libglib2.0-bin
Installed-Size: $(du -sk "$stage/usr" | cut -f1)
Description: App manager for Linux desktops
 Lists the apps installed with apt, Flatpak and AppImage, removes them
 without touching shared libraries or orphans, offers to trash what was
 left behind, and installs or updates apps straight from a GitHub release.
EOF

cat > "$stage/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q /usr/share/icons/hicolor || true
fi
exit 0
EOF
chmod 755 "$stage/DEBIAN/postinst"

cat > "$stage/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
exit 0
EOF
chmod 755 "$stage/DEBIAN/postrm"

mkdir -p dist
out="dist/${name}_${version}_${arch}.deb"
dpkg-deb --build --root-owner-group "$stage" "$out" >/dev/null
echo "built $out"
dpkg-deb --info "$out" | sed -n '1,12p'
