#!/bin/sh
# Adds AntAn to the application menu for the current user (no root needed).
# AntAn'ı o anki kullanıcı için uygulama menüsüne ekler (root gerekmez).
# Remove / kaldırmak için: ./install-desktop.sh --remove
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor/256x256/apps"

if [ "$1" = "--remove" ]; then
    rm -f "$APPS/antan.desktop" "$ICONS/antan.png"
    echo "AntAn removed from the application menu."
    exit 0
fi

mkdir -p "$APPS" "$ICONS"
cp "$DIR/antan.png" "$ICONS/antan.png"
sed "s|@EXEC@|\"$DIR/antan\"|" "$DIR/antan.desktop.in" > "$APPS/antan.desktop"
chmod +x "$APPS/antan.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
echo "AntAn added to the application menu (run '$0 --remove' to undo)."
